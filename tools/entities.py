"""Entity extraction — people, events, places from wiki articles.

Opt-in feature (config: entities.enabled). LLM reads article metadata
and extracts structured entities for timeline, map, and people views.

Uses two-phase extraction for large KBs to avoid token overflow.
"""

import json
import logging
from pathlib import Path

import frontmatter

from .config import load_config, ensure_dirs
from .llm import chat

logger = logging.getLogger("llmbase.entities")

# ─── Customizable constants ──────────────────────────────────────
# Override to change entity extraction behavior for different domains.
#
#     import tools.entities as ent
#     ent.ENTITY_SYSTEM_PROMPT = "Extract scientific entities..."
#     ent.ENTITY_PROMPT = "... custom format ..."
#

# Callable that formats article dicts into text lines for the LLM prompt.
# Signature: (articles: list[dict]) -> list[str]
# Default: sends summary for small KBs, tags-only for large ones.
# Override for custom formatting (e.g. always include summary, add custom fields).
ENTITY_ARTICLE_FORMATTER = None

ENTITY_SYSTEM_PROMPT = """You are a knowledge base analyst. Extract structured entities
(people, events, places) from wiki article metadata.

Rules:
- Only extract entities clearly supported by the article data
- Dates should be in a consistent format: "YYYY", "c.YYYY", "YYYY-YYYY", "YYYY BCE"
- Names must be bilingual when possible: name (English) + name_local (original script)
- coords for places: [latitude, longitude] if determinable, null otherwise
- Each entity references the article slugs it appears in
- Do NOT invent entities not supported by the data
- Respond with ONLY valid JSON, no markdown fences"""

ENTITY_PROMPT = """Extract people, events, and places from these {count} wiki articles:

{articles}

Output a JSON object:
{{
  "people": [
    {{"name": "English Name", "name_local": "本地名", "dates": "c.372-289 BCE",
      "role": "Philosopher", "articles": ["slug1", "slug2"]}}
  ],
  "events": [
    {{"name": "Event Name", "name_local": "事件名", "date": "1190 CE",
      "description": "One-line description", "articles": ["slug1"]}}
  ],
  "places": [
    {{"name": "Place Name", "name_local": "地名", "coords": [35.6, 116.99],
      "articles": ["slug1"]}}
  ]
}}

Rules:
- Include ALL identifiable people, events, and places
- coords can be null if unknown
- articles = list of article slugs where this entity is mentioned
- Output ONLY the JSON object"""


def extract_entities(base_dir: Path | None = None) -> dict:
    """Extract entities from wiki articles using LLM. Cache to entities.json."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    if not cfg.get("entities", {}).get("enabled", False):
        logger.info("[entities] Entity extraction disabled in config")
        return get_entities(base_dir)

    empty = {"people": [], "events": [], "places": []}
    if not concepts_dir.exists():
        _save_entities(meta_dir, empty)
        return empty

    # Collect article metadata
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
        _save_entities(meta_dir, empty)
        return empty

    # Build compact article list (avoid token overflow)
    # Use custom formatter if provided, otherwise default logic
    if callable(ENTITY_ARTICLE_FORMATTER):
        try:
            article_lines = list(ENTITY_ARTICLE_FORMATTER(articles))
        except Exception as e:
            logger.warning(f"[entities] Custom formatter failed: {e}, using default")
            article_lines = None
    else:
        article_lines = None

    if article_lines is None:
        if len(articles) <= 80:
            article_lines = [
                f'- {a["slug"]}: {a["title"]} | {a["summary"]}'
                for a in articles
            ]
        else:
            # Large KB: only send titles + tags (no summaries)
            article_lines = [
                f'- {a["slug"]}: {a["title"]} | tags: {", ".join(a["tags"][:3])}'
                for a in articles
            ]

    articles_text = "\n".join(article_lines)
    prompt = ENTITY_PROMPT.format(count=len(articles), articles=articles_text)

    try:
        response = chat(prompt, system=ENTITY_SYSTEM_PROMPT, max_tokens=cfg["llm"]["max_tokens"])
        result = _parse_entity_response(response)
    except Exception as e:
        logger.error(f"[entities] Extraction failed: {e}")
        result = {"people": [], "events": [], "places": []}

    # Deduplicate entities (LLM often generates duplicates for large KBs)
    result["people"] = _dedup_entities(result.get("people", []))
    result["events"] = _dedup_entities(result.get("events", []))
    result["places"] = _dedup_entities(result.get("places", []))

    # Add metadata
    result["article_count"] = len(articles)
    result["extracted_at"] = __import__("datetime").datetime.now(
        __import__("datetime").timezone.utc
    ).isoformat()

    # Cache
    _save_entities(meta_dir, result)
    people_count = len(result.get('people', []))
    events_count = len(result.get('events', []))
    places_count = len(result.get('places', []))
    logger.info(
        f"[entities] Extracted {people_count} people, "
        f"{events_count} events, {places_count} places"
    )

    from .hooks import emit
    emit("entity_extracted",
         people_count=people_count, events_count=events_count,
         places_count=places_count, article_count=len(articles))

    return result


def _dedup_entities(entities: list[dict]) -> list[dict]:
    """Merge duplicate entities by name/name_local.

    Entities with the same name (case-insensitive) or same name_local
    are merged: articles lists combined, longest dates/role kept.
    """
    if not entities:
        return []

    merged: dict[str, dict] = {}

    # Build lookup indices for transitive matching
    name_index: dict[str, str] = {}    # lowercase name → merge key
    local_index: dict[str, str] = {}   # name_local → merge key

    for entity in entities:
        if not isinstance(entity, dict):
            continue
        name = (entity.get("name") or "").strip().lower()
        name_local = (entity.get("name_local") or "").strip()

        # Find existing match by name OR name_local (transitive via indices)
        key_by_name = name_index.get(name) if name else None
        key_by_local = local_index.get(name_local) if name_local else None

        # If both indices point to different entities, merge them first
        if (key_by_name and key_by_local
                and key_by_name != key_by_local
                and key_by_name in merged and key_by_local in merged):
            # Union: merge key_by_local into key_by_name
            target = merged[key_by_name]
            source = merged.pop(key_by_local)
            target_articles = set(target.get("articles") or [])
            target_articles.update(source.get("articles") or [])
            target["articles"] = sorted(target_articles)
            for field in ("dates", "date", "role", "description"):
                if len(source.get(field) or "") > len(target.get(field) or ""):
                    target[field] = source[field]
            # Repoint local_index entries
            for k, v in list(local_index.items()):
                if v == key_by_local:
                    local_index[k] = key_by_name
            for k, v in list(name_index.items()):
                if v == key_by_local:
                    name_index[k] = key_by_name

        match_key = key_by_name or key_by_local

        if match_key and match_key in merged:
            # Merge into existing
            existing = merged[match_key]
            # Combine article lists (guard against null)
            existing_articles = set(existing.get("articles") or [])
            existing_articles.update(entity.get("articles") or [])
            existing["articles"] = sorted(existing_articles)
            # Keep longer dates/date string
            for field in ("dates", "date"):
                if len(entity.get(field) or "") > len(existing.get(field) or ""):
                    existing[field] = entity[field]
            # Keep longer role/description
            for field in ("role", "description"):
                if len(entity.get(field) or "") > len(existing.get(field) or ""):
                    existing[field] = entity[field]
            # Update indices for transitive closure
            if name:
                name_index[name] = match_key
            if name_local:
                local_index[name_local] = match_key
        else:
            # New entity — ensure articles is always a list
            key = name or name_local or str(len(merged))
            entry = dict(entity)
            if not isinstance(entry.get("articles"), list):
                entry["articles"] = []
            merged[key] = entry
            if name:
                name_index[name] = key
            if name_local:
                local_index[name_local] = key

    return list(merged.values())


def _save_entities(meta_dir: Path, result: dict):
    """Write entities to cache file."""
    meta_dir.mkdir(parents=True, exist_ok=True)
    path = meta_dir / "entities.json"
    path.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")


def get_entities(base_dir: Path | None = None) -> dict:
    """Read cached entities."""
    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    path = meta_dir / "entities.json"
    if path.exists():
        try:
            return json.loads(path.read_text())
        except (json.JSONDecodeError, OSError):
            logger.warning("[entities] Corrupted entities.json, returning empty")
    return {"people": [], "events": [], "places": []}


def _parse_entity_response(response: str) -> dict:
    """Parse LLM JSON response into entity dict."""
    from .llm import extract_json
    text = extract_json(response)  # Handle thinking mode output
    if text.startswith("```"):
        text = text.split("\n", 1)[1] if "\n" in text else text[3:]
    if text.endswith("```"):
        text = text.rsplit("```", 1)[0]
    text = text.strip()

    start = text.find("{")
    end = text.rfind("}")
    if start == -1 or end == -1:
        return {"people": [], "events": [], "places": []}

    try:
        data = json.loads(text[start:end + 1])
        # Validate structure
        for key in ("people", "events", "places"):
            if key not in data or not isinstance(data[key], list):
                data[key] = []
        return data
    except (json.JSONDecodeError, KeyError):
        return {"people": [], "events": [], "places": []}
