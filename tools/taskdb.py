"""Task database -- SQLite-backed progress tracking for the learning pipeline.

Replaces per-plugin JSON progress files with a unified, queryable task queue.
Survives redeployments because the DB lives in the persistent data directory.

Usage:
    from .taskdb import get_db, claim_task, complete_task, fail_task

The DB file is placed at <data_dir>/llmbase.db where data_dir comes from
config.yaml paths.data (default: same as base_dir).
"""

import json
import logging
import sqlite3
import threading
from datetime import datetime, timezone, timedelta
from pathlib import Path

from .config import load_config

logger = logging.getLogger("llmbase.taskdb")

_local = threading.local()

SCHEMA_VERSION = 1

SCHEMA = """
CREATE TABLE IF NOT EXISTS tasks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    source       TEXT NOT NULL,
    item_key     TEXT NOT NULL,
    priority     INTEGER DEFAULT 3,
    status       TEXT DEFAULT 'queued',
    retries      INTEGER DEFAULT 0,
    max_retries  INTEGER DEFAULT 3,
    error_msg    TEXT,
    created_at   TEXT NOT NULL,
    started_at   TEXT,
    completed_at TEXT,
    UNIQUE(source, item_key)
);

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status, priority);
CREATE INDEX IF NOT EXISTS idx_tasks_source ON tasks(source, status);

CREATE TABLE IF NOT EXISTS source_health (
    source               TEXT PRIMARY KEY,
    last_success         TEXT,
    last_failure         TEXT,
    last_error           TEXT,
    consecutive_failures INTEGER DEFAULT 0,
    backoff_until        TEXT,
    total_successes      INTEGER DEFAULT 0,
    total_failures       INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS run_log (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    run_at           TEXT NOT NULL,
    strategy         TEXT,
    source           TEXT,
    items_attempted  INTEGER DEFAULT 0,
    items_succeeded  INTEGER DEFAULT 0,
    items_failed     INTEGER DEFAULT 0,
    duration_seconds REAL,
    error_msg        TEXT
);

CREATE TABLE IF NOT EXISTS worker_state (
    task_name   TEXT PRIMARY KEY,
    last_run_at TEXT,
    next_run_at TEXT
);

CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT
);
"""


def _db_path(base_dir: Path) -> Path:
    cfg = load_config(base_dir)
    data_dir = cfg.get("paths", {}).get("data")
    if data_dir:
        p = Path(data_dir)
    else:
        p = base_dir
    p.mkdir(parents=True, exist_ok=True)
    return p / "llmbase.db"


def get_db(base_dir: Path | str) -> sqlite3.Connection:
    """Get a thread-local SQLite connection with WAL mode."""
    base_dir = Path(base_dir).resolve()
    db_file = _db_path(base_dir)

    if not hasattr(_local, "connections"):
        _local.connections = {}

    key = str(db_file)
    if key in _local.connections:
        return _local.connections[key]

    conn = sqlite3.connect(str(db_file), timeout=10)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    conn.executescript(SCHEMA)

    # Set schema version
    conn.execute(
        "INSERT OR IGNORE INTO schema_meta(key, value) VALUES('version', ?)",
        (str(SCHEMA_VERSION),),
    )
    conn.commit()

    _local.connections[key] = conn
    logger.info(f"TaskDB opened: {db_file}")
    return conn


def _now() -> str:
    return datetime.now(timezone.utc).isoformat()


# --- Task lifecycle ----------------------------------------------------------


def get_completed_items(source: str, *, base_dir: Path) -> set[str]:
    """Return set of item_keys already completed for a source."""
    db = get_db(base_dir)
    rows = db.execute(
        "SELECT item_key FROM tasks WHERE source=? AND status IN ('completed', 'skipped')",
        (source,),
    ).fetchall()
    return {r["item_key"] for r in rows}


def claim_task(source: str, item_key: str, priority: int = 3, *, base_dir: Path) -> int:
    """Create or claim a task. Returns task id."""
    db = get_db(base_dir)
    now = _now()
    try:
        db.execute(
            """INSERT INTO tasks(source, item_key, priority, status, created_at, started_at)
               VALUES(?, ?, ?, 'in_progress', ?, ?)""",
            (source, item_key, priority, now, now),
        )
    except sqlite3.IntegrityError:
        # Task already exists -- update to in_progress
        db.execute(
            """UPDATE tasks SET status='in_progress', started_at=?, priority=?
               WHERE source=? AND item_key=? AND status IN ('queued', 'failed')""",
            (now, priority, source, item_key),
        )
    db.commit()
    row = db.execute(
        "SELECT id FROM tasks WHERE source=? AND item_key=?", (source, item_key)
    ).fetchone()
    return row["id"]


def complete_task(task_id: int, *, base_dir: Path):
    """Mark a task as completed."""
    db = get_db(base_dir)
    db.execute(
        "UPDATE tasks SET status='completed', completed_at=?, error_msg=NULL WHERE id=?",
        (_now(), task_id),
    )
    db.commit()


def fail_task(task_id: int, error_msg: str, *, base_dir: Path):
    """Mark a task as failed. Auto-requeue if retries < max_retries."""
    db = get_db(base_dir)
    row = db.execute("SELECT retries, max_retries FROM tasks WHERE id=?", (task_id,)).fetchone()
    if not row:
        return

    new_retries = row["retries"] + 1
    if new_retries < row["max_retries"]:
        status = "queued"
    else:
        status = "failed"

    db.execute(
        "UPDATE tasks SET status=?, retries=?, error_msg=?, completed_at=? WHERE id=?",
        (status, new_retries, error_msg, _now(), task_id),
    )
    db.commit()


def get_queued_tasks(source: str | None = None, limit: int = 10, *, base_dir: Path) -> list[dict]:
    """Get next tasks to process, ordered by priority."""
    db = get_db(base_dir)
    if source:
        rows = db.execute(
            "SELECT * FROM tasks WHERE source=? AND status='queued' ORDER BY priority, created_at LIMIT ?",
            (source, limit),
        ).fetchall()
    else:
        rows = db.execute(
            "SELECT * FROM tasks WHERE status='queued' ORDER BY priority, created_at LIMIT ?",
            (limit,),
        ).fetchall()
    return [dict(r) for r in rows]


def enqueue_task(source: str, item_key: str, priority: int = 3, *, base_dir: Path):
    """Add a task to the queue (idempotent -- skips if already exists)."""
    db = get_db(base_dir)
    try:
        db.execute(
            "INSERT INTO tasks(source, item_key, priority, status, created_at) VALUES(?, ?, ?, 'queued', ?)",
            (source, item_key, priority, _now()),
        )
        db.commit()
    except sqlite3.IntegrityError:
        pass  # Already exists


# --- Source health -----------------------------------------------------------


def get_source_health(source: str, *, base_dir: Path) -> dict:
    db = get_db(base_dir)
    row = db.execute("SELECT * FROM source_health WHERE source=?", (source,)).fetchone()
    if row:
        return dict(row)
    return {
        "source": source,
        "last_success": None,
        "last_failure": None,
        "last_error": None,
        "consecutive_failures": 0,
        "backoff_until": None,
        "total_successes": 0,
        "total_failures": 0,
    }


def update_source_health(source: str, success: bool, error_msg: str | None = None, *, base_dir: Path):
    """Update health tracking for a source after a run."""
    db = get_db(base_dir)
    now = _now()
    health = get_source_health(source, base_dir=base_dir)

    if success:
        db.execute(
            """INSERT INTO source_health(source, last_success, consecutive_failures, total_successes)
               VALUES(?, ?, 0, 1)
               ON CONFLICT(source) DO UPDATE SET
                 last_success=?, consecutive_failures=0,
                 total_successes=total_successes+1, backoff_until=NULL""",
            (source, now, now),
        )
    else:
        failures = health["consecutive_failures"] + 1
        # Exponential backoff: 5min, 10min, 20min, ... capped at 24h
        backoff_secs = min(86400, 300 * (2 ** (failures - 1)))
        backoff_until = (datetime.now(timezone.utc) + timedelta(seconds=backoff_secs)).isoformat()

        db.execute(
            """INSERT INTO source_health(source, last_failure, last_error, consecutive_failures,
                 total_failures, backoff_until)
               VALUES(?, ?, ?, ?, 1, ?)
               ON CONFLICT(source) DO UPDATE SET
                 last_failure=?, last_error=?, consecutive_failures=?,
                 total_failures=total_failures+1, backoff_until=?""",
            (source, now, error_msg, failures, backoff_until,
             now, error_msg, failures, backoff_until),
        )
    db.commit()


def should_skip_source(source: str, *, base_dir: Path) -> bool:
    """Check if a source is in backoff period."""
    health = get_source_health(source, base_dir=base_dir)
    backoff = health.get("backoff_until")
    if not backoff:
        return False
    return datetime.now(timezone.utc) < datetime.fromisoformat(backoff)


# --- Run log ----------------------------------------------------------------


def log_run(
    strategy: str,
    source: str,
    items_attempted: int,
    items_succeeded: int,
    items_failed: int,
    duration_seconds: float,
    error_msg: str | None = None,
    *,
    base_dir: Path,
):
    db = get_db(base_dir)
    db.execute(
        """INSERT INTO run_log(run_at, strategy, source, items_attempted, items_succeeded,
             items_failed, duration_seconds, error_msg)
           VALUES(?, ?, ?, ?, ?, ?, ?, ?)""",
        (_now(), strategy, source, items_attempted, items_succeeded, items_failed, duration_seconds, error_msg),
    )
    db.commit()


# --- Worker state ------------------------------------------------------------


def get_worker_state(task_name: str, *, base_dir: Path) -> dict | None:
    db = get_db(base_dir)
    row = db.execute("SELECT * FROM worker_state WHERE task_name=?", (task_name,)).fetchone()
    return dict(row) if row else None


def set_worker_state(task_name: str, last_run_at: str, next_run_at: str, *, base_dir: Path):
    db = get_db(base_dir)
    db.execute(
        """INSERT INTO worker_state(task_name, last_run_at, next_run_at) VALUES(?, ?, ?)
           ON CONFLICT(task_name) DO UPDATE SET last_run_at=?, next_run_at=?""",
        (task_name, last_run_at, next_run_at, last_run_at, next_run_at),
    )
    db.commit()


# --- Stats (for API) --------------------------------------------------------


def get_task_stats(*, base_dir: Path) -> dict:
    """Get aggregate task statistics for the worker status API."""
    db = get_db(base_dir)

    # Task counts by status
    status_counts = {}
    for row in db.execute("SELECT status, count(*) as cnt FROM tasks GROUP BY status").fetchall():
        status_counts[row["status"]] = row["cnt"]

    # Per-source stats
    sources = {}
    for row in db.execute(
        """SELECT source, status, count(*) as cnt FROM tasks GROUP BY source, status"""
    ).fetchall():
        src = row["source"]
        if src not in sources:
            sources[src] = {"completed": 0, "queued": 0, "failed": 0, "in_progress": 0}
        sources[src][row["status"]] = row["cnt"]

    # Merge health info
    for row in db.execute("SELECT * FROM source_health").fetchall():
        src = row["source"]
        if src not in sources:
            sources[src] = {"completed": 0, "queued": 0, "failed": 0, "in_progress": 0}
        sources[src]["last_success"] = row["last_success"]
        sources[src]["health"] = "backoff" if should_skip_source(src, base_dir=base_dir) else "ok"

    # Recent runs
    recent_runs = [
        dict(r)
        for r in db.execute(
            "SELECT * FROM run_log ORDER BY run_at DESC LIMIT 10"
        ).fetchall()
    ]

    # Worker schedule
    worker = {}
    for row in db.execute("SELECT * FROM worker_state").fetchall():
        worker[row["task_name"]] = {
            "last_run": row["last_run_at"],
            "next_run": row["next_run_at"],
        }

    # Failed tasks detail
    failed_tasks = [
        dict(r)
        for r in db.execute(
            "SELECT source, item_key, error_msg, retries FROM tasks WHERE status='failed' LIMIT 20"
        ).fetchall()
    ]

    return {
        "tasks": status_counts,
        "sources": sources,
        "recent_runs": recent_runs,
        "worker": worker,
        "failed_tasks": failed_tasks,
    }


# --- Migration from JSON progress files -------------------------------------


def migrate_from_json(base_dir: Path, migrators: dict | None = None) -> dict:
    """One-time migration: read *_progress.json files and populate tasks table.

    Args:
        base_dir: Project root directory.
        migrators: Optional dict of {source_name: callable(meta_dir, base_dir) -> int}.
                   Each callable should read its progress file and call enqueue_task()
                   for each item, returning the count of migrated items.
                   If None, attempts auto-discovery of *_progress.json files.

    Returns a summary of what was migrated.
    """
    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    summary = {}

    if migrators:
        for source, func in migrators.items():
            count = func(meta_dir, base_dir)
            if count > 0:
                summary[source] = count
                logger.info(f"Migrated {count} tasks from {source}")
    else:
        # Auto-discover: read any *_progress.json and migrate completed items
        if meta_dir.exists():
            for progress_file in sorted(meta_dir.glob("*_progress.json")):
                source = progress_file.stem.replace("_progress", "")
                count = _migrate_generic(source, progress_file, base_dir)
                if count > 0:
                    summary[source] = count
                    logger.info(f"Migrated {count} tasks from {progress_file.name}")

    return summary


def _read_json(path: Path) -> dict:
    if path.exists():
        return json.loads(path.read_text())
    return {}


def _migrate_generic(source: str, progress_file: Path, base_dir: Path) -> int:
    """Generic migration: scan a progress JSON for list-valued fields and import them.

    Looks for fields whose names start with 'ingested_', 'browsed_', 'completed_',
    or 'processed_' and treats their values as completed item keys.
    """
    data = _read_json(progress_file)
    count = 0
    prefixes = ("ingested_", "browsed_", "completed_", "processed_")
    for field, items in data.items():
        if not isinstance(items, list):
            continue
        if not any(field.startswith(p) for p in prefixes):
            continue
        for item in items:
            key = str(item)
            enqueue_task(source, key, priority=3, base_dir=base_dir)
            db = get_db(base_dir)
            db.execute(
                "UPDATE tasks SET status='completed', completed_at=? WHERE source=? AND item_key=?",
                (_now(), source, key),
            )
            db.commit()
            count += 1
    return count
