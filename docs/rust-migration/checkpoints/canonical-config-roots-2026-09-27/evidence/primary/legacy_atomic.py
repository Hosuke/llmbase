"""Atomic file writes — prevents corruption from concurrent/interrupted writes.

Usage:
    from .atomic import atomic_write_json, atomic_write_text

    atomic_write_json(path, data)      # temp file + rename
    atomic_write_text(path, "content") # temp file + rename
"""

import json
import tempfile
from pathlib import Path


def atomic_write_json(path: Path, data: dict, ensure_ascii: bool = False):
    """Write JSON atomically using temp file + rename.

    If the process crashes mid-write, the original file is untouched.
    """
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)

    # Write to temp file in same directory (same filesystem for atomic rename)
    fd, tmp_path = tempfile.mkstemp(
        dir=str(path.parent), suffix=".tmp", prefix=f".{path.stem}_"
    )
    try:
        with open(fd, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=ensure_ascii)
        # Atomic rename
        Path(tmp_path).replace(path)
    except Exception:
        # Clean up temp file on failure
        try:
            Path(tmp_path).unlink()
        except OSError:
            pass
        raise


def atomic_write_text(path: Path, content: str):
    """Write UTF-8 text atomically using temp file + rename.

    Same POSIX-rename guarantee as ``atomic_write_json``: readers either
    see the previous file (or no file) or the fully written new file —
    never a partial write, regardless of crash or concurrent reader.
    """
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)

    fd, tmp_path = tempfile.mkstemp(
        dir=str(path.parent), suffix=".tmp", prefix=f".{path.stem}_"
    )
    try:
        with open(fd, "w", encoding="utf-8") as f:
            f.write(content)
        Path(tmp_path).replace(path)
    except Exception:
        try:
            Path(tmp_path).unlink()
        except OSError:
            pass
        raise
