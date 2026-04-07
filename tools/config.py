"""Configuration loader."""

import os
from pathlib import Path

import yaml


def load_config(base_dir: Path | None = None) -> dict:
    """Load config.yaml from the project root."""
    if base_dir is None:
        base_dir = Path.cwd()
    config_path = base_dir / "config.yaml"
    if not config_path.exists():
        return _defaults()
    with open(config_path) as f:
        cfg = yaml.safe_load(f)
    # Resolve relative paths against base_dir
    for key in ("raw", "wiki", "outputs", "meta", "concepts", "data"):
        p = cfg.get("paths", {}).get(key)
        if p:
            cfg["paths"][key] = str((base_dir / p).resolve())
    return cfg


def _defaults() -> dict:
    base = Path.cwd()
    return {
        "llm": {"model": "claude-sonnet-4-6", "max_tokens": 8192},
        "paths": {
            "raw": str(base / "raw"),
            "wiki": str(base / "wiki"),
            "outputs": str(base / "wiki" / "outputs"),
            "meta": str(base / "wiki" / "_meta"),
            "concepts": str(base / "wiki" / "concepts"),
        },
        "compile": {"batch_size": 10, "backlinks": True},
        "search": {"port": 5555},
        "lint": {"web_search": False},
        "worker": {
            "enabled": False,
            "learn_interval_hours": 6,
            "compile_interval_hours": 1,
            "taxonomy_interval_hours": 12,
            "health_check_interval_hours": 24,
            "learn_batch_size": 10,
            "learn_source": "cbeta",
        },
        "health": {
            "auto_fix_broken_links": True,
            "max_stubs_per_run": 10,
        },
        "entities": {
            "enabled": False,
            "extract_interval_hours": 24,
        },
    }


def ensure_dirs(cfg: dict):
    """Create all configured directories if they don't exist."""
    for key in ("raw", "wiki", "outputs", "meta", "concepts"):
        Path(cfg["paths"][key]).mkdir(parents=True, exist_ok=True)
