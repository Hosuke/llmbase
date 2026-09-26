"""Content-hash-validated cache for pipeline chunks (议 C, v0.7.5).

Why: downstream wenguan-style pipelines split a source into chunks,
hand each chunk to an LLM, and cache the LLM's output keyed by the
chunk's slot position (``chunks/{idx:02d}.md`` in siwen's original).
When chunk boundaries shift (e.g. splitter rule is tuned, source is
re-OCR'd), the *same* slot now holds *different* content — but the
positional key still resolves, and the cache happily returns the
*old* output. Result: the downstream pipeline stitches together
LLM outputs for chunks that no longer exist. In production at
siwen (2026-04-20) this corrupted 3 books in a single run before
being noticed.

Fix: key by ``(cid, content_hash)`` instead of ``cid`` alone. On
read, *both* must match; a boundary change produces a new
``content_hash`` so the cache correctly misses and forces recompute.

Upstream contract (deliberately minimal — no stage/domain knowledge):

    cache = ChunkCache(base_dir)

    # LLM / expensive computation, guarded by the cache:
    cached = cache.get(cid, content_hash)
    if cached is not None:
        return cached
    output = expensive_call(chunk)
    cache.put(cid, content_hash, output)
    return output

    # Force recompute of a specific slot (e.g. "this chunk is wrong"):
    cache.clear(cid)

``cid``: downstream-chosen string identifying the slot (siwen wenguan:
the chunk's H3 title; line-range pipelines: ``"L{start}-L{end}"``).
``content_hash``: downstream-chosen string identifying the slot's
current content (typically ``hashlib.sha256(chunk_text).hexdigest()``,
but any stable per-content fingerprint works — the cache treats it
as opaque).

Upstream does not compute the content_hash itself: different pipelines
hash different things (raw bytes vs. normalized text vs. structured
dict), and baking in one choice would bind downstream to it. This
parallels the rule-pack / hook pattern used by ``normalize_heads``.
"""

from __future__ import annotations

import hashlib
import shutil
from pathlib import Path

from .atomic import atomic_write_text
from . import _native

_DEFAULT_ATOMIC_WRITE = atomic_write_text
_DEFAULT_RMTREE = shutil.rmtree


class ChunkCache:
    """File-backed (cid, content_hash) → output cache.

    Layout on disk::

        <base>/<subdir>/<cid_slot>/<content_slot>.md

    where ``cid_slot`` and ``content_slot`` are sha256 prefixes of
    ``cid`` and ``content_hash`` respectively. Two-level layout lets
    ``clear(cid)`` be a single ``rmtree`` without enumerating all
    stored content hashes for that cid.

    Writes are atomic (``atomic_write_text`` → tempfile + POSIX
    rename). Concurrent readers either see the previous value or the
    new one, never a partial write. Concurrent ``clear`` + ``put`` on
    the same cid may race such that ``put`` writes into a recreated
    dir after ``clear`` — this is benign: the result is one cached
    entry for the new hash, which is what a ``put`` after ``clear``
    would produce anyway.
    """

    # Full sha256 for both the cid directory and the content-hash
    # filename. Truncating either (the original design used 16 and 32
    # hex) creates a bounded-collision window where distinct cids (or
    # distinct content hashes) can resolve to the same file — a hit
    # would then return the wrong payload, which defeats the *whole
    # point* of this cache. Codex review v0.7.5 flagged truncation as
    # MEDIUM; 64-hex full hashes are well within filesystem name
    # limits (255 bytes on ext4/APFS) and cost nothing.
    _CID_HEX = 64
    _CONTENT_HEX = 64

    def __init__(self, base: Path | str, subdir: str = ".chunk_cache"):
        self._root = Path(base) / subdir

    @property
    def root(self) -> Path:
        """On-disk cache directory (readable; subject to changes)."""
        return self._root

    def get(self, cid: str, content_hash: str) -> str | None:
        """Return the cached output for ``(cid, content_hash)``, or
        ``None`` if either the cid has never been written or the
        content hash does not match (i.e. the content has changed at
        that slot — boundary moved or input re-ingested)."""
        if _native.available():
            value = _native.pipeline_dispatch("compat_pipeline_cache_get", {
                "subdir": ".", "cid": cid, "content_hash": content_hash,
            }, self._root)
            if "invalid_utf8" in value:
                data = bytes(value["invalid_utf8"])
                start = value["start"]
                end = len(data) if value["length"] is None else start + value["length"]
                raise UnicodeDecodeError("utf-8", data, start, end, "invalid UTF-8 cache content")
            return value["text"] if value["hit"] else None
        p = self._key_path(cid, content_hash)
        try:
            return p.read_text(encoding="utf-8")
        except FileNotFoundError:
            return None
        except OSError:
            # Transient I/O error — treat as miss so the caller
            # recomputes rather than surfacing a disk glitch as a
            # pipeline failure.
            return None

    def put(self, cid: str, content_hash: str, output: str) -> None:
        """Store ``output`` under ``(cid, content_hash)``. Atomic write
        — concurrent readers see either the previous value at that key
        or the new one, never a partial file."""
        if not isinstance(output, str):
            raise TypeError(
                f"ChunkCache.put: output must be str, got {type(output).__name__}"
            )
        if _native.available() and atomic_write_text is _DEFAULT_ATOMIC_WRITE:
            _native.pipeline_dispatch("compat_pipeline_cache_put", {
                "subdir": ".", "cid": cid, "content_hash": content_hash, "output": output,
            }, self._root)
            return
        _native.clear_trace()  # An explicitly replaced writer is a live Python boundary.
        atomic_write_text(self._key_path(cid, content_hash), output)

    def clear(self, cid: str) -> None:
        """Remove every entry stored for ``cid`` (across all historical
        content hashes). Idempotent on unknown cids (``FileNotFoundError``
        is treated as already-cleared); other errors — permission
        denied, I/O failure — propagate so the caller can detect that
        the cache may still hold stale entries.

        Use when you want to force recompute for a slot — typically
        during debugging or after an LLM prompt change that
        invalidates previously cached outputs. For a full reset,
        remove the cache root externally; this method is intentionally
        scoped to one cid to avoid accidental wipe."""
        if _native.available() and shutil.rmtree is _DEFAULT_RMTREE:
            _native.pipeline_dispatch("compat_pipeline_cache_clear", {
                "subdir": ".", "cid": cid,
            }, self._root)
            return
        _native.clear_trace()
        d = self._cid_dir(cid)
        try:
            shutil.rmtree(d)
        except FileNotFoundError:
            return  # already absent — clear is idempotent here

    # ── internals ──────────────────────────────────────────────────

    def _cid_dir(self, cid: str) -> Path:
        return self._root / _h(cid, self._CID_HEX)

    def _key_path(self, cid: str, content_hash: str) -> Path:
        return self._cid_dir(cid) / f"{_h(content_hash, self._CONTENT_HEX)}.md"


def _h(s: str, length: int) -> str:
    """Short sha256 prefix of *s* — used for filesystem-safe key encoding."""
    return hashlib.sha256(s.encode("utf-8")).hexdigest()[:length]
