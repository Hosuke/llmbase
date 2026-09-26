"""Append-only JSONL audit trail — **log is truth** (议 D, v0.7.7).

Every stage run writes its history here. Driver-written events drive
``state.rebuild_state``; handler-written events (via ``ctx.log``) are
free-form but must not collide with the reserved names.

Layout on disk::

    {base_dir}/.pipeline/{stage}/{sha256(key)}.jsonl

Each line is a complete JSON object with a mandatory ``ts`` (ISO-8601
UTC, added by ``append``) and ``event`` field. Lines are never
half-written: ``append`` takes ``fcntl.LOCK_EX`` on the fd before a
single buffered ``write()``, so concurrent writers serialize and a
crash mid-write leaves the next appender starting on a new offset
(the partial line remains at end-of-file and is skipped by
``iter_events``).

This module is package-internal. Downstream should go through
``run_stage()`` / ``StageContext.log()``, not here directly.
"""

from __future__ import annotations

import fcntl
import hashlib
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterator
from .. import _native


# Stage names become directory segments under ``.pipeline/``; any
# separator or traversal segment would let a malicious caller write
# outside the intended tree. Keys are always hashed so they don't need
# this check — but stage lands on disk verbatim for operator
# legibility, so it gets whitelisted instead.
_STAGE_RE = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_.\-]*$")


_KEY_HEX = 64
"""Full sha256 for the per-key filename.

Codex HIGH v0.7.7 round 7 reclassified short-prefix collision: two
unrelated ``(stage, key)`` pairs sharing one JSONL file is not just
spurious ``StageBusyError`` — it cross-contaminates their state
(``rebuild_state`` reads events from both keys as one history) and
their lock (one key's lock gates the other). That's semantic state
corruption, not a livelock, and it matches the precedent set by
``chunk_cache`` (where truncation was flagged as MEDIUM and
full-hash adopted in v0.7.5).

Full 64-hex puts collision probability effectively at zero across any
realistic key universe. Filesystem name limits (255 bytes on
ext4/APFS) are generous enough to swallow a 68-char filename
(``<64-hex>.jsonl``) without concern."""


def append(base_dir: Path, stage: str, key: str, event: dict) -> None:
    """Append one event to the ``(stage, key)`` log.

    A ``ts`` field (ISO-8601 UTC, microsecond precision) is always set
    by this function; any caller-supplied ``ts`` is overwritten —
    append timestamps are authoritative for ``rebuild_state``.

    Concurrency: ``fcntl.LOCK_EX`` on the fd ensures writers serialize.
    The full record is written in a single ``write()`` call so readers
    never observe an interleaved line. On crash before ``write()``
    completes, append is a no-op (the bytes were never flushed);
    on crash *after* ``write()`` but before ``fsync()``, the line is
    either fully visible or fully absent on next boot.
    """
    if _native.available():
        _log_path(base_dir, stage, key)  # Preserve public validation before the timestamp hook.
        _native.pipeline_dispatch("compat_pipeline_append", {
            "stage": stage, "key": key, "event": dict(event), "ts": _now_iso(),
        }, base_dir)
        return
    path = _log_path(base_dir, stage, key)
    path.parent.mkdir(parents=True, exist_ok=True)
    record = dict(event)
    record["ts"] = _now_iso()
    line = (json.dumps(record, ensure_ascii=False) + "\n").encode("utf-8")
    with open(path, "ab+") as f:
        fcntl.flock(f.fileno(), fcntl.LOCK_EX)
        try:
            # Heal a prior torn write: if the file has data and does
            # not end with "\n", a crashed appender left a partial
            # line. Emit a leading "\n" so our event starts cleanly on
            # its own line instead of being glued onto the partial.
            # The partial remains a malformed line — iter_events skips
            # it — but no valid line is ever lost. ``ab+`` gives a
            # readable fd in append mode; writes always land at EOF
            # regardless of the read cursor position (POSIX O_APPEND).
            f.seek(0, 2)  # end
            if f.tell() > 0:
                f.seek(-1, 2)
                if f.read(1) != b"\n":
                    f.write(b"\n")
            f.write(line)
            f.flush()
            os.fsync(f.fileno())
        finally:
            fcntl.flock(f.fileno(), fcntl.LOCK_UN)


def iter_events(base_dir: Path, stage: str, key: str) -> Iterator[dict]:
    """Yield every event in the log, oldest first.

    Malformed lines (torn writes from a crashed appender, or manual
    edits that broke JSON) are silently skipped — the JSONL format
    makes each line independently parseable, so one corrupt line does
    not poison the rest of the history. Typically the only affected
    line is the last one on disk (a mid-write crash). Callers that
    need to detect corruption should run a separate audit.
    """
    if _native.available():
        _log_path(base_dir, stage, key)
        yield from _native.pipeline_dispatch("compat_pipeline_events", {
            "stage": stage, "key": key,
        }, base_dir)
        return
    path = _log_path(base_dir, stage, key)
    if not path.exists():
        return
    # Read as bytes and decode per line: a torn write that truncates
    # a multi-byte UTF-8 sequence (common with ``ensure_ascii=False``
    # + CJK payloads) would raise ``UnicodeDecodeError`` under strict
    # text mode and abort iteration, breaking the crash-recovery
    # contract. Codex HIGH v0.7.7 round 8. Per-line ``try/except``
    # keeps one bad line from poisoning the rest of the history.
    with open(path, "rb") as f:
        for raw_line in f:
            try:
                line = raw_line.decode("utf-8")
            except UnicodeDecodeError:
                continue  # torn multi-byte or binary trash — skip
            s = line.strip()
            if not s:
                continue
            try:
                obj = json.loads(s)
            except json.JSONDecodeError:
                # Torn write or manual edit — skip. iter_events is
                # the contract surface for "survive bad lines"; if
                # you want strict parsing, use json.loads yourself.
                continue
            if not isinstance(obj, dict):
                # A valid JSON non-object (list, string, number) in
                # the log — doesn't carry an ``event`` field so
                # rebuild_state can't interpret it. Silently drop
                # rather than blowing up reconstruction.
                continue
            yield obj


def tail(base_dir: Path, stage: str, key: str, limit: int = 50) -> list[dict]:
    """Return the last ``limit`` events (most-recent last)."""
    if limit <= 0:
        return []
    if _native.available() and iter_events is _DEFAULT_ITER_EVENTS:
        _log_path(base_dir, stage, key)
        return _native.pipeline_dispatch("compat_pipeline_tail", {
            "stage": stage, "key": key, "limit": limit,
        }, base_dir)
    _native.clear_trace()
    events: list[dict] = []
    for e in iter_events(base_dir, stage, key):
        events.append(e)
        if len(events) > limit:
            events.pop(0)
    return events


def log_path(base_dir: Path, stage: str, key: str) -> Path:
    """Public accessor for the on-disk log path (read-only use)."""
    return _log_path(base_dir, stage, key)


# ── internals ─────────────────────────────────────────────────────────

def _log_path(base_dir: Path, stage: str, key: str) -> Path:
    if not isinstance(stage, str) or not _STAGE_RE.match(stage):
        raise ValueError(
            f"invalid stage name {stage!r}: must match "
            f"[A-Za-z0-9_][A-Za-z0-9_.-]* (no path separators, no traversal)"
        )
    if stage in (".", ".."):
        raise ValueError(f"invalid stage name {stage!r}: reserved")
    return Path(base_dir) / ".pipeline" / stage / f"{_key_hash(key)}.jsonl"


def _key_hash(key: str) -> str:
    return hashlib.sha256(key.encode("utf-8")).hexdigest()[:_KEY_HEX]


def _now_iso() -> str:
    """Monkeypatch target for tests needing deterministic timestamps."""
    return datetime.now(timezone.utc).isoformat()


_DEFAULT_ITER_EVENTS = iter_events
