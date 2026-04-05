"""Taxonomy — LLM-generated hierarchical categories from wiki articles.

The taxonomy is NOT hardcoded. Instead, the LLM reads all article
titles, tags, and summaries, then produces a domain-appropriate
hierarchical classification. This means llmbase works for any domain:
Buddhist studies, software engineering, cooking, history, etc.

Flow:
  generate_taxonomy()  →  LLM generates tree + assigns articles  →  taxonomy.json
  build_taxonomy(lang) →  reads cached taxonomy.json  →  returns localized tree

The worker calls generate_taxonomy() periodically (default every 12h).
The web API calls build_taxonomy(lang) on each request (fast, reads cache).
"""

import json
import logging
from pathlib import Path

import frontmatter

from .config import load_config
from .llm import chat

logger = logging.getLogger("llmbase.taxonomy")


TAXONOMY_SYSTEM_PROMPT = """You are a knowledge base architect. Your job is to analyze a collection
of wiki articles and produce a hierarchical classification system (taxonomy).

Rules:
- Derive categories ENTIRELY from the actual content — do not assume any domain
- Create 3-7 top-level categories (more if the content is very diverse)
- Each top-level category can have 0-5 subcategories
- Every article must be assigned to exactly one category (the most specific one that fits)
- Category names must be trilingual: English, 中文, 日本語
- Use short, clear category names (2-4 words)
- Group by SEMANTIC similarity, not surface-level keyword matching
- If an article doesn't fit any natural group, put it in an "Other" category
- Respond with ONLY valid JSON, no markdown fences, no explanation"""

TAXONOMY_PROMPT_TEMPLATE = """Analyze these {count} wiki articles and create a hierarchical taxonomy.

Articles:
{articles}

Produce a JSON array of categories. Each category has this structure:
{{
  "id": "kebab-case-id",
  "label": {{"en": "English Name", "zh": "中文名", "ja": "日本語名"}},
  "children": [
    {{
      "id": "child-id",
      "label": {{"en": "...", "zh": "...", "ja": "..."}},
      "children": [],
      "article_slugs": ["slug1", "slug2"]
    }}
  ],
  "article_slugs": ["slug3"]
}}

Rules:
- article_slugs at the parent level = articles that belong to this category but not to any child
- Every article slug must appear exactly once across the entire tree
- children can be empty [] if no subcategories are needed
- Output ONLY the JSON array, nothing else"""


def generate_taxonomy(base_dir: Path | None = None) -> dict:
    """Use LLM to generate taxonomy from current articles, save to cache.

    Called by the worker periodically. This is the expensive operation
    that invokes the LLM. Results are cached to taxonomy.json.
    """
    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    meta_dir.mkdir(parents=True, exist_ok=True)

    if not concepts_dir.exists():
        return {"categories": []}

    # Collect all article metadata
    articles = []
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        articles.append({
            "slug": md_file.stem,
            "title": post.metadata.get("title", md_file.stem),
            "tags": post.metadata.get("tags", []),
            "summary": post.metadata.get("summary", ""),
        })

    if not articles:
        return {"categories": []}

    # Format articles for the prompt
    article_lines = []
    for a in articles:
        tags_str = ", ".join(a["tags"]) if a["tags"] else "none"
        article_lines.append(f'- slug: {a["slug"]} | title: {a["title"]} | tags: {tags_str} | summary: {a["summary"]}')
    articles_text = "\n".join(article_lines)

    prompt = TAXONOMY_PROMPT_TEMPLATE.format(count=len(articles), articles=articles_text)

    try:
        response = chat(prompt, system=TAXONOMY_SYSTEM_PROMPT, max_tokens=cfg["llm"]["max_tokens"])
        tree = _parse_taxonomy_response(response)

        if tree:
            # Validate: ensure all articles are assigned, fix if not
            tree = _ensure_complete_assignment(tree, articles)
            result = {"categories": tree, "generated": True}
        else:
            logger.warning("[taxonomy] LLM returned invalid taxonomy, falling back to tag-based")
            result = {"categories": _fallback_taxonomy(articles), "generated": False}

    except Exception as e:
        logger.error(f"[taxonomy] LLM taxonomy generation failed: {e}, using fallback")
        result = {"categories": _fallback_taxonomy(articles), "generated": False}

    # Save cache
    path = meta_dir / "taxonomy.json"
    path.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
    logger.info(f"[taxonomy] Generated {len(result['categories'])} categories for {len(articles)} articles")

    return result


def build_taxonomy(base_dir: Path | None = None, lang: str = "zh") -> list[dict]:
    """Read cached taxonomy and return localized tree for the web API.

    This is fast (no LLM call). If no cache exists, generates a simple
    tag-based fallback synchronously.
    """
    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    concepts_dir = Path(cfg["paths"]["concepts"])
    cache_path = meta_dir / "taxonomy.json"

    if cache_path.exists():
        cached = json.loads(cache_path.read_text())
        raw_tree = cached.get("categories", [])
    else:
        # No cache yet — use fast fallback (no LLM)
        articles = _load_articles(concepts_dir)
        raw_tree = _fallback_taxonomy(articles)

    # Localize labels and resolve article_slugs → {slug, title}
    title_map = _build_title_map(concepts_dir)
    return _localize_tree(raw_tree, lang, title_map)


def load_taxonomy(base_dir: Path | None = None) -> dict:
    """Load cached taxonomy (raw, not localized)."""
    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    path = meta_dir / "taxonomy.json"
    if path.exists():
        return json.loads(path.read_text())
    return {"categories": []}


# ─── Internal helpers ─────────────────────────────────────────────


def _parse_taxonomy_response(response: str) -> list[dict] | None:
    """Parse LLM JSON response into taxonomy tree."""
    text = response.strip()
    # Strip markdown fences if present
    if text.startswith("```"):
        text = text.split("\n", 1)[1] if "\n" in text else text[3:]
    if text.endswith("```"):
        text = text.rsplit("```", 1)[0]
    text = text.strip()

    # Try to find the JSON array
    start = text.find("[")
    end = text.rfind("]")
    if start == -1 or end == -1:
        return None

    try:
        tree = json.loads(text[start:end + 1])
        if not isinstance(tree, list):
            return None
        # Basic validation: each node needs id and label
        for node in tree:
            if not isinstance(node, dict) or "id" not in node or "label" not in node:
                return None
        return tree
    except (json.JSONDecodeError, KeyError):
        return None


def _ensure_complete_assignment(tree: list[dict], articles: list[dict]) -> list[dict]:
    """Make sure every article appears exactly once in the tree."""
    all_slugs = {a["slug"] for a in articles}
    assigned = set()

    def _collect_assigned(nodes):
        for node in nodes:
            for slug in node.get("article_slugs", []):
                assigned.add(slug)
            _collect_assigned(node.get("children", []))

    _collect_assigned(tree)

    # Find unassigned articles
    missing = all_slugs - assigned
    if missing:
        # Add an "Other" category for unassigned
        other_node = None
        for node in tree:
            if node["id"] == "other":
                other_node = node
                break
        if other_node is None:
            other_node = {
                "id": "other",
                "label": {"en": "Other", "zh": "其他", "ja": "その他"},
                "children": [],
                "article_slugs": [],
            }
            tree.append(other_node)
        other_node["article_slugs"].extend(sorted(missing))

    # Remove duplicates (keep first occurrence)
    seen = set()

    def _dedup(nodes):
        for node in nodes:
            slugs = node.get("article_slugs", [])
            node["article_slugs"] = [s for s in slugs if s not in seen and not seen.add(s)]
            _dedup(node.get("children", []))

    _dedup(tree)

    return tree


def _fallback_taxonomy(articles: list[dict]) -> list[dict]:
    """Tag-frequency-based taxonomy when LLM is unavailable.

    Groups articles by their most common tags. No hardcoded domains.
    """
    from collections import Counter

    if not articles:
        return []

    # Count tag frequency
    tag_counter = Counter()
    article_tags = {}
    for a in articles:
        tags = [t.lower().replace(" ", "-") for t in a.get("tags", [])]
        article_tags[a["slug"]] = tags
        for t in tags:
            tag_counter[t] += 1

    # Use top tags as categories (tags appearing in 2+ articles, up to 10)
    top_tags = [tag for tag, count in tag_counter.most_common(10) if count >= 2]

    if not top_tags:
        # Everything in one "All" category
        return [{
            "id": "all",
            "label": {"en": "All Articles", "zh": "全部文章", "ja": "全記事"},
            "children": [],
            "article_slugs": [a["slug"] for a in articles],
        }]

    assigned = set()
    categories = []

    for tag in top_tags:
        slugs = []
        for a in articles:
            if a["slug"] in assigned:
                continue
            if tag in article_tags.get(a["slug"], []):
                slugs.append(a["slug"])
                assigned.add(a["slug"])
        if slugs:
            # Use the tag itself as the label (best effort, no hardcoded mapping)
            categories.append({
                "id": tag,
                "label": {"en": tag.replace("-", " ").title(), "zh": tag, "ja": tag},
                "children": [],
                "article_slugs": slugs,
            })

    # Unassigned → Other
    unassigned = [a["slug"] for a in articles if a["slug"] not in assigned]
    if unassigned:
        categories.append({
            "id": "other",
            "label": {"en": "Other", "zh": "其他", "ja": "その他"},
            "children": [],
            "article_slugs": unassigned,
        })

    return categories


def _localize_tree(tree: list[dict], lang: str, title_map: dict[str, str]) -> list[dict]:
    """Convert raw taxonomy tree to the format the web API expects.

    Resolves article_slugs to {slug, title} objects and picks
    the label for the requested language.
    """
    result = []
    for node in tree:
        slugs = node.get("article_slugs", [])
        articles = [{"slug": s, "title": title_map.get(s, s)} for s in slugs]

        children = _localize_tree(node.get("children", []), lang, title_map)
        child_count = sum(c["total"] for c in children)

        label = node.get("label", {})
        if isinstance(label, str):
            display_label = label
        else:
            display_label = label.get(lang, label.get("en", label.get("zh", node["id"])))

        result.append({
            "id": node["id"],
            "label": display_label,
            "count": len(articles),
            "total": len(articles) + child_count,
            "articles": articles,
            "children": children,
        })

    return result


def _load_articles(concepts_dir: Path) -> list[dict]:
    """Load article metadata from disk."""
    articles = []
    if not concepts_dir.exists():
        return articles
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        articles.append({
            "slug": md_file.stem,
            "title": post.metadata.get("title", md_file.stem),
            "tags": post.metadata.get("tags", []),
            "summary": post.metadata.get("summary", ""),
        })
    return articles


def _build_title_map(concepts_dir: Path) -> dict[str, str]:
    """Build slug → title mapping."""
    title_map = {}
    if not concepts_dir.exists():
        return title_map
    for md_file in concepts_dir.glob("*.md"):
        post = frontmatter.load(str(md_file))
        title_map[md_file.stem] = post.metadata.get("title", md_file.stem)
    return title_map
