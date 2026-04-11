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


# ─── Customizable constants ──────────────────────────────────────
# Downstream projects can override these to customise taxonomy behaviour.
#
#     import tools.taxonomy as tax
#     tax.TAXONOMY_LABEL_KEYS = ["zh"]          # single-language labels
#     tax.TAXONOMY_GENERATOR = my_rule_fn        # skip LLM entirely
#

# Language keys used in taxonomy labels.  Override to change the set of
# languages in label dicts (e.g. ["zh"] for a monolingual KB).
TAXONOMY_LABEL_KEYS: list[str] = ["en", "zh", "ja"]

# Pluggable taxonomy generator.  When set to a callable, generate_taxonomy()
# calls it instead of the built-in LLM path.  Signature:
#   (articles: list[dict], cfg: dict) -> list[dict]   # returns category tree
# The function receives the same article dicts (slug, title, tags, summary)
# and should return a list of category nodes with id, label, children,
# article_slugs fields.  Set to None (default) to use the LLM generator.
TAXONOMY_GENERATOR = None


TAXONOMY_SYSTEM_PROMPT = """You are a knowledge base architect. Your job is to analyze a collection
of wiki articles and produce a deep, well-structured hierarchical taxonomy (like a library catalog
or an academic classification system).

Rules:
- Derive categories ENTIRELY from the actual content — do not assume any domain
- Create a DEEP tree structure — use as many levels as the content naturally supports:
  * < 20 articles: 2-3 levels deep
  * 20-100 articles: 3-4 levels deep
  * 100+ articles: 4-5 levels deep
- A category with 4+ articles should almost always have subcategories
- Leaf categories should have 1-3 articles each (fine-grained grouping)
- Every article must be assigned to exactly one leaf or category
- Category names must be trilingual: English, 中文, 日本語
- Use short, clear category names (2-4 words)
- Group by SEMANTIC similarity, not surface-level keyword matching
- Think like a librarian: broad → narrow → specific
- If an article doesn't fit any natural group, put it in an "Other" category
- Respond with ONLY valid JSON, no markdown fences, no explanation"""

TAXONOMY_PROMPT_TEMPLATE = """Analyze these {count} wiki articles and create a DEEP hierarchical taxonomy.

Articles:
{articles}

Produce a JSON array of categories. The tree can be nested multiple levels deep:
{{
  "id": "kebab-case-id",
  "label": {{"en": "English Name", "zh": "中文名", "ja": "日本語名"}},
  "children": [
    {{
      "id": "child-id",
      "label": {{"en": "...", "zh": "...", "ja": "..."}},
      "children": [
        {{
          "id": "grandchild-id",
          "label": {{"en": "...", "zh": "...", "ja": "..."}},
          "children": [],
          "article_slugs": ["slug1"]
        }}
      ],
      "article_slugs": ["slug2"]
    }}
  ],
  "article_slugs": ["slug3"]
}}

Rules:
- children can be nested to ANY depth — let the content determine the tree depth
- article_slugs at a node = articles that belong to this category but not to any child
- Every article slug must appear EXACTLY ONCE across the entire tree
- A category with 4+ articles should be split into subcategories
- Prefer deep narrow trees over flat wide ones — this creates a better browsing experience
- Output ONLY the JSON array, nothing else"""


def generate_taxonomy(base_dir: Path | None = None) -> dict:
    """Use LLM to generate taxonomy from current articles, save to cache.

    For large KBs (100+ articles), uses a two-phase approach:
    Phase 1: LLM generates top-level categories from tag summary (cheap)
    Phase 2: Assigns articles to categories by tag matching (no LLM)

    For small KBs (<100 articles), sends all articles to LLM in one shot.

    WILL NOT overwrite a locked taxonomy — returns existing instead.
    """
    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    meta_dir.mkdir(parents=True, exist_ok=True)

    # Respect locked taxonomy
    existing = load_taxonomy(base_dir)
    if existing.get("locked"):
        logger.info("[taxonomy] Taxonomy is locked, skipping generation")
        return existing

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

    try:
        # Pluggable strategy: if TAXONOMY_GENERATOR is set, use it
        if callable(TAXONOMY_GENERATOR):
            logger.info("[taxonomy] Using custom TAXONOMY_GENERATOR")
            tree = TAXONOMY_GENERATOR(articles, cfg)
        elif len(articles) <= 100:
            tree = _generate_single_pass(articles, cfg)
        else:
            tree = _generate_two_phase(articles, cfg)

        if tree:
            tree = _ensure_complete_assignment(tree, articles)
            result = {"categories": tree, "generated": True}
        else:
            logger.warning("[taxonomy] LLM returned invalid taxonomy, falling back to tag-based")
            result = {"categories": _fallback_taxonomy(articles), "generated": False}
    except Exception as e:
        logger.error(f"[taxonomy] Taxonomy generation failed: {e}, using fallback")
        result = {"categories": _fallback_taxonomy(articles), "generated": False}

    # Save cache
    path = meta_dir / "taxonomy.json"
    from .atomic import atomic_write_json
    atomic_write_json(path, result)
    logger.info(f"[taxonomy] Generated {len(result['categories'])} categories for {len(articles)} articles")

    # Sync taxonomy categories back to article tags
    _sync_taxonomy_to_tags(result.get("categories", []), concepts_dir)

    from .hooks import emit
    emit("taxonomy_generated",
         category_count=len(result["categories"]),
         article_count=len(articles),
         generated=result.get("generated", False))

    return result


def _generate_single_pass(articles: list[dict], cfg: dict) -> list[dict] | None:
    """Small KB: send all articles to LLM in one prompt."""
    article_lines = []
    for a in articles:
        tags_str = ", ".join(a["tags"][:5]) if a["tags"] else "none"
        article_lines.append(f'- {a["slug"]} | {a["title"]} | {tags_str}')
    articles_text = "\n".join(article_lines)
    prompt = TAXONOMY_PROMPT_TEMPLATE.format(count=len(articles), articles=articles_text)
    # Use 2x max_tokens for taxonomy — thinking models need extra room
    tax_tokens = min(cfg["llm"]["max_tokens"] * 2, 16384)
    response = chat(prompt, system=TAXONOMY_SYSTEM_PROMPT, max_tokens=tax_tokens)
    return _parse_taxonomy_response(response)


def _generate_two_phase(articles: list[dict], cfg: dict) -> list[dict] | None:
    """Large KB (100+ articles): two-phase taxonomy to avoid token overflow.

    Phase 1: LLM sees ALL tags (not just top 40) + article title samples
             → generates category structure with match_tags
    Phase 2: Articles assigned by title-based LLM classification in batches
             (not just tag matching, which is too coarse)
    """
    from collections import Counter

    # Phase 1: Build comprehensive tag summary
    tag_counter = Counter()
    title_samples: dict[str, list[str]] = {}
    for a in articles:
        for t in a.get("tags", []):
            t_lower = t.lower()
            if t_lower.startswith("category:"):
                continue
            tag_counter[t_lower] += 1
            title_samples.setdefault(t_lower, [])
            if len(title_samples[t_lower]) < 2:
                title_samples[t_lower].append(a["title"])

    # ALL tags with 2+ occurrences (not just top 40)
    tag_summary = []
    for tag, count in tag_counter.most_common():
        if count < 2 and len(tag_summary) >= 60:
            break  # Stop after 60 tags or when freq drops to 1
        samples = title_samples.get(tag, [])[:2]
        sample_str = "; ".join(samples)
        tag_summary.append(f"- {tag} ({count}): {sample_str}")
    tag_text = "\n".join(tag_summary)

    # Also show a sample of article titles to give LLM better domain understanding
    import random
    sample_titles = random.sample(
        [a["title"] for a in articles],
        min(30, len(articles))
    )
    titles_text = "\n".join(f"  {t}" for t in sample_titles)

    phase1_prompt = f"""This knowledge base has {len(articles)} articles. Here are the tags and sample titles:

Tags (with article counts):
{tag_text}

Sample article titles:
{titles_text}

Create a DEEP hierarchical taxonomy. CRITICAL RULES:
- Each DISTINCT tradition, school, discipline, or topic MUST be its own top-level category
- Do NOT lump different traditions or subjects together — keep them SEPARATE
- Create subcategories within each top-level category
- Include a catch-all category for articles that don't fit elsewhere

Produce a JSON array:
{{
  "id": "kebab-case-id",
  "label": {{"en": "English Name", "zh": "中文名", "ja": "日本語名"}},
  "match_tags": ["tag1", "tag2"],
  "match_title_keywords": ["keyword1", "keyword2"],
  "children": [...]
}}

match_tags = tags that should map to this category.
match_title_keywords = keywords in article TITLES that indicate this category.
Output ONLY the JSON array."""

    logger.info(f"[taxonomy] Phase 1: generating category structure from {len(tag_counter)} tags...")
    tax_tokens = min(cfg["llm"]["max_tokens"] * 2, 16384)
    response = chat(phase1_prompt, system=TAXONOMY_SYSTEM_PROMPT, max_tokens=tax_tokens)
    category_tree = _parse_taxonomy_response(response)

    if not category_tree:
        return None

    # Phase 2: Assign articles to categories by matching tags
    logger.info(f"[taxonomy] Phase 2: assigning {len(articles)} articles to categories...")
    _assign_articles_to_tree(category_tree, articles)

    return category_tree


def _assign_articles_to_tree(tree: list[dict], articles: list[dict]):
    """Assign articles to categories using tags + title keywords.

    Matching priority: title keyword (strongest) > specific tag > generic tag.
    """
    import re

    # Build flat mappings
    tag_to_node: dict[str, tuple[dict, int]] = {}
    keyword_to_node: dict[str, tuple[dict, int]] = {}

    def _index(nodes, depth=0):
        for node in nodes:
            for tag in node.get("match_tags", []):
                t = tag.lower()
                if t not in tag_to_node or depth > tag_to_node[t][1]:
                    tag_to_node[t] = (node, depth)
            for kw in node.get("match_title_keywords", []):
                k = kw.lower()
                if k not in keyword_to_node or depth > keyword_to_node[k][1]:
                    keyword_to_node[k] = (node, depth)
            _index(node.get("children", []), depth + 1)

    _index(tree)

    assigned = set()
    for a in articles:
        best_node = None
        best_depth = -1
        best_score = 0  # keyword match scores higher than tag match

        title_lower = a.get("title", "").lower()

        # Title keyword matching (strongest signal)
        for kw, (node, depth) in keyword_to_node.items():
            if kw in title_lower:
                score = 100 + depth  # Keyword match always wins over tag
                if score > best_score:
                    best_node = node
                    best_depth = depth
                    best_score = score

        # Tag matching (fallback)
        if best_score < 100:
            for tag in a.get("tags", []):
                t = tag.lower()
                if t.startswith("category:"):
                    continue
                if t in tag_to_node:
                    node, depth = tag_to_node[t]
                    score = depth
                    if score > best_score:
                        best_node = node
                        best_depth = depth
                        best_score = score

        if best_node is not None:
            best_node.setdefault("article_slugs", []).append(a["slug"])
            assigned.add(a["slug"])

    # Unassigned → "other"
    unassigned = [a["slug"] for a in articles if a["slug"] not in assigned]
    if unassigned:
        tree.append({
            "id": "other",
            "label": {"en": "Other", "zh": "其他", "ja": "その他"},
            "children": [],
            "article_slugs": unassigned,
        })

    # Clean up match_tags from output (not needed in cache)
    def _clean(nodes):
        for n in nodes:
            n.pop("match_tags", None)
            n.pop("match_title_keywords", None)
            n.setdefault("article_slugs", [])
            _clean(n.get("children", []))

    _clean(tree)


def _sync_taxonomy_to_tags(tree: list[dict], concepts_dir: Path, path: list[str] | None = None):
    """Write taxonomy category path back to article tags.

    For each article assigned in the taxonomy tree, adds a `category:xxx`
    tag reflecting its position. This unifies taxonomy and wiki tags.

    Example: an article under "Science > Physics" gets:
      tags: [...existing..., "category:buddhism", "category:buddhism/practice"]
    """
    if path is None:
        path = []

    for node in tree:
        node_id = node.get("id", "")
        current_path = path + [node_id] if node_id else path

        # Tag articles at this node
        for slug in node.get("article_slugs", []):
            _apply_category_tags(concepts_dir, slug, current_path)

        # Recurse into children
        _sync_taxonomy_to_tags(node.get("children", []), concepts_dir, current_path)


def _apply_category_tags(concepts_dir: Path, slug: str, category_path: list[str]):
    """Add category:xxx tags to an article, removing old category tags."""
    article_path = concepts_dir / f"{slug}.md"
    if not article_path.exists():
        return

    post = frontmatter.load(str(article_path))
    tags = post.metadata.get("tags", [])

    # Remove old category tags
    tags = [t for t in tags if not t.startswith("category:")]

    # Add new category tags (each level of the path)
    for i in range(len(category_path)):
        cat_tag = "category:" + "/".join(category_path[:i + 1])
        if cat_tag not in tags:
            tags.append(cat_tag)

    post.metadata["tags"] = tags
    article_path.write_text(frontmatter.dumps(post), encoding="utf-8")


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

    # Deduplicate: each slug appears only once (first occurrence wins)
    _dedup_tree(raw_tree)

    # Localize labels and resolve article_slugs → {slug, title}
    title_map = _build_title_map(concepts_dir)
    return _localize_tree(raw_tree, lang, title_map)


def _dedup_tree(nodes: list[dict], seen: set | None = None):
    """Remove duplicate article_slugs across the tree. First occurrence wins."""
    if seen is None:
        seen = set()
    for node in nodes:
        # Dedup children first (deeper = more specific, keep those)
        _dedup_tree(node.get("children", []), seen)
        # Then dedup this node's slugs
        unique = []
        for s in node.get("article_slugs", []):
            if s not in seen:
                seen.add(s)
                unique.append(s)
        node["article_slugs"] = unique


def assign_new_articles(base_dir: Path | None = None):
    """Assign newly compiled articles to existing taxonomy categories.

    Runs after compile — no LLM needed, uses tag matching.
    New articles get added to the category whose existing articles
    share the most tags. Respects locked taxonomy (only adds, never restructures).
    """
    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    concepts_dir = Path(cfg["paths"]["concepts"])
    tax_path = meta_dir / "taxonomy.json"

    if not tax_path.exists():
        return

    taxonomy = json.loads(tax_path.read_text())
    categories = taxonomy.get("categories", [])
    if not categories:
        return

    # Collect all slugs already in taxonomy
    assigned = set()
    def _collect(nodes):
        for n in nodes:
            assigned.update(n.get("article_slugs", []))
            _collect(n.get("children", []))
    _collect(categories)

    # Find unassigned articles
    all_slugs = {f.stem for f in concepts_dir.glob("*.md")}
    unassigned = all_slugs - assigned
    if not unassigned:
        return

    # Build tag profile for each category (from its existing articles)
    cat_profiles = {}  # category_id → set of tags
    def _build_profiles(nodes):
        for n in nodes:
            cat_id = n.get("id", "")
            tags = set()
            for slug in n.get("article_slugs", []):
                article_path = concepts_dir / f"{slug}.md"
                if article_path.exists():
                    post = frontmatter.load(str(article_path))
                    tags.update(t.lower() for t in post.metadata.get("tags", []))
            cat_profiles[cat_id] = (tags, n)
            _build_profiles(n.get("children", []))
    _build_profiles(categories)

    # Assign each new article to best-matching category
    for slug in unassigned:
        article_path = concepts_dir / f"{slug}.md"
        if not article_path.exists():
            continue
        post = frontmatter.load(str(article_path))
        article_tags = set(t.lower() for t in post.metadata.get("tags", []))

        if not article_tags:
            # No tags — put in "Other"
            _add_to_other(categories, slug)
            continue

        # Score each category by tag overlap
        best_cat = None
        best_score = 0
        for cat_id, (cat_tags, node) in cat_profiles.items():
            if not cat_tags:
                continue
            overlap = len(article_tags & cat_tags)
            if overlap > best_score:
                best_score = overlap
                best_cat = node

        if best_cat and best_score > 0:
            best_cat.setdefault("article_slugs", []).append(slug)
        else:
            _add_to_other(categories, slug)

    # Save updated taxonomy (preserve locked flag)
    taxonomy["categories"] = categories
    tax_path.write_text(json.dumps(taxonomy, indent=2, ensure_ascii=False), encoding="utf-8")
    logger.info(f"[taxonomy] Assigned {len(unassigned)} new articles to categories")


def _add_to_other(categories: list, slug: str):
    """Add slug to the 'other' category, creating it if needed."""
    for c in categories:
        if c.get("id") in ("other", "其他"):
            c.setdefault("article_slugs", []).append(slug)
            return
    categories.append({
        "id": "other",
        "label": {"en": "Other", "zh": "其他", "ja": "その他"},
        "article_slugs": [slug],
        "children": [],
    })


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
    from .llm import extract_json
    text = extract_json(response)  # Handle thinking mode output
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
        # Fix label format: ensure all labels are trilingual dicts
        _fix_labels(tree)
        return tree
    except (json.JSONDecodeError, KeyError):
        return None


def _fix_labels(tree: list[dict]):
    """Ensure all category labels are dicts with every TAXONOMY_LABEL_KEYS entry.

    LLMs sometimes return label as a string instead of a dict.
    This normalizes all labels in the tree recursively.
    """
    keys = TAXONOMY_LABEL_KEYS
    for node in tree:
        label = node.get("label", "")
        if isinstance(label, dict):
            # Find a usable fallback from any existing key
            fallback_val = None
            for k in keys:
                if isinstance(label.get(k), str) and label[k]:
                    fallback_val = label[k]
                    break
            if fallback_val is None:
                fallback_val = node.get("id", "")
            for k in keys:
                label.setdefault(k, fallback_val)
        else:
            # String, number, list, null → normalize to dict
            text = str(label) if label else node.get("id", "unknown")
            node["label"] = {k: text for k in keys}
        # Coerce dict label values to clean strings
        if isinstance(node["label"], dict):
            fallback = node.get("id", "")
            for k in keys:
                v = node["label"].get(k)
                node["label"][k] = v if isinstance(v, str) and v else fallback
        # Recurse into children (guard against null/non-list/non-dict items)
        children = node.get("children")
        if isinstance(children, dict):
            children = [children]  # Single child object → wrap in list
        if isinstance(children, list):
            node["children"] = [c for c in children if isinstance(c, dict)]
            _fix_labels(node["children"])
        else:
            node["children"] = []


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


def _localize_title(title: str, lang: str) -> str:
    """Extract the language-specific part of a bilingual title.

    "Mencius / 孟子" + lang=zh → "孟子"
    "Mencius / 孟子" + lang=en → "Mencius"
    "Mencius / 孟子" + lang=zh-en → "Mencius / 孟子"
    "some-slug-only" → "some-slug-only" (no change)
    """
    import re
    if not title or "/" not in title:
        return title
    if lang == "zh-en":
        return title

    parts = [p.strip() for p in title.split("/") if p.strip()]
    if len(parts) < 2:
        return title

    has_cjk = lambda s: bool(re.search(r'[\u4e00-\u9fff\u3400-\u4dbf\u3040-\u309f\u30a0-\u30ff]', s))

    if lang in ("zh", "ja"):
        cjk = next((p for p in parts if has_cjk(p)), None)
        return cjk or parts[-1]
    else:
        en = next((p for p in parts if not has_cjk(p)), None)
        return en or parts[0]


def _localize_tree(tree: list[dict], lang: str, title_map: dict[str, str]) -> list[dict]:
    """Convert raw taxonomy tree to the format the web API expects.

    Resolves article_slugs to {slug, title} objects. Both category labels
    AND article titles are localized to the requested language.
    """
    result = []
    for node in tree:
        slugs = node.get("article_slugs", [])
        articles = [{"slug": s, "title": _localize_title(title_map.get(s, s), lang)} for s in slugs]

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
