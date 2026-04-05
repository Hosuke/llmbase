"""Background worker — autonomous learning, compilation, and maintenance.

Runs alongside the web server. Periodically:
1. Ingests new content from configured sources (CBETA, etc.)
2. Compiles unprocessed raw documents into wiki
3. Rebuilds index
4. Runs health checks

Configure via config.yaml `worker:` section.
"""

import logging
import time
import threading
from datetime import datetime, timezone
from pathlib import Path

from .config import load_config, ensure_dirs

logger = logging.getLogger("llmbase.worker")


def run_worker(base_dir: Path | None = None):
    """Main worker loop — runs forever, executing scheduled tasks."""
    base = Path(base_dir) if base_dir else Path.cwd()
    cfg = load_config(base)
    ensure_dirs(cfg)

    worker_cfg = cfg.get("worker", {})
    if not worker_cfg.get("enabled", False):
        logger.info("Worker disabled in config. Set worker.enabled: true to activate.")
        return

    learn_interval = worker_cfg.get("learn_interval_hours", 6) * 3600
    compile_interval = worker_cfg.get("compile_interval_hours", 1) * 3600
    learn_batch = worker_cfg.get("learn_batch_size", 10)
    learn_source = worker_cfg.get("learn_source", "cbeta")

    taxonomy_interval = worker_cfg.get("taxonomy_interval_hours", 12) * 3600
    health_interval = worker_cfg.get("health_check_interval_hours", 24) * 3600

    logger.info(
        f"Worker started: learn every {learn_interval/3600:.0f}h, "
        f"compile every {compile_interval/3600:.0f}h, "
        f"health every {health_interval/3600:.0f}h"
    )

    last_learn = 0
    last_compile = 0
    last_taxonomy = 0
    last_health = 0

    while True:
        now = time.time()

        # Learn from sources
        if now - last_learn >= learn_interval:
            _task_learn(base, learn_source, learn_batch)
            last_learn = now

        # Compile new documents
        if now - last_compile >= compile_interval:
            _task_compile(base)
            last_compile = now

        # Regenerate taxonomy periodically
        if now - last_taxonomy >= taxonomy_interval:
            _task_taxonomy(base)
            last_taxonomy = now

        # Health checks and auto-repair
        if now - last_health >= health_interval:
            _task_health_check(base)
            last_health = now

        time.sleep(60)  # Check every minute


def _task_learn(base: Path, source: str, batch_size: int):
    """Ingest a batch from the configured source."""
    logger.info(f"[learn] Starting batch of {batch_size} from {source}")
    try:
        if source == "cbeta":
            from .cbeta import learn
            results = learn(batch_size=batch_size, base_dir=base)
            logger.info(f"[learn] CBETA: ingested {len(results)} new works")
        elif source == "wikisource":
            from .wikisource import learn
            results = learn(batch_size=batch_size, base_dir=base)
            logger.info(f"[learn] Wikisource: ingested {len(results)} new works: {results}")
        elif source == "both":
            from .cbeta import learn as cbeta_learn
            from .wikisource import learn as ws_learn
            r1 = cbeta_learn(batch_size=batch_size // 2, base_dir=base)
            r2 = ws_learn(batch_size=batch_size // 2, base_dir=base)
            logger.info(f"[learn] CBETA: {len(r1)}, Wikisource: {len(r2)}")
        else:
            logger.warning(f"[learn] Unknown source: {source}")
    except Exception as e:
        logger.error(f"[learn] Error: {e}")


def _task_compile(base: Path):
    """Compile any unprocessed raw documents."""
    logger.info("[compile] Checking for uncompiled documents...")
    try:
        from .compile import compile_new, rebuild_index
        articles = compile_new(base, batch_size=5)
        if articles:
            logger.info(f"[compile] Created {len(articles)} new articles")
            rebuild_index(base)
            logger.info("[compile] Index rebuilt")
        else:
            logger.debug("[compile] Nothing to compile")
    except Exception as e:
        logger.error(f"[compile] Error: {e}")


def _task_taxonomy(base: Path):
    """Regenerate taxonomy from current articles (unless locked)."""
    from .taxonomy import load_taxonomy, generate_taxonomy

    # Respect locked taxonomy — don't overwrite manually curated categories
    existing = load_taxonomy(base)
    if existing.get("locked"):
        logger.info("[taxonomy] Taxonomy is locked, skipping regeneration")
        return

    logger.info("[taxonomy] Regenerating category taxonomy...")
    try:
        taxonomy = generate_taxonomy(base)
        cats = len(taxonomy.get("categories", []))
        logger.info(f"[taxonomy] Generated {cats} categories")
    except Exception as e:
        logger.error(f"[taxonomy] Error: {e}")

    # Regenerate Xi Ci for all languages
    logger.info("[xici] Regenerating guided introductions...")
    try:
        from .xici import generate_xici
        for lang in ("zh", "en", "ja", "zh-en"):
            generate_xici(base, lang)
        logger.info("[xici] Generated Xi Ci for all languages")
    except Exception as e:
        logger.error(f"[xici] Error: {e}")


def _task_health_check(base: Path):
    """Run lint checks and auto-fix broken links."""
    logger.info("[health] Running health checks...")
    try:
        from .lint import lint, auto_fix
        import json

        # Run checks
        results = lint(base)
        total = results.get("total_issues", 0)
        logger.info(f"[health] Found {total} issues")

        # Auto-fix
        fixes = []
        if total > 0:
            fixes = auto_fix(base)
            logger.info(f"[health] Applied {len(fixes)} fixes")

        # Persist health report
        _save_health_report(base, results, fixes)

    except Exception as e:
        logger.error(f"[health] Error: {e}")


def _save_health_report(base: Path, results: dict, fixes: list[str]):
    """Save health check results to wiki/_meta/health.json."""
    import json

    cfg = load_config(base)
    meta_dir = Path(cfg["paths"]["meta"])
    meta_dir.mkdir(parents=True, exist_ok=True)

    report = {
        "checked_at": datetime.now(timezone.utc).isoformat(),
        "results": results,
        "fixes_applied": fixes,
    }
    health_path = meta_dir / "health.json"
    health_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    logger.info(f"[health] Report saved to {health_path}")


def start_worker_thread(base_dir: Path | None = None):
    """Start worker as a background daemon thread."""
    t = threading.Thread(target=run_worker, args=(base_dir,), daemon=True)
    t.start()
    return t
