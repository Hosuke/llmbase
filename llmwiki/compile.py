"""Compile module: LLM reads raw/ and builds a structured wiki.

Customization contract
======================
Downstream projects may override these module-level constants at import
time to change compile behavior **without forking any function**:

  SYSTEM_PROMPT           – system message sent to the LLM
  COMPILE_USER_PROMPT     – user prompt template (placeholders: {title},
                            {content}, {existing}, {article_format})
  COMPILE_ARTICLE_FORMAT  – the example article format embedded in the
                            user prompt (the most common override point)
  SECTION_HEADERS         – list of (key, markdown_header) tuples that
                            control how _split_sections / _assemble_sections /
                            _merge_into handle multi-language content

Example (single-language classical-Chinese KB)::

    import llmwiki.compile as c
    c.SECTION_HEADERS = [("文言", "## 文言")]
    c.COMPILE_ARTICLE_FORMAT = "## 文言\\n\\n以文言撰寫完整內容。"
    c.SYSTEM_PROMPT = "You are a classical-Chinese knowledge compiler..."
"""

import json
import re as _slug_re
from datetime import datetime, timezone
from pathlib import Path
from contextvars import ContextVar

import frontmatter

from .config import load_config, ensure_dirs, get_section_headers, _read_config
from .llm import chat
from . import _native


def _legacy_sanitize_slug(slug: str) -> str:
    """Normalize a concept slug to a safe filename stem.

    LLMs occasionally emit URLs verbatim as slugs (issue #5), and the
    broken-link stub fixer can receive such strings as wikilink targets.
    Left unsanitized they produce paths like
    ``concepts/foo/?ref=bar.md`` — legal on POSIX, but corrupt the wiki
    namespace (wikilink parsers, backlinks.json, and lint heal all assume
    plain-stem slugs). Collapse URL punctuation and whitespace to ``-``.
    """
    if not slug:
        return ""
    s = _slug_re.sub(r"[\s/\\?#&:]+", "-", slug)
    s = s.replace("..", "")
    s = _slug_re.sub(r"-+", "-", s).strip(".-_ ")
    return s


# Maps a raw doc's `type` field (set by ingest plugins) to the canonical
# plugin id used in source refs and remote sync rows. Plugins not listed
# here pass through unchanged (so the registry stays open and llmbase
# never needs to know about every domain a downstream may add).
_RAW_TYPE_TO_PLUGIN = {
    "buddhist_sutra": "cbeta",
    "wikisource": "wikisource",
    "classical_text": "ctext",
    "ctext": "ctext",
}


# ─── Customizable constants ──────────────────────────────────────
# Downstream projects can override these module-level constants to
# customize compile behavior without forking.  Example:
#
#     import llmwiki.compile as compile_mod
#     compile_mod.SECTION_HEADERS = [("文言", "## 文言")]
#     compile_mod.COMPILE_ARTICLE_FORMAT = "## 文言\n\n以文言撰寫完整內容。"
#     compile_mod.SYSTEM_PROMPT = "You are a classical-Chinese compiler..."
#

# Language sections used by _split_sections / _assemble_sections / _merge_into.
# Each entry: (section_key, markdown_header_line).
# DEFAULT_SECTION_HEADERS is kept as a DISTINCT object so that
# llmwiki.config.get_language_profile() can detect import-time downstream
# overrides by identity (`SECTION_HEADERS is not DEFAULT_SECTION_HEADERS`).
# The customization contract is ASSIGNMENT (`compile.SECTION_HEADERS = [...]`),
# not in-place mutation.
DEFAULT_SECTION_HEADERS: list[tuple[str, str]] = [
    ("english", "## English"),
    ("中文", "## 中文"),
    ("日本語", "## 日本語"),
]
SECTION_HEADERS: list[tuple[str, str]] = DEFAULT_SECTION_HEADERS


SYSTEM_PROMPT = """You are a knowledge base compiler. Your job is to read raw source documents
and produce structured wiki articles in markdown format.

Rules:
- Write clear, well-organized articles with proper headings
- Use [[wiki-link]] syntax for cross-references to other concepts
- Include a brief summary at the top of each article
- Categorize content into clear concepts
- Maintain factual accuracy - do not invent information not in the source
- Use backlinks to connect related concepts
- Output valid markdown with YAML frontmatter

IMPORTANT — Trilingual output:
- Write each article in THREE languages: English, 中文, 日本語
- Structure each article with three sections using h2 headers:
  ## English
  (full article content in English)
  ## 中文
  (完整中文内容，不是翻译，而是用中文学术风格重新撰写)
  ## 日本語
  (日本語による完全な記事内容)
- The summary field in frontmatter should be in English
- The title field should include both: "English Title / 中文标题"
- Keep [[wiki-links]] consistent across all three languages (use the same slug)"""


# Example article format shown inside the user prompt.
# Override this to change the per-section instructions the LLM sees.
COMPILE_ARTICLE_FORMAT = """## English

Full article content in English. Use [[Other Concept]] for cross-references.

## 中文

完整的中文文章内容。使用中文学术风格撰写，不是简单翻译。使用 [[Other Concept]] 进行交叉引用。

## 日本語

完全な日本語の記事内容。学術的な日本語で記述する。[[Other Concept]] でクロスリファレンスを使用する。"""


# Full user prompt template.  Placeholders: {title}, {content}, {existing}, {article_format}.
# Override the entire string, or just override COMPILE_ARTICLE_FORMAT for the common case.
COMPILE_USER_PROMPT = """I have a raw document titled "{title}" that needs to be compiled into wiki articles.

Source document:
---
{content}
---

EXISTING ARTICLES (you MUST reuse these — DO NOT create new articles for concepts that already exist):
{existing}

CRITICAL DEDUPLICATION RULES:
- If a concept ALREADY EXISTS above (even under a different name, translation, or variant), you MUST use ===UPDATE=== with the EXISTING slug
- A concept with a suffix (e.g., "X说", "X论", "X位") is usually the SAME as the base concept "X" — use UPDATE
- A concept in one language (e.g., Chinese title) that matches an existing concept in another language (e.g., English slug) is the SAME — use UPDATE
- When in doubt, UPDATE an existing article rather than creating a new one
- New articles should only be created for genuinely NEW concepts not covered above

Please:
1. Identify the key concepts from this document (1-5 concepts)
2. For each concept, produce a wiki article in this exact format:

===ARTICLE===
slug: concept-name-here
title: English Title / 中文标题
summary: One-line summary in English
tags: tag1, tag2, tag3
---
{article_format}
===END===

If a concept already exists in the wiki, instead output:
===UPDATE===
slug: existing-concept-slug
append: |
  Additional content to add from this source.
===END===

Focus on extracting knowledge, not just summarizing. Each language section should be substantive, not a mere translation."""


def _legacy_compile_new(base_dir: Path | None = None, batch_size: int | None = None) -> list[str]:
    """Compile unprocessed raw documents into wiki articles."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    raw_dir = Path(cfg["paths"]["raw"])
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    section_headers = get_section_headers(cfg)

    if batch_size is None:
        batch_size = cfg.get("compile", {}).get("batch_size", 10)

    # Find uncompiled raw documents
    uncompiled = _find_uncompiled(raw_dir)
    if not uncompiled:
        return []

    batch = uncompiled[:batch_size]
    compiled_articles = []

    from .hooks import emit
    try:
        _preview_titles = [frontmatter.load(str(p)).metadata.get("title", p.parent.name) for p in batch[:5]]
    except Exception:
        _preview_titles = [p.parent.name for p in batch[:5]]
    emit("before_compile", batch_size=len(batch), titles=_preview_titles)

    # Load existing index for context
    index = _load_index(meta_dir)
    existing_concepts = _list_existing_concepts(concepts_dir)

    # Load compiled-sources log to avoid recompiling on volume reset
    compiled_log_path = meta_dir / "compiled_sources.json"
    if compiled_log_path.exists():
        compiled_sources = set(json.loads(compiled_log_path.read_text()))
    else:
        compiled_sources = set()

    for doc_path in batch:
        post = frontmatter.load(str(doc_path))
        title = post.metadata.get("title", doc_path.parent.name)
        content = post.content

        # Check if this source was already compiled (survives volume reset)
        source_key = post.metadata.get("source", "") or title
        if source_key in compiled_sources:
            # Already compiled before — mark and skip
            post.metadata["compiled"] = True
            doc_path.write_text(frontmatter.dumps(post), encoding="utf-8")
            continue

        if not content.strip():
            # Check for non-md files in the directory
            for f in doc_path.parent.iterdir():
                if f.suffix in (".txt", ".py", ".json", ".csv") and f.name != "index.md":
                    content += f"\n\n## File: {f.name}\n\n```\n{f.read_text(errors='ignore')[:5000]}\n```"

        # Ask LLM to extract concepts and write articles
        existing_text = (
            chr(10).join('  - ' + c for c in existing_concepts)
            if existing_concepts else '  (none yet)'
        )
        prompt = COMPILE_USER_PROMPT.format(
            title=title,
            content=content[:15000],
            existing=existing_text,
            article_format=COMPILE_ARTICLE_FORMAT,
        )

        response = chat(prompt, system=SYSTEM_PROMPT, max_tokens=cfg["llm"]["max_tokens"])

        # Build source reference from raw doc metadata
        raw_type = post.metadata.get("type", "unknown")
        source_id = _RAW_TYPE_TO_PLUGIN.get(raw_type, raw_type)
        source_ref = {
            "plugin": source_id,
            "url": post.metadata.get("source", ""),
            "title": title,
        }
        # Add plugin-specific fields
        for key in ("work_id", "canon", "work", "chapter", "book"):
            if key in post.metadata:
                source_ref[key] = post.metadata[key]

        # Parse response and write articles (with source ref)
        articles = _parse_compile_response(response)
        for article in articles:
            article["sources"] = [source_ref]
            article_path = _write_article(article, concepts_dir, headers=section_headers)
            if article_path:
                compiled_articles.append(str(article_path))
                existing_concepts.append(article["slug"])

        # Mark raw doc as compiled
        post.metadata["compiled"] = True
        post.metadata["compiled_at"] = datetime.now(timezone.utc).isoformat()
        doc_path.write_text(frontmatter.dumps(post), encoding="utf-8")

        # Log to compiled_sources (survives volume reset)
        source_key = post.metadata.get("source", "") or title
        compiled_sources.add(source_key)

        # Emit compiled hook — downstream can register callbacks for remote
        # sync, notifications, etc. via tools.hooks.register("compiled", ...)
        from .hooks import emit
        compile_work_id = (
            post.metadata.get("work_id")
            or post.metadata.get("work")
            or post.metadata.get("book")
        )
        emit(
            "compiled",
            source=source_id,
            work_id=compile_work_id,
            raw_type=raw_type,
            title=title,
            metadata=dict(post.metadata),
        )

    # Persist compiled sources log
    compiled_log_path.write_text(json.dumps(sorted(compiled_sources), ensure_ascii=False), encoding="utf-8")

    # Rebuild index
    rebuild_index(base_dir)

    # Assign new articles to taxonomy categories (no LLM, tag-based)
    if compiled_articles:
        try:
            from .taxonomy import assign_new_articles
            assign_new_articles(base_dir)
        except Exception:
            pass  # Non-critical

    if compiled_articles:
        emit("after_compile_batch", count=len(compiled_articles),
             articles=compiled_articles[:10])

    return compiled_articles


def _legacy_compile_all(base_dir: Path | None = None) -> list[str]:
    """Recompile everything - reset compiled flags and run."""
    cfg = load_config(base_dir)
    raw_dir = Path(cfg["paths"]["raw"])

    # Reset all compiled flags
    for doc_dir in raw_dir.iterdir():
        if not doc_dir.is_dir():
            continue
        index_path = doc_dir / "index.md"
        if index_path.exists():
            post = frontmatter.load(str(index_path))
            post.metadata["compiled"] = False
            index_path.write_text(frontmatter.dumps(post), encoding="utf-8")
        for md_file in doc_dir.glob("*.md"):
            if md_file.name != "index.md":
                post = frontmatter.load(str(md_file))
                post.metadata["compiled"] = False
                md_file.write_text(frontmatter.dumps(post), encoding="utf-8")

    return compile_new(base_dir, batch_size=999)


def _legacy_rebuild_index(base_dir: Path | None = None):
    """Rebuild the master index file from all wiki articles."""
    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    meta_dir.mkdir(parents=True, exist_ok=True)

    index_entries = []
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        entry = {
            "slug": md_file.stem,
            "title": post.metadata.get("title", md_file.stem),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []),
            "sources": post.metadata.get("sources", []),
        }
        index_entries.append(entry)

    # Write JSON index for programmatic access
    index_json_path = meta_dir / "index.json"
    from .atomic import atomic_write_json
    atomic_write_json(index_json_path, index_entries)

    # Write markdown index for Obsidian
    index_md = "---\ntitle: Wiki Index\nupdated: {}\n---\n\n# Knowledge Base Index\n\n".format(
        datetime.now(timezone.utc).isoformat()
    )
    # Group by tags
    tag_groups: dict[str, list] = {}
    for entry in index_entries:
        tags = entry.get("tags", [])
        if not tags:
            tags = ["uncategorized"]
        for tag in tags:
            tag_groups.setdefault(tag, []).append(entry)

    for tag in sorted(tag_groups.keys()):
        index_md += f"\n## {tag.title()}\n\n"
        for entry in tag_groups[tag]:
            index_md += f"- [[{entry['slug']}|{entry['title']}]] — {entry['summary']}\n"

    index_md += f"\n\n---\n*{len(index_entries)} articles indexed*\n"
    index_md_path = meta_dir / "_index.md"
    index_md_path.write_text(index_md, encoding="utf-8")

    # Build alias map (must come before backlinks)
    from .resolve import build_aliases, save_aliases
    aliases = build_aliases(concepts_dir)
    save_aliases(aliases, meta_dir)

    # Write backlinks map (uses aliases for correct resolution)
    _build_backlinks(concepts_dir, meta_dir)

    from .hooks import emit
    emit("index_rebuilt", article_count=len(index_entries))

    return index_entries


def _legacy_find_uncompiled(raw_dir: Path) -> list[Path]:
    """Find raw documents that haven't been compiled yet."""
    uncompiled = []
    if not raw_dir.exists():
        return uncompiled

    for doc_dir in sorted(raw_dir.iterdir()):
        if not doc_dir.is_dir():
            continue
        index_path = doc_dir / "index.md"
        if index_path.exists():
            post = frontmatter.load(str(index_path))
            if not post.metadata.get("compiled", False):
                uncompiled.append(index_path)
        else:
            for md_file in sorted(doc_dir.glob("*.md")):
                post = frontmatter.load(str(md_file))
                if not post.metadata.get("compiled", False):
                    uncompiled.append(md_file)
                    break
    return uncompiled


def _legacy_load_index(meta_dir: Path) -> list[dict]:
    """Load existing index."""
    index_path = meta_dir / "index.json"
    if index_path.exists():
        return json.loads(index_path.read_text())
    return []


def _legacy_list_existing_concepts(concepts_dir: Path) -> list[str]:
    """List existing concepts as 'slug (title)' for LLM disambiguation."""
    if not concepts_dir.exists():
        return []
    results = []
    for f in concepts_dir.glob("*.md"):
        post = frontmatter.load(str(f))
        title = post.metadata.get("title", f.stem)
        results.append(f"{f.stem} ({title})")
    return results


def _legacy_parse_compile_response(response: str) -> list[dict]:
    """Parse LLM response into article dicts."""
    articles = []

    # Parse ===ARTICLE=== blocks
    parts = response.split("===ARTICLE===")
    for part in parts[1:]:
        end_idx = part.find("===END===")
        if end_idx == -1:
            block = part.strip()
        else:
            block = part[:end_idx].strip()

        article = _parse_article_block(block)
        if article:
            article["type"] = "new"
            articles.append(article)

    # Parse ===UPDATE=== blocks
    parts = response.split("===UPDATE===")
    for part in parts[1:]:
        end_idx = part.find("===END===")
        if end_idx == -1:
            block = part.strip()
        else:
            block = part[:end_idx].strip()

        article = _parse_update_block(block)
        if article:
            article["type"] = "update"
            articles.append(article)

    return articles


def _legacy_parse_article_block(block: str) -> dict | None:
    """Parse a single article block."""
    lines = block.strip().split("\n")
    meta = {}
    content_start = 0

    for i, line in enumerate(lines):
        if line.strip() == "---":
            content_start = i + 1
            break
        if ":" in line:
            key, _, value = line.partition(":")
            key = key.strip()
            value = value.strip()
            if key in ("slug", "title", "summary"):
                meta[key] = value
            elif key == "tags":
                meta["tags"] = [t.strip() for t in value.split(",")]

    if "slug" not in meta:
        return None

    content = "\n".join(lines[content_start:]).strip()
    meta["content"] = content
    return meta


def _legacy_parse_update_block(block: str) -> dict | None:
    """Parse an update block."""
    lines = block.strip().split("\n")
    slug = None
    append_content = []
    in_append = False

    for line in lines:
        if line.strip().startswith("slug:"):
            slug = line.split(":", 1)[1].strip()
        elif line.strip().startswith("append:"):
            in_append = True
            rest = line.split(":", 1)[1].strip()
            if rest and rest != "|":
                append_content.append(rest)
        elif in_append:
            append_content.append(line)

    if not slug:
        return None

    return {"slug": slug, "content": "\n".join(append_content).strip()}


def _legacy_write_article(
    article: dict,
    concepts_dir: Path,
    headers: list[tuple[str, str]] | None = None,
) -> Path | None:
    """Write or update an article file. Merges into existing articles.

    Three-layer duplicate prevention:
    1. Exact slug match → merge
    2. Alias resolution (title, slug, CJK variants) → merge
    3. CJK substring scan across all articles → merge
    """
    import re as _re
    from .resolve import build_aliases, resolve_link

    slug = sanitize_slug(article["slug"])
    if not slug:
        return None
    article["slug"] = slug
    article_path = (concepts_dir / f"{slug}.md").resolve()
    # Path traversal guard
    if not str(article_path).startswith(str(concepts_dir.resolve())):
        return None

    # Layer 1: exact slug match
    if article_path.exists():
        _merge_into(article_path, article, headers=headers)
        return article_path

    # Layer 2: alias resolution
    aliases = build_aliases(concepts_dir)
    title = article.get("title", slug)
    candidates = [slug, title] + [p.strip() for p in title.split("/") if p.strip()]
    for candidate in candidates:
        resolved = resolve_link(candidate, aliases)
        if resolved and resolved != slug:
            existing_path = concepts_dir / f"{resolved}.md"
            if existing_path.exists():
                _merge_into(existing_path, article, headers=headers)
                return existing_path

    # Layer 3: CJK substring scan — catches variant titles (e.g., "X说" matching "X")
    new_cjk = _re.sub(r'[^\u4e00-\u9fff\u3400-\u4dbf]', '', title)
    if new_cjk:
        for md_file in concepts_dir.glob("*.md"):
            existing_post = frontmatter.load(str(md_file))
            existing_title = existing_post.metadata.get("title", "")
            existing_cjk = _re.sub(r'[^\u4e00-\u9fff\u3400-\u4dbf]', '', existing_title)
            if not existing_cjk:
                continue
            # Exact CJK match (handles single chars: 仁 == 仁)
            if new_cjk == existing_cjk:
                _merge_into(md_file, article, headers=headers)
                return md_file
            # Substring match for 2+ chars with 60% length ratio
            short, long = (new_cjk, existing_cjk) if len(new_cjk) <= len(existing_cjk) else (existing_cjk, new_cjk)
            if len(short) >= 2 and short in long and len(short) / len(long) >= 0.6:
                _merge_into(md_file, article, headers=headers)
                return md_file

    # Truly new article
    post = frontmatter.Post(article.get("content", ""))
    post.metadata["title"] = article.get("title", slug)
    post.metadata["summary"] = article.get("summary", "")
    post.metadata["tags"] = article.get("tags", [])
    post.metadata["sources"] = article.get("sources", [])
    post.metadata["created"] = datetime.now(timezone.utc).isoformat()
    post.metadata["updated"] = datetime.now(timezone.utc).isoformat()
    article_path.write_text(frontmatter.dumps(post), encoding="utf-8")
    return article_path


def _legacy_merge_into(
    existing_path: Path,
    article: dict,
    headers: list[tuple[str, str]] | None = None,
):
    """Merge new article content into an existing article (叠加进化).

    Section-level dedup: splits by configured SECTION_HEADERS,
    keeps the longer version of each section. Never blindly appends
    entire content blocks — prevents duplicate sections.
    """
    headers = SECTION_HEADERS if headers is None else headers

    existing = frontmatter.load(str(existing_path))
    new_content = article.get("content", "")
    if not new_content or not new_content.strip():
        return None

    # Split both into language sections
    existing_sections = _split_sections(existing.content, headers=headers)
    new_sections = _split_sections(new_content, headers=headers)

    changed = False
    for lang_key, _ in headers:
        new_sec = new_sections.get(lang_key, "").strip()
        old_sec = existing_sections.get(lang_key, "").strip()

        if not new_sec:
            continue

        if not old_sec:
            # Add missing language section
            existing_sections[lang_key] = new_sec
            changed = True
        elif len(new_sec) > len(old_sec) * 1.2:
            # New version is significantly longer — replace
            existing_sections[lang_key] = new_sec
            changed = True
        # Otherwise keep existing (avoid duplication)

    # Merge sources (deduplicate by stable key)
    new_sources = article.get("sources", [])
    if new_sources:
        existing_sources = existing.metadata.get("sources", [])

        def _source_key(s):
            return (s.get("plugin", ""), s.get("url", ""),
                    s.get("work_id", ""), s.get("title", ""))

        existing_keys = {_source_key(s) for s in existing_sources}
        added = False
        for src in new_sources:
            if _source_key(src) not in existing_keys:
                existing_sources.append(src)
                existing_keys.add(_source_key(src))
                added = True
        if added:
            existing.metadata["sources"] = existing_sources
            changed = True

    if changed:
        # Reassemble content from sections
        existing.content = _assemble_sections(existing_sections, headers=headers)
        existing.metadata["updated"] = datetime.now(timezone.utc).isoformat()
        old_tags = set(existing.metadata.get("tags", []))
        new_tags = set(article.get("tags", []))
        existing.metadata["tags"] = sorted(old_tags | new_tags)
        existing_path.write_text(frontmatter.dumps(existing), encoding="utf-8")

    return None


def _legacy_split_sections(
    content: str,
    headers: list[tuple[str, str]] | None = None,
) -> dict[str, str]:
    """Split article into {section_key: content} dict.

    Recognises headers defined in the module-level SECTION_HEADERS list,
    so downstream projects that override SECTION_HEADERS (e.g. to a
    single "## 文言" section) will get correct splitting automatically.
    """
    import re
    headers = SECTION_HEADERS if headers is None else headers

    if len(headers) == 1 and headers[0][1] == "":
        return {"_preamble": "", headers[0][0]: content}

    sections: dict[str, str] = {"_preamble": ""}
    current = "_preamble"

    # Build header → key mapping from SECTION_HEADERS
    header_map: list[tuple[str, re.Pattern]] = []
    for key, header in headers:
        if header == "":
            continue
        # Build a regex: "## English" → r"^## English\s*$"
        escaped = re.escape(header)
        flags = re.IGNORECASE if key.isascii() else 0
        header_map.append((key, re.compile(rf"^{escaped}\s*$", flags)))

    for line in content.split("\n"):
        matched = False
        for key, pat in header_map:
            if pat.match(line):
                current = key
                sections.setdefault(current, "")
                matched = True
                break
        if matched:
            continue
        if re.match(r"^---$", line) and current != "_preamble":
            continue
        sections[current] = sections.get(current, "") + line + "\n"

    return sections


def _legacy_assemble_sections(
    sections: dict[str, str],
    headers: list[tuple[str, str]] | None = None,
) -> str:
    """Reassemble sections into a single content string.

    Uses SECTION_HEADERS for ordering, so downstream overrides are respected.
    """
    headers = SECTION_HEADERS if headers is None else headers
    parts = []
    preamble = sections.get("_preamble", "").strip()
    if preamble:
        parts.append(preamble)

    for lang, header in headers:
        sec = sections.get(lang, "").strip()
        if sec:
            parts.append(sec if header == "" else f"{header}\n\n{sec}")

    return "\n\n".join(parts)


def _legacy_build_backlinks(concepts_dir: Path, meta_dir: Path):
    """Build a backlinks map from wiki-link references.

    Uses alias resolution so that [[参禅]] correctly maps to the
    canonical slug 'can-chan' instead of the raw Chinese text.
    """
    import re
    from .resolve import load_aliases, resolve_link

    aliases = load_aliases(meta_dir)
    backlinks: dict[str, list[str]] = {}
    link_pattern = re.compile(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")

    for md_file in concepts_dir.glob("*.md"):
        content = md_file.read_text()
        slug = md_file.stem
        for match in link_pattern.finditer(content):
            raw_target = match.group(1).strip()
            # Resolve via aliases; fall back to old normalization
            resolved = resolve_link(raw_target, aliases)
            target_key = resolved or raw_target.lower().replace(" ", "-")
            backlinks.setdefault(target_key, [])
            if slug not in backlinks[target_key]:
                backlinks[target_key].append(slug)

    backlinks_path = meta_dir / "backlinks.json"
    from .atomic import atomic_write_json
    atomic_write_json(backlinks_path, backlinks)


# These functions retain the imported public/private names and resolve effective
# module settings at invocation time. The default native path performs actual
# parsing, reads, merge decisions, and journaled writes in the shared Rust core.
_active_compile_config: ContextVar[dict | None] = ContextVar("llmwiki_compile_config", default=None)
_active_raw_source: ContextVar[dict | None] = ContextVar("llmwiki_compile_raw_source", default=None)
_DEFAULT_CHAT = chat


def _native_compile(operation: str, args: dict | None = None, *, cfg=None, **roots):
    cfg = cfg or _active_compile_config.get()
    if cfg is None:
        # Private helpers accept a standalone configured directory. Derive a
        # local metadata root when no compile invocation supplies full config.
        # This call context is transient: reading config here must not publish
        # an inferred library identity to the native registry.
        selected = next(iter(roots.values()), Path.cwd())
        selected = Path(selected).resolve()
        cfg = _read_config(selected.parent)
        if "concepts" in roots:
            cfg["paths"]["meta"] = str(selected.parent / "_meta")
    paths = dict(cfg["paths"])
    paths.update({key: str(Path(path).resolve()) for key, path in roots.items()})
    return _native.dispatch("compat_compile_" + operation, args or {}, cfg.get("base_dir"), paths=paths)["value"]


def sanitize_slug(slug: str) -> str:
    if not _native.available():
        return _legacy_sanitize_slug(slug)
    if not slug:
        return ""
    return _native.compile_transform("sanitize_slug", {"slug": slug})


def _split_sections(content: str, headers: list[tuple[str, str]] | None = None) -> dict[str, str]:
    if not _native.available():
        return _legacy_split_sections(content, headers)
    return _native.compile_transform("split", {"content": content, "headers": SECTION_HEADERS if headers is None else headers})


def _assemble_sections(sections: dict[str, str], headers: list[tuple[str, str]] | None = None) -> str:
    if not _native.available():
        return _legacy_assemble_sections(sections, headers)
    return _native.compile_transform("assemble", {"sections": sections, "headers": SECTION_HEADERS if headers is None else headers})


def _parse_article_block(block: str) -> dict | None:
    if not _native.available():
        return _legacy_parse_article_block(block)
    return _native.compile_transform("parse_article", {"block": block})


def _parse_update_block(block: str) -> dict | None:
    if not _native.available():
        return _legacy_parse_update_block(block)
    return _native.compile_transform("parse_update", {"block": block})


def _parse_compile_response(response: str) -> list[dict]:
    if not _native.available():
        return _legacy_parse_compile_response(response)
    return _native.compile_transform("parse_response", {"response": response})


def _find_uncompiled(raw_dir: Path) -> list[Path]:
    if not _native.available():
        return _legacy_find_uncompiled(raw_dir)
    raw_dir = Path(raw_dir)
    paths = [Path(path) for path in _native_compile("find", raw=raw_dir)]
    return paths if raw_dir.is_absolute() else [raw_dir / path.relative_to(raw_dir.resolve()) for path in paths]


def _load_index(meta_dir: Path) -> list[dict]:
    if not _native.available():
        return _legacy_load_index(meta_dir)
    return _native_compile("load_index", meta=meta_dir)


def _list_existing_concepts(concepts_dir: Path) -> list[str]:
    if not _native.available():
        return _legacy_list_existing_concepts(concepts_dir)
    return _native_compile("existing", concepts=concepts_dir)


def _write_article(article: dict, concepts_dir: Path, headers: list[tuple[str, str]] | None = None) -> Path | None:
    if not _native.available():
        return _legacy_write_article(article, concepts_dir, headers)
    result = _native_compile("write", {"article": article, "headers": SECTION_HEADERS if headers is None else headers,
                                     "raw_source": _active_raw_source.get()}, concepts=concepts_dir)
    article["slug"] = result["slug"]
    return Path(result["path"]) if result["path"] is not None else None


def _merge_into(existing_path: Path, article: dict, headers: list[tuple[str, str]] | None = None):
    if not _native.available():
        return _legacy_merge_into(existing_path, article, headers)
    existing_path = Path(existing_path)
    _native_compile("write", {"relative": existing_path.name, "article": article,
                              "headers": SECTION_HEADERS if headers is None else headers,
                              "raw_source": _active_raw_source.get()}, concepts=existing_path.parent)
    return None


def rebuild_index(base_dir: Path | None = None):
    if not _native.available():
        return _legacy_rebuild_index(base_dir)
    entries = _native_compile("rebuild", cfg=load_config(base_dir))
    from .hooks import emit
    emit("index_rebuilt", article_count=len(entries))
    return entries


def _build_backlinks(concepts_dir: Path, meta_dir: Path):
    if not _native.available():
        return _legacy_build_backlinks(concepts_dir, meta_dir)
    _native_compile("backlinks", concepts=concepts_dir, meta=meta_dir)
    return None


def compile_all(base_dir: Path | None = None) -> list[str]:
    if not _native.available():
        return _legacy_compile_all(base_dir)
    _native_compile("reset", cfg=load_config(base_dir))
    return compile_new(base_dir, batch_size=999)


def compile_new(base_dir: Path | None = None, batch_size: int | None = None) -> list[str]:
    if not _native.available():
        return _legacy_compile_new(base_dir, batch_size)
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    token = _active_compile_config.set(cfg)
    try:
        return _compile_new_native(cfg, base_dir, batch_size)
    finally:
        _active_compile_config.reset(token)


def _compile_new_native(cfg: dict, base_dir: Path | None, batch_size: int | None) -> list[str]:
    from .hooks import emit
    raw_dir = Path(cfg["paths"]["raw"]).resolve()
    concepts_dir = Path(cfg["paths"]["concepts"]).resolve()
    meta_dir = Path(cfg["paths"]["meta"]).resolve()
    section_headers = get_section_headers(cfg)
    if batch_size is None:
        batch_size = cfg.get("compile", {}).get("batch_size", 10)
    batch = _find_uncompiled(raw_dir)[:batch_size]
    if not batch:
        return []
    def snapshot(path, attachments=False):
        return _native_compile("read_raw", {"relative": str(path.relative_to(raw_dir)), "attachments": attachments})
    preview = [snapshot(path)["metadata"].get("title", path.parent.name) for path in batch[:5]]
    emit("before_compile", batch_size=len(batch), titles=preview)
    _load_index(meta_dir)
    existing_concepts = _list_existing_concepts(concepts_dir)
    compiled_sources = set(_native_compile("sources"))
    compiled_articles = []
    for doc_path in batch:
        saved = snapshot(doc_path, attachments=True)
        metadata, content = saved["metadata"], saved["content"]
        title = metadata.get("title", doc_path.parent.name)
        source_key = metadata.get("source", "") or title
        guard = {"relative": str(doc_path.relative_to(raw_dir)), "content_hash": saved["content_hash"]}
        if source_key in compiled_sources:
            _native_compile("mark", {**guard, "compiled": True, "with_timestamp": False})
            continue
        prompt = COMPILE_USER_PROMPT.format(title=title, content=content[:15000],
            existing="\n".join("  - " + concept for concept in existing_concepts) if existing_concepts else "  (none yet)",
            article_format=COMPILE_ARTICLE_FORMAT)
        if chat is _DEFAULT_CHAT:
            from .llm import chat_with_meta
            response, model_meta = chat_with_meta(prompt, system=SYSTEM_PROMPT, max_tokens=cfg["llm"]["max_tokens"])
            if model_meta.truncated:
                raise ValueError("compile response was truncated; source remains uncompiled")
        else:
            response = chat(prompt, system=SYSTEM_PROMPT, max_tokens=cfg["llm"]["max_tokens"])
        raw_type = metadata.get("type", "unknown")
        source_id = _RAW_TYPE_TO_PLUGIN.get(raw_type, raw_type)
        source_ref = {"plugin": source_id, "url": metadata.get("source", ""), "title": title}
        for key in ("work_id", "canon", "work", "chapter", "book"):
            if key in metadata:
                source_ref[key] = metadata[key]
        articles = _parse_compile_response(response)
        if not articles or not any(article.get("content", "").strip() for article in articles):
            raise ValueError("compile response contains no usable article; source remains uncompiled")
        applied = []
        source_token = _active_raw_source.set(guard)
        try:
            for article in articles:
                article["sources"] = [source_ref]
                article_path = _write_article(article, concepts_dir, headers=section_headers)
                if article_path:
                    applied.append(str(article_path))
                    existing_concepts.append(article["slug"])
        finally:
            _active_raw_source.reset(source_token)
        if not applied:
            raise ValueError("compile response produced no article; source remains uncompiled")
        compiled_articles.extend(applied)
        marked = _native_compile("mark", {**guard, "compiled": True, "with_timestamp": True, "source_key": source_key})
        compiled_sources.add(source_key)
        emit("compiled", source=source_id, work_id=metadata.get("work_id") or metadata.get("work") or metadata.get("book"),
             raw_type=raw_type, title=title, metadata=marked["metadata"])
    rebuild_index(base_dir)
    if compiled_articles:
        try:
            from .taxonomy import assign_new_articles
            assign_new_articles(base_dir)
        except Exception:
            pass
        emit("after_compile_batch", count=len(compiled_articles), articles=compiled_articles[:10])
    return compiled_articles
