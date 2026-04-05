"""Wiki-link alias resolution — maps any name to its canonical slug.

Articles have pinyin slugs (can-chan.md) but wiki-links use Chinese text
([[参禅]]). This module builds and queries an alias map so that any
known name (Chinese title, English title, pinyin slug, traditional
variant) resolves to the correct article.

Usage:
    from .resolve import load_aliases, resolve_link

    aliases = load_aliases(meta_dir)
    slug = resolve_link("参禅", aliases)  # → "can-chan"
"""

import json
from pathlib import Path

import frontmatter


def build_aliases(concepts_dir: Path) -> dict[str, str]:
    """Build alias map from all article metadata.

    For each article, registers these aliases → canonical slug:
    - The slug itself (filename stem)
    - Each part of the title split by "/" (bilingual titles)
    - The full title as-is

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

    return aliases


def save_aliases(aliases: dict[str, str], meta_dir: Path):
    """Write aliases.json to the meta directory."""
    meta_dir.mkdir(parents=True, exist_ok=True)
    path = meta_dir / "aliases.json"
    path.write_text(json.dumps(aliases, indent=2, ensure_ascii=False), encoding="utf-8")


def load_aliases(meta_dir: Path) -> dict[str, str]:
    """Load aliases.json from the meta directory."""
    path = meta_dir / "aliases.json"
    if path.exists():
        return json.loads(path.read_text())
    return {}


def resolve_link(target: str, aliases: dict[str, str]) -> str | None:
    """Resolve a wiki-link target to a canonical slug.

    Tries multiple normalizations:
    1. Exact match (case-insensitive)
    2. With spaces → hyphens
    3. Stripped whitespace

    Returns the canonical slug or None if unresolvable.
    """
    if not target:
        return None

    key = _normalize(target)

    # Direct lookup
    if key in aliases:
        return aliases[key]

    # Try with spaces replaced by hyphens
    hyphenated = key.replace(" ", "-")
    if hyphenated in aliases:
        return aliases[hyphenated]

    # Try stripping all whitespace
    stripped = key.replace(" ", "")
    if stripped in aliases:
        return aliases[stripped]

    return None


def _normalize(text: str) -> str:
    """Normalize text for alias lookup: lowercase, strip."""
    return text.strip().lower()


def _register(aliases: dict[str, str], name: str, slug: str):
    """Register a name → slug mapping (normalized)."""
    key = _normalize(name)
    if key and key not in aliases:
        aliases[key] = slug
