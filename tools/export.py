"""Structured export — clean data for downstream projects.

Provides semantic export functions that resolve relationships,
split trilingual content, and package everything a downstream
project (like Nuwa) needs.
"""

import json
import re
from pathlib import Path

import frontmatter

from .config import load_config, ensure_dirs
from .compile import _split_sections


def export_article(slug: str, base_dir: Path | None = None) -> dict | None:
    """Export a single article with full context.

    Returns resolved backlinks, outgoing links, sources, related articles,
    and content split by language section.
    """
    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    # Resolve slug via aliases
    from .resolve import load_aliases, resolve_link
    aliases = load_aliases(meta_dir)
    resolved = resolve_link(slug, aliases)
    if resolved:
        slug = resolved

    article_path = (concepts_dir / f"{slug}.md").resolve()
    if not str(article_path).startswith(str(concepts_dir.resolve()) + "/"):
        return None  # Path traversal guard
    if not article_path.exists():
        return None

    post = frontmatter.load(str(article_path))

    # Split content by language
    sections = _split_sections(post.content)
    content = {}
    for key, header in [("english", "english"), ("zh", "中文"), ("ja", "日本語")]:
        section = sections.get(header, "").strip()
        if section:
            content[key] = section

    # Outgoing wiki-links
    link_pattern = re.compile(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")
    outgoing = []
    seen_slugs = set()
    for match in link_pattern.finditer(post.content):
        target = match.group(1).strip()
        target_slug = resolve_link(target, aliases) or target.lower().replace(" ", "-")
        if target_slug not in seen_slugs:
            seen_slugs.add(target_slug)
            target_path = concepts_dir / f"{target_slug}.md"
            if target_path.exists():
                tp = frontmatter.load(str(target_path))
                outgoing.append({"slug": target_slug, "title": tp.metadata.get("title", target_slug)})

    # Backlinks
    backlinks = []
    bl_path = meta_dir / "backlinks.json"
    if bl_path.exists():
        try:
            bl_data = json.loads(bl_path.read_text())
            for bl_slug in bl_data.get(slug, []):
                bl_article = concepts_dir / f"{bl_slug}.md"
                if bl_article.exists():
                    bp = frontmatter.load(str(bl_article))
                    backlinks.append({"slug": bl_slug, "title": bp.metadata.get("title", bl_slug)})
        except (json.JSONDecodeError, OSError):
            pass

    # Related by tags
    tags = set(t.lower() for t in post.metadata.get("tags", []) if not t.startswith("category:"))
    related = []
    if tags:
        for md_file in concepts_dir.glob("*.md"):
            if md_file.stem == slug:
                continue
            other = frontmatter.load(str(md_file))
            other_tags = set(t.lower() for t in other.metadata.get("tags", []) if not t.startswith("category:"))
            shared = tags & other_tags
            if shared:
                related.append({
                    "slug": md_file.stem,
                    "title": other.metadata.get("title", md_file.stem),
                    "shared_tags": sorted(shared),
                })
        related.sort(key=lambda x: -len(x["shared_tags"]))

    return {
        "slug": slug,
        "title": post.metadata.get("title", slug),
        "summary": post.metadata.get("summary", ""),
        "tags": [t for t in post.metadata.get("tags", []) if not t.startswith("category:")],
        "sources": post.metadata.get("sources", []),
        "created": post.metadata.get("created", ""),
        "updated": post.metadata.get("updated", ""),
        "content": content,
        "backlinks": backlinks,
        "outgoing_links": outgoing,
        "related_by_tags": related[:10],
    }


def export_by_tag(tag: str, base_dir: Path | None = None) -> dict:
    """Export all articles with a given tag."""
    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])

    articles = []
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        tags = [t.lower() for t in post.metadata.get("tags", [])]
        if tag.lower() in tags:
            articles.append({
                "slug": md_file.stem,
                "title": post.metadata.get("title", md_file.stem),
                "summary": post.metadata.get("summary", ""),
                "tags": post.metadata.get("tags", []),
                "sources": post.metadata.get("sources", []),
            })

    return {"tag": tag, "count": len(articles), "articles": articles}


def export_graph(slug: str, depth: int = 2, base_dir: Path | None = None) -> dict:
    """Export an article and N levels of connected articles.

    Traverses outgoing wiki-links and backlinks to build a subgraph.
    """
    cfg = load_config(base_dir)
    # Resolve root slug to canonical
    from .resolve import load_aliases, resolve_link
    _cfg = load_config(base_dir)
    aliases = load_aliases(Path(_cfg["paths"]["meta"]))
    canonical_root = resolve_link(slug, aliases) or slug

    visited: dict[str, dict] = {}
    queue = [(canonical_root, 0)]

    while queue:
        current_slug, current_depth = queue.pop(0)
        if current_slug in visited or current_depth > depth:
            continue

        article = export_article(current_slug, base_dir)
        if not article:
            continue

        canonical = article["slug"]  # Use canonical slug from export_article
        if canonical in visited:
            continue

        visited[canonical] = {
            "slug": canonical,
            "title": article["title"],
            "summary": article["summary"],
            "tags": article["tags"],
            "depth": current_depth,
        }

        if current_depth < depth:
            for link in article.get("outgoing_links", []):
                if link["slug"] not in visited:
                    queue.append((link["slug"], current_depth + 1))
            for bl in article.get("backlinks", []):
                if bl["slug"] not in visited:
                    queue.append((bl["slug"], current_depth + 1))

    return {
        "root": canonical_root,
        "depth": depth,
        "nodes": list(visited.values()),
        "count": len(visited),
    }
