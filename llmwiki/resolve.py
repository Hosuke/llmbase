"""Wiki-link alias resolution — maps any name to its canonical slug.

Articles have pinyin slugs (can-chan.md) but wiki-links use Chinese text
([[参禅]]). This module builds and queries an alias map so that any
known name (Chinese title, English title, pinyin slug, traditional/
simplified variant) resolves to the correct article.

Usage:
    from .resolve import load_aliases, resolve_link

    aliases = load_aliases(meta_dir)
    slug = resolve_link("参禅", aliases)  # → "can-chan"
    slug = resolve_link("參禪", aliases)  # → "can-chan" (traditional)
"""

import json
import re
from pathlib import Path

import frontmatter

# Lazy-loaded opencc converters
_t2s = None
_s2t = None


def _get_converters():
    """Lazy-load opencc simplified↔traditional converters."""
    global _t2s, _s2t
    if _t2s is None:
        try:
            from opencc import OpenCC
            _t2s = OpenCC('t2s')
            _s2t = OpenCC('s2t')
        except ImportError:
            _t2s = _s2t = False  # Mark as unavailable
    return _t2s, _s2t


def build_aliases(concepts_dir: Path) -> dict[str, str]:
    """Build alias map from all article metadata.

    For each article, registers these aliases → canonical slug:
    - The slug itself (filename stem)
    - Each part of the title split by "/" (bilingual titles)
    - The full title as-is
    - Simplified ↔ Traditional Chinese variants of all CJK names
    - merged_from slugs (from dedup history)

    All lookups are case-insensitive and whitespace-normalized.
    """
    aliases: dict[str, str] = {}

    if not concepts_dir.exists():
        return aliases

    for md_file in sorted(concepts_dir.glob("*.md")):
        slug = md_file.stem
        post = frontmatter.load(str(md_file))
        title = post.metadata.get("title", slug)

        # Register the slug itself
        _register(aliases, slug, slug)

        # Register the full title
        _register(aliases, title, slug)

        # Register each part of bilingual title "English / 中文"
        for part in title.split("/"):
            part = part.strip()
            if part:
                _register(aliases, part, slug)

        # Register merged_from aliases (from dedup merges)
        for old_slug in post.metadata.get("merged_from", []):
            _register(aliases, old_slug, slug)

    # Second pass: generate simplified ↔ traditional variants
    _register_cjk_variants(aliases)

    return aliases


def save_aliases(aliases: dict[str, str], meta_dir: Path):
    """Write aliases.json to the meta directory (atomic)."""
    from .atomic import atomic_write_json
    atomic_write_json(meta_dir / "aliases.json", aliases)


def load_aliases(meta_dir: Path) -> dict[str, str]:
    """Load aliases.json from the meta directory."""
    path = meta_dir / "aliases.json"
    if path.exists():
        return json.loads(path.read_text())
    return {}


def resolve_link(target: str, aliases: dict[str, str]) -> str | None:
    """Resolve a wiki-link target to a canonical slug.

    Resolution cascade:
    1. Exact match (case-insensitive)
    2. Spaces → hyphens
    3. Stripped whitespace
    4. Simplified/Traditional Chinese conversion
    5. Fuzzy: strip all punctuation and compare

    Returns the canonical slug or None if unresolvable.
    """
    if not target:
        return None

    key = _normalize(target)

    # 1. Direct lookup
    if key in aliases:
        return aliases[key]

    # 2. Spaces → hyphens
    hyphenated = key.replace(" ", "-")
    if hyphenated in aliases:
        return aliases[hyphenated]

    # 3. Stripped whitespace
    stripped = key.replace(" ", "")
    if stripped in aliases:
        return aliases[stripped]

    # 4. Try simplified/traditional conversion
    t2s, s2t = _get_converters()
    if t2s and t2s is not False:
        simplified = _normalize(t2s.convert(target))
        if simplified in aliases:
            return aliases[simplified]
        traditional = _normalize(s2t.convert(target))
        if traditional in aliases:
            return aliases[traditional]

    # 5. Fuzzy: strip all non-alphanumeric/CJK and compare
    fuzzy_key = _fuzzy_normalize(target)
    for alias_key, alias_slug in aliases.items():
        if _fuzzy_normalize(alias_key) == fuzzy_key:
            return alias_slug

    return None


def _normalize(text: str) -> str:
    """Normalize text for alias lookup: lowercase, strip."""
    return text.strip().lower()


def _fuzzy_normalize(text: str) -> str:
    """Aggressive normalization: remove punctuation, spaces, stopwords, case."""
    t = re.sub(r'[^\w\u4e00-\u9fff\u3400-\u4dbf]', '', text.strip().lower())
    # Remove English articles/prepositions that cause false mismatches
    for stop in ('the', 'of', 'in', 'on', 'and', 'for', 'its'):
        t = t.replace(stop, '')
    return t


def _register(aliases: dict[str, str], name: str, slug: str):
    """Register a name → slug mapping (normalized)."""
    key = _normalize(name)
    if key and key not in aliases:
        aliases[key] = slug


def _register_cjk_variants(aliases: dict[str, str]):
    """For every CJK key, register its simplified ↔ traditional variant."""
    t2s, s2t = _get_converters()
    if not t2s or t2s is False:
        return

    cjk_pattern = re.compile(r'[\u4e00-\u9fff\u3400-\u4dbf]')
    new_entries: dict[str, str] = {}

    for key, slug in aliases.items():
        if not cjk_pattern.search(key):
            continue
        # Generate both variants
        simplified = _normalize(t2s.convert(key))
        traditional = _normalize(s2t.convert(key))
        if simplified and simplified not in aliases:
            new_entries[simplified] = slug
        if traditional and traditional not in aliases:
            new_entries[traditional] = slug

    aliases.update(new_entries)


# The installed candidate delegates default alias work to Rust. The original
# functions remain solely for an explicitly unbuilt source checkout.
_legacy_build_aliases = build_aliases
_legacy_load_aliases = load_aliases
_legacy_resolve_link = resolve_link
_legacy_normalize = _normalize
_legacy_fuzzy_normalize = _fuzzy_normalize
_legacy_register = _register
_legacy_register_cjk_variants = _register_cjk_variants
_legacy_get_converters = _get_converters


class _NativeConverter:
    def __init__(self, traditional):
        self.traditional = traditional

    def convert(self, text):
        from . import _native
        return _native.resolve_transform("convert", {"text": text, "traditional": self.traditional})


def _get_converters():
    from . import _native
    if not _native.available():
        return _legacy_get_converters()
    global _t2s, _s2t
    if _t2s is None:
        _t2s, _s2t = _NativeConverter(False), _NativeConverter(True)
    return _t2s, _s2t


def _normalize(text: str) -> str:
    from . import _native
    if not _native.available():
        return _legacy_normalize(text)
    return _native.resolve_transform("normalize", {"text": text})


def _fuzzy_normalize(text: str) -> str:
    from . import _native
    if not _native.available():
        return _legacy_fuzzy_normalize(text)
    return _native.resolve_transform("fuzzy", {"text": text})


def _register(aliases: dict[str, str], name: str, slug: str):
    from . import _native
    if not _native.available():
        return _legacy_register(aliases, name, slug)
    pairs = _native.resolve_transform("register", {"aliases": list(aliases.items()), "name": name, "slug": slug})
    aliases.update(pairs)


def _register_cjk_variants(aliases: dict[str, str]):
    from . import _native
    if not _native.available():
        return _legacy_register_cjk_variants(aliases)
    t2s, s2t = _get_converters()
    if not t2s:
        return
    if not isinstance(t2s, _NativeConverter) or not isinstance(s2t, _NativeConverter):
        # Explicitly replaced external converters are trusted live callbacks.
        value = _legacy_register_cjk_variants(aliases)
        _native.python_boundary("resolve_variants", "configured CJK converters")
        return value
    aliases.update(_native.resolve_transform("variants", {"aliases": list(aliases.items())}))


def build_aliases(concepts_dir: Path) -> dict[str, str]:
    from . import _native
    if not _native.available():
        return _legacy_build_aliases(concepts_dir)
    aliases = dict(_native.resolve_transform("build", {"concepts_dir": str(concepts_dir), "cjk": False}))
    _register_cjk_variants(aliases)
    return aliases


def load_aliases(meta_dir: Path) -> dict[str, str]:
    from . import _native
    if not _native.available():
        return _legacy_load_aliases(meta_dir)
    return dict(_native.resolve_transform("load", {"meta_dir": str(meta_dir)}))


def resolve_link(target: str, aliases: dict[str, str]) -> str | None:
    from . import _native
    if not _native.available():
        return _legacy_resolve_link(target, aliases)
    args = {"target": target, "aliases": list(aliases.items())}
    initial = _native.resolve_transform("resolve_initial", args)
    if initial["found"]:
        return initial["value"]
    conversions = []
    if target:
        t2s, s2t = _get_converters()
        if t2s:
            conversions = [t2s.convert(target), s2t.convert(target)]
    args["conversions"] = conversions
    return _native.resolve_transform("resolve", args)
