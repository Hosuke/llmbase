"""Lint module: health checks, consistency checks, and wiki enhancement."""

import json
import re
from pathlib import Path

import frontmatter

from .config import load_config, ensure_dirs
from .llm import chat


SYSTEM_PROMPT = """You are a knowledge base quality analyst. Your job is to review wiki articles
and identify issues, inconsistencies, and opportunities for improvement.

Be specific and actionable in your findings. Reference article titles and specific content."""


STUB_SYSTEM_PROMPT = """You are a trilingual knowledge base compiler. Generate a stub article
for a missing concept based on how existing articles reference it.

Rules:
- Write in three languages: English, 中文, 日本語 (each under an h2 header)
- Keep it concise but informative (2-3 paragraphs per language)
- Use [[wiki-link]] for cross-references to related concepts
- Base your content ONLY on what can be reasonably inferred from the provided contexts
- If you cannot determine what the concept is about, respond with exactly: CANNOT_GENERATE"""


def lint(base_dir: Path | None = None) -> dict:
    """Run all lint checks on the wiki."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)

    results = {
        "structural": check_structural(cfg),
        "broken_links": check_broken_links(cfg),
        "orphans": check_orphans(cfg),
        "missing_metadata": check_missing_metadata(cfg),
    }

    # Count total issues
    total = sum(len(v) for v in results.values())
    results["total_issues"] = total

    return results


def lint_deep(base_dir: Path | None = None) -> str:
    """Use LLM to do a deep quality check on the wiki content."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])

    articles = []
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        articles.append({
            "slug": md_file.stem,
            "title": post.metadata.get("title", md_file.stem),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []),
            "content_preview": post.content[:500],
        })

    if not articles:
        return "No articles to lint. Run `llmbase compile` first."

    articles_text = json.dumps(articles, indent=2, ensure_ascii=False)

    prompt = f"""Review this knowledge base wiki for quality issues.

Articles:
{articles_text}

Please check for and report:
1. **Inconsistencies**: Contradictory information across articles
2. **Missing data**: Important topics referenced but not yet covered
3. **Weak connections**: Concepts that should be linked but aren't
4. **New article candidates**: Connections between existing concepts that deserve their own article
5. **Suggested questions**: Further research questions worth exploring based on the knowledge base

Format your response as a structured markdown report."""

    return chat(prompt, system=SYSTEM_PROMPT, max_tokens=cfg["llm"]["max_tokens"])


def check_structural(cfg: dict) -> list[str]:
    """Check for structural issues."""
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    if not (meta_dir / "_index.md").exists():
        issues.append("Missing master index file (_index.md)")

    if not (meta_dir / "index.json").exists():
        issues.append("Missing JSON index (index.json)")

    article_count = len(list(concepts_dir.glob("*.md")))
    if article_count == 0:
        issues.append("No articles in the wiki")

    return issues


def check_broken_links(cfg: dict) -> list[str]:
    """Find broken wiki-links [[target]] that don't have corresponding articles."""
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    link_pattern = re.compile(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")
    existing_slugs = {f.stem.lower() for f in concepts_dir.glob("*.md")}

    for md_file in concepts_dir.glob("*.md"):
        content = md_file.read_text()
        for match in link_pattern.finditer(content):
            target = match.group(1).strip().lower().replace(" ", "-")
            if target not in existing_slugs:
                issues.append(f"Broken link in {md_file.stem}: [[{match.group(1)}]]")

    return issues


def check_orphans(cfg: dict) -> list[str]:
    """Find articles that are not linked to from any other article."""
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    backlinks_path = meta_dir / "backlinks.json"
    if not backlinks_path.exists():
        return ["Backlinks map not built yet"]

    backlinks = json.loads(backlinks_path.read_text())
    linked_slugs = set(backlinks.keys())

    for md_file in concepts_dir.glob("*.md"):
        slug = md_file.stem.lower()
        if slug not in linked_slugs:
            issues.append(f"Orphan article (no incoming links): {md_file.stem}")

    return issues


def check_missing_metadata(cfg: dict) -> list[str]:
    """Find articles with missing or incomplete metadata."""
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])

    for md_file in concepts_dir.glob("*.md"):
        post = frontmatter.load(str(md_file))
        slug = md_file.stem
        if not post.metadata.get("title"):
            issues.append(f"Missing title: {slug}")
        if not post.metadata.get("summary"):
            issues.append(f"Missing summary: {slug}")
        if not post.metadata.get("tags"):
            issues.append(f"Missing tags: {slug}")

    return issues


def fix_broken_links(base_dir: Path | None = None, max_stubs: int = 10) -> list[str]:
    """Generate stub articles for broken wiki-link targets.

    Strategy A: Use LLM to generate a trilingual stub from referencing context.
    Strategy B: If LLM fails, create a minimal placeholder stub.
    Returns list of fix descriptions.
    """
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])
    link_pattern = re.compile(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")
    existing_slugs = {f.stem.lower() for f in concepts_dir.glob("*.md")}

    # Collect broken links grouped by target slug
    # target_slug -> [(source_slug, context_snippet)]
    missing: dict[str, list[tuple[str, str]]] = {}

    for md_file in concepts_dir.glob("*.md"):
        content = md_file.read_text()
        for match in link_pattern.finditer(content):
            raw_target = match.group(1).strip()
            target_slug = raw_target.lower().replace(" ", "-")
            if target_slug in existing_slugs:
                continue
            # Extract context around the link (200 chars each side)
            start = max(0, match.start() - 200)
            end = min(len(content), match.end() + 200)
            snippet = content[start:end].strip()
            missing.setdefault(target_slug, []).append((md_file.stem, snippet))

    if not missing:
        return []

    fixes = []
    from datetime import datetime, timezone

    for target_slug, refs in list(missing.items())[:max_stubs]:
        # Build context from all referencing articles
        contexts = "\n\n---\n\n".join(
            f"From article '{src}':\n...{snippet}..."
            for src, snippet in refs[:5]  # cap context to 5 references
        )
        ref_slugs = [src for src, _ in refs]

        # Strategy A: LLM-generated stub
        article_content = None
        title = target_slug.replace("-", " ").title()
        summary = ""
        tags = ["stub"]

        try:
            prompt = (
                f"The concept '{target_slug}' is referenced in existing articles but has no article yet.\n\n"
                f"Here is how it appears in context:\n\n{contexts}\n\n"
                f"Generate a wiki article in this exact format (no extra text before or after):\n\n"
                f"title: English Title / 中文标题\n"
                f"summary: One-line summary in English\n"
                f"tags: tag1, tag2\n"
                f"---\n"
                f"## English\n\n(content)\n\n"
                f"## 中文\n\n(内容)\n\n"
                f"## 日本語\n\n(内容)"
            )
            response = chat(prompt, system=STUB_SYSTEM_PROMPT, max_tokens=2048)

            if "CANNOT_GENERATE" not in response and len(response.strip()) > 100:
                # Parse the LLM response
                lines = response.strip().split("\n")
                body_start = 0
                for i, line in enumerate(lines):
                    if line.strip() == "---":
                        body_start = i + 1
                        break
                    if ":" in line:
                        key, _, val = line.partition(":")
                        key = key.strip().lower()
                        val = val.strip()
                        if key == "title" and val:
                            title = val
                        elif key == "summary" and val:
                            summary = val
                        elif key == "tags" and val:
                            tags = [t.strip().lower() for t in val.split(",")]
                            if "stub" not in tags:
                                tags.append("stub")

                article_content = "\n".join(lines[body_start:]).strip()
        except Exception:
            pass  # Fall through to Strategy B

        # Strategy B: minimal stub
        if not article_content or len(article_content) < 50:
            referrers = ", ".join(f"[[{s}]]" for s in ref_slugs[:5])
            article_content = (
                f"## English\n\n"
                f"This article has not been fully written yet. "
                f"It is referenced by: {referrers}.\n\n"
                f"## 中文\n\n"
                f"本条目尚未完成撰写。引用来源：{referrers}。\n\n"
                f"## 日本語\n\n"
                f"この記事はまだ完成していません。参照元：{referrers}。"
            )
            summary = summary or "Stub article — referenced but not yet written"
            title = target_slug.replace("-", " ").title()

        # Write the stub article
        post = frontmatter.Post(article_content)
        post.metadata["title"] = title
        post.metadata["summary"] = summary
        post.metadata["tags"] = tags
        post.metadata["stub"] = True
        post.metadata["created"] = datetime.now(timezone.utc).isoformat()
        post.metadata["updated"] = datetime.now(timezone.utc).isoformat()

        article_path = concepts_dir / f"{target_slug}.md"
        article_path.write_text(frontmatter.dumps(post), encoding="utf-8")
        fixes.append(f"Created stub for broken link: {target_slug} (referenced by {len(refs)} article(s))")

    # Rebuild index so new stubs appear in search and backlinks
    if fixes:
        from .compile import rebuild_index
        rebuild_index(base_dir)

    return fixes


def auto_fix(base_dir: Path | None = None) -> list[str]:
    """Attempt to auto-fix common lint issues using LLM."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])
    fixes = []

    # Fix missing metadata
    for md_file in concepts_dir.glob("*.md"):
        post = frontmatter.load(str(md_file))
        needs_fix = False

        if not post.metadata.get("summary") and post.content.strip():
            prompt = f"Write a one-line summary for this article:\n\n# {post.metadata.get('title', md_file.stem)}\n\n{post.content[:2000]}"
            summary = chat(prompt, max_tokens=256)
            post.metadata["summary"] = summary.strip().strip('"')
            needs_fix = True

        if not post.metadata.get("tags") and post.content.strip():
            prompt = f"List 2-4 relevant tags for this article (comma-separated, lowercase):\n\n# {post.metadata.get('title', md_file.stem)}\n\n{post.content[:2000]}"
            tags = chat(prompt, max_tokens=128)
            post.metadata["tags"] = [t.strip().lower() for t in tags.split(",")]
            needs_fix = True

        if needs_fix:
            md_file.write_text(frontmatter.dumps(post), encoding="utf-8")
            fixes.append(f"Fixed metadata for: {md_file.stem}")

    # Fix broken links by generating stubs
    health_cfg = cfg.get("health", {})
    if health_cfg.get("auto_fix_broken_links", True):
        max_stubs = health_cfg.get("max_stubs_per_run", 10)
        link_fixes = fix_broken_links(base_dir, max_stubs)
        fixes.extend(link_fixes)

    return fixes
