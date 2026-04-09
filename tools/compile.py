"""Compile module: LLM reads raw/ and builds a structured wiki."""

import json
from datetime import datetime, timezone
from pathlib import Path

import frontmatter

from .config import load_config, ensure_dirs
from .llm import chat


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


def compile_new(base_dir: Path | None = None, batch_size: int | None = None) -> list[str]:
    """Compile unprocessed raw documents into wiki articles."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    raw_dir = Path(cfg["paths"]["raw"])
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    if batch_size is None:
        batch_size = cfg.get("compile", {}).get("batch_size", 10)

    # Find uncompiled raw documents
    uncompiled = _find_uncompiled(raw_dir)
    if not uncompiled:
        return []

    batch = uncompiled[:batch_size]
    compiled_articles = []

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
        prompt = f"""I have a raw document titled "{title}" that needs to be compiled into wiki articles.

Source document:
---
{content[:15000]}
---

EXISTING ARTICLES (you MUST reuse these — DO NOT create new articles for concepts that already exist):
{chr(10).join('  - ' + c for c in existing_concepts) if existing_concepts else '  (none yet)'}

CRITICAL DEDUPLICATION RULES:
- If a concept ALREADY EXISTS above (even under a different name, translation, or variant), you MUST use ===UPDATE=== with the EXISTING slug
- A concept with a suffix (e.g., "X说", "X论", "X位") is usually the SAME as the base concept "X" — use UPDATE
- A concept in one language (e.g., Chinese title) that matches an existing concept in another language (e.g., English slug) is the SAME — use UPDATE
- When in doubt, UPDATE an existing article rather than creating a new one
- New articles should only be created for genuinely NEW concepts not covered above

Please:
1. Identify the key concepts from this document (1-5 concepts)
2. For each concept, produce a TRILINGUAL wiki article in this exact format:

===ARTICLE===
slug: concept-name-here
title: English Title / 中文标题
summary: One-line summary in English
tags: tag1, tag2, tag3
---
## English

Full article content in English. Use [[Other Concept]] for cross-references.

## 中文

完整的中文文章内容。使用中文学术风格撰写，不是简单翻译。使用 [[Other Concept]] 进行交叉引用。

## 日本語

完全な日本語の記事内容。学術的な日本語で記述する。[[Other Concept]] でクロスリファレンスを使用する。
===END===

If a concept already exists in the wiki, instead output:
===UPDATE===
slug: existing-concept-slug
append: |
  Additional trilingual content to add from this source.
===END===

Focus on extracting knowledge, not just summarizing. Each language section should be substantive, not a mere translation."""

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
            article_path = _write_article(article, concepts_dir)
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

    return compiled_articles


def compile_all(base_dir: Path | None = None) -> list[str]:
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


def rebuild_index(base_dir: Path | None = None):
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

    return index_entries


def _find_uncompiled(raw_dir: Path) -> list[Path]:
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


def _load_index(meta_dir: Path) -> list[dict]:
    """Load existing index."""
    index_path = meta_dir / "index.json"
    if index_path.exists():
        return json.loads(index_path.read_text())
    return []


def _list_existing_concepts(concepts_dir: Path) -> list[str]:
    """List existing concepts as 'slug (title)' for LLM disambiguation."""
    if not concepts_dir.exists():
        return []
    results = []
    for f in concepts_dir.glob("*.md"):
        post = frontmatter.load(str(f))
        title = post.metadata.get("title", f.stem)
        results.append(f"{f.stem} ({title})")
    return results


def _parse_compile_response(response: str) -> list[dict]:
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


def _parse_article_block(block: str) -> dict | None:
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


def _parse_update_block(block: str) -> dict | None:
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


def _write_article(article: dict, concepts_dir: Path) -> Path | None:
    """Write or update an article file. Merges into existing articles.

    Three-layer duplicate prevention:
    1. Exact slug match → merge
    2. Alias resolution (title, slug, CJK variants) → merge
    3. CJK substring scan across all articles → merge
    """
    import re as _re
    from .resolve import build_aliases, resolve_link

    slug = article["slug"]
    # Sanitize slug: prevent path traversal and invalid filenames
    slug = slug.replace("/", "-").replace("\\", "-").replace("..", "").strip(".-_ ")
    if not slug:
        return None
    article["slug"] = slug
    article_path = (concepts_dir / f"{slug}.md").resolve()
    # Path traversal guard
    if not str(article_path).startswith(str(concepts_dir.resolve())):
        return None

    # Layer 1: exact slug match
    if article_path.exists():
        _merge_into(article_path, article)
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
                _merge_into(existing_path, article)
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
                _merge_into(md_file, article)
                return md_file
            # Substring match for 2+ chars with 60% length ratio
            short, long = (new_cjk, existing_cjk) if len(new_cjk) <= len(existing_cjk) else (existing_cjk, new_cjk)
            if len(short) >= 2 and short in long and len(short) / len(long) >= 0.6:
                _merge_into(md_file, article)
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


def _merge_into(existing_path: Path, article: dict):
    """Merge new article content into an existing article (叠加进化).

    Section-level dedup: splits by ## English / ## 中文 / ## 日本語,
    keeps the longer version of each section. Never blindly appends
    entire content blocks — prevents 6× duplicate sections.
    """
    import re

    existing = frontmatter.load(str(existing_path))
    new_content = article.get("content", "")
    if not new_content or not new_content.strip():
        return None

    # Split both into language sections
    existing_sections = _split_sections(existing.content)
    new_sections = _split_sections(new_content)

    changed = False
    for lang_key in ("english", "中文", "日本語"):
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
        existing.content = _assemble_sections(existing_sections)
        existing.metadata["updated"] = datetime.now(timezone.utc).isoformat()
        old_tags = set(existing.metadata.get("tags", []))
        new_tags = set(article.get("tags", []))
        existing.metadata["tags"] = sorted(old_tags | new_tags)
        existing_path.write_text(frontmatter.dumps(existing), encoding="utf-8")

    return None


def _split_sections(content: str) -> dict[str, str]:
    """Split trilingual article into {lang: content} dict."""
    import re
    sections = {"_preamble": ""}
    current = "_preamble"

    for line in content.split("\n"):
        if re.match(r"^## English\s*$", line, re.IGNORECASE):
            current = "english"
            sections.setdefault(current, "")
            continue
        elif re.match(r"^## 中文\s*$", line):
            current = "中文"
            sections.setdefault(current, "")
            continue
        elif re.match(r"^## 日本語\s*$", line):
            current = "日本語"
            sections.setdefault(current, "")
            continue
        elif re.match(r"^---$", line) and current != "_preamble":
            # Section separator from previous merges — skip
            continue

        sections[current] = sections.get(current, "") + line + "\n"

    return sections


def _assemble_sections(sections: dict[str, str]) -> str:
    """Reassemble sections into a single content string."""
    parts = []
    preamble = sections.get("_preamble", "").strip()
    if preamble:
        parts.append(preamble)

    for lang, header in [("english", "## English"), ("中文", "## 中文"), ("日本語", "## 日本語")]:
        sec = sections.get(lang, "").strip()
        if sec:
            parts.append(f"{header}\n\n{sec}")

    return "\n\n".join(parts)


def _build_backlinks(concepts_dir: Path, meta_dir: Path):
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
