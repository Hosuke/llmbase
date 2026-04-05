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
        "dirty_tags": check_dirty_tags(cfg),
        "duplicates": check_duplicates(cfg),
        "stubs": check_stubs(cfg),
        "uncategorized": check_uncategorized(cfg, base_dir),
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
    """Find broken wiki-links [[target]] that don't have corresponding articles.

    Uses alias resolution so that [[参禅]] correctly resolves to can-chan.md
    instead of being falsely flagged as broken.
    """
    from .resolve import load_aliases, resolve_link

    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    link_pattern = re.compile(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")
    existing_slugs = {f.stem.lower() for f in concepts_dir.glob("*.md")}
    aliases = load_aliases(meta_dir)

    for md_file in concepts_dir.glob("*.md"):
        content = md_file.read_text()
        for match in link_pattern.finditer(content):
            raw_target = match.group(1).strip()
            # Try alias resolution first
            resolved = resolve_link(raw_target, aliases)
            if resolved and resolved in existing_slugs:
                continue
            # Fall back to old normalization
            simple = raw_target.lower().replace(" ", "-")
            if simple in existing_slugs:
                continue
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


def check_dirty_tags(cfg: dict) -> list[str]:
    """Find articles with malformed tags (LLM prompt leaks, sentences, etc.).

    Valid tags should be short (< 30 chars), lowercase, no sentences.
    Dirty tags look like: "2-4 tags. we need to interpret the article's content"
    """
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])

    for md_file in concepts_dir.glob("*.md"):
        post = frontmatter.load(str(md_file))
        tags = post.metadata.get("tags", [])
        slug = md_file.stem

        dirty = []
        for tag in tags:
            if not isinstance(tag, str):
                dirty.append(str(tag))
            elif len(tag) > 40:
                dirty.append(tag[:40] + "...")
            elif " " in tag and len(tag.split()) > 4:
                # More than 4 words — probably a sentence, not a tag
                dirty.append(tag)
            elif any(phrase in tag.lower() for phrase in [
                "we need", "the user", "output", "list ", "tags:",
                "tag1", "tag2", "interpret", "based on",
            ]):
                dirty.append(tag)

        if dirty:
            issues.append(f"Dirty tags in {slug}: {dirty}")

    return issues


def fix_dirty_tags(base_dir: Path | None = None) -> list[str]:
    """Clean up dirty tags by removing bad ones and regenerating via LLM."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])

    dirty_articles = check_dirty_tags(cfg)
    if not dirty_articles:
        return []

    fixes = []
    for issue in dirty_articles:
        slug = issue.split("Dirty tags in ")[1].split(":")[0]
        path = concepts_dir / f"{slug}.md"
        if not path.exists():
            continue

        post = frontmatter.load(str(path))
        old_tags = post.metadata.get("tags", [])

        # Keep only clean tags
        clean = [t for t in old_tags if isinstance(t, str) and len(t) <= 40
                 and len(t.split()) <= 4
                 and not any(p in t.lower() for p in [
                     "we need", "the user", "output", "list ", "tags:",
                     "tag1", "tag2", "interpret", "based on",
                 ])]

        if len(clean) < 2:
            # Too few clean tags remaining — ask LLM for new ones
            title = post.metadata.get("title", slug)
            prompt = f"List 2-4 relevant tags for this article (comma-separated, lowercase, short keywords only):\n\n# {title}\n\n{post.content[:2000]}"
            try:
                response = chat(prompt, max_tokens=128)
                new_tags = [t.strip().lower() for t in response.split(",") if t.strip() and len(t.strip()) <= 40]
                clean = list(set(clean + new_tags))
            except Exception:
                pass

        if set(clean) != set(old_tags):
            post.metadata["tags"] = sorted(clean)
            path.write_text(frontmatter.dumps(post), encoding="utf-8")
            fixes.append(f"Cleaned tags for {slug}: {old_tags} → {clean}")

    return fixes


def check_stubs(cfg: dict) -> list[str]:
    """Find garbage/empty stub articles that should be cleaned.

    Detects:
    - Unfilled LLM templates ("English Title / 中文标题")
    - Stubs with no real content (< 50 chars)
    - "has not been written yet" placeholder text
    - LLM prompt leak in summary ("The user says", "the user wants")
    - Title is "..." or only dots/punctuation
    - CJK-only slug (should be pinyin, e.g. "人性善" instead of "ren-xing-shan")
    """
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    cjk_re = re.compile(r'^[\u4e00-\u9fff\u3400-\u4dbf]+$')

    for md_file in concepts_dir.glob("*.md"):
        post = frontmatter.load(str(md_file))
        title = post.metadata.get("title", "")
        summary = post.metadata.get("summary", "")
        content = post.content.strip()
        slug = md_file.stem

        if "English Title / 中文标题" in title:
            issues.append(f"Unfilled template: {slug}")
        elif "One-line summary in English" in summary:
            issues.append(f"Unfilled template: {slug}")
        elif title.replace(".", "").replace("/", "").replace(" ", "") == "":
            issues.append(f"Empty/garbage title: {slug}")
        elif "has not been fully written" in content or "has not been written yet" in content:
            issues.append(f"Placeholder stub: {slug}")
        elif "尚未完成撰写" in content:
            issues.append(f"Placeholder stub: {slug}")
        elif any(leak in summary.lower() for leak in ["the user says", "the user wants", "the user asks", "the user is"]):
            issues.append(f"LLM prompt leak: {slug}")
        elif cjk_re.match(slug):
            issues.append(f"CJK slug (should be pinyin): {slug}")
        elif len(content) < 50 and not post.metadata.get("stub"):
            issues.append(f"Near-empty article: {slug} ({len(content)} chars)")

    return issues


def clean_garbage(base_dir: Path | None = None) -> list[str]:
    """Remove garbage/empty stub articles detected by check_stubs.

    Returns list of removed article slugs.
    """
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])

    stubs = check_stubs(cfg)
    if not stubs:
        return []

    removed = []
    for issue in stubs:
        # Parse "Unfilled template: slug" or "Placeholder stub: slug"
        slug = issue.split(": ", 1)[-1].split(" (")[0]
        path = concepts_dir / f"{slug}.md"
        if path.exists():
            path.unlink()
            removed.append(slug)

    if removed:
        from .compile import rebuild_index
        rebuild_index(base_dir)

    return removed


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


def check_duplicates(cfg: dict) -> list[str]:
    """Detect duplicate articles using scored heuristics.

    High-confidence pairs (score >= 3, e.g. identical CJK title) are
    confirmed without LLM. No LLM call needed — avoids thinking-token issues.
    """
    concepts_dir = Path(cfg["paths"]["concepts"])
    if not concepts_dir.exists():
        return []

    articles = []
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        articles.append({
            "slug": md_file.stem,
            "title": post.metadata.get("title", md_file.stem),
            "tags": set(t.lower() for t in post.metadata.get("tags", [])),
            "summary": post.metadata.get("summary", ""),
        })

    if len(articles) < 2:
        return []

    candidates = _find_duplicate_candidates(articles)
    issues = []
    for slug_a, slug_b in candidates:
        issues.append(f"Likely duplicate: {slug_a} <-> {slug_b}")

    return issues


def _find_duplicate_candidates(articles: list[dict]) -> list[tuple[str, str]]:
    """Pre-filter: find article pairs that are likely duplicates.

    Uses cheap heuristics — no LLM call:
    - Slug substring overlap (ASCII: min 4 chars; CJK: any length)
    - High tag overlap (>= 60% Jaccard)
    - CJK substring matching across titles AND slugs
      (仁 is substring of 仁爱 → candidate)
    """
    import re
    cjk_re = re.compile(r'[\u4e00-\u9fff\u3400-\u4dbf]')

    candidates = []
    n = len(articles)

    def _is_cjk(text: str) -> bool:
        return bool(cjk_re.search(text))

    def _extract_cjk(text: str) -> str:
        """Extract all CJK characters from text."""
        return re.sub(r'[^\u4e00-\u9fff\u3400-\u4dbf]', '', text)

    def _all_cjk_names(article: dict) -> set[str]:
        """Get all CJK names for an article: from title parts AND slug."""
        names = set()
        # From title: split by / and extract CJK
        for part in article["title"].split("/"):
            cjk = _extract_cjk(part.strip())
            if cjk:
                names.add(cjk)
        # From slug if it contains CJK
        slug_cjk = _extract_cjk(article["slug"])
        if slug_cjk:
            names.add(slug_cjk)
        return names

    def _simplify(text: str) -> str:
        """Convert traditional Chinese to simplified for comparison."""
        try:
            from opencc import OpenCC
            return OpenCC('t2s').convert(text)
        except ImportError:
            return text

    def _cjk_substring_match(names_a: set[str], names_b: set[str]) -> bool:
        """Check if CJK names match (exact, simplified/traditional, or near-exact).

        Rules:
        - Exact match (including after simplification): always match
        - Single char (仁): exact only, no substring
        - 2+ chars: substring OK if >= 67% of longer string
        """
        # Expand both sets with simplified variants
        expanded_a = names_a | {_simplify(n) for n in names_a}
        expanded_b = names_b | {_simplify(n) for n in names_b}

        for a in expanded_a:
            for b in expanded_b:
                if a == b:
                    return True
                short, long = (a, b) if len(a) <= len(b) else (b, a)
                if len(short) <= 1:
                    continue
                if short in long and len(short) / len(long) >= 0.67:
                    return True
        return False

    for i in range(n):
        for j in range(i + 1, n):
            a, b = articles[i], articles[j]
            score = 0

            # Slug substring matching
            a_slug, b_slug = a["slug"], b["slug"]
            if _is_cjk(a_slug) or _is_cjk(b_slug):
                # CJK slug: no minimum length
                if a_slug in b_slug or b_slug in a_slug:
                    score += 2
            else:
                # ASCII slug: min 4 chars to avoid "ren" in "renzhe" false positive
                if len(a_slug) >= 4 and a_slug in b_slug:
                    score += 2
                elif len(b_slug) >= 4 and b_slug in a_slug:
                    score += 2

            # Tag Jaccard similarity
            if a["tags"] and b["tags"]:
                intersection = len(a["tags"] & b["tags"])
                union = len(a["tags"] | b["tags"])
                if union > 0 and intersection / union >= 0.6:
                    score += 2

            # CJK name substring matching (the key fix)
            # Collects CJK from title parts AND slug, then does substring comparison
            cjk_a = _all_cjk_names(a)
            cjk_b = _all_cjk_names(b)
            if cjk_a and cjk_b and _cjk_substring_match(cjk_a, cjk_b):
                score += 3

            if score >= 2:
                candidates.append((a["slug"], b["slug"]))

    return candidates


def merge_duplicates(base_dir: Path | None = None, max_merges: int = 15) -> list[str]:
    """Merge confirmed duplicate articles using LLM.

    For each duplicate pair:
    1. LLM picks the primary article (better slug/title)
    2. Content from secondary is appended to primary (叠加进化)
    3. Secondary is deleted, all [[wiki-links]] pointing to it are rewritten
    4. Index and backlinks are rebuilt
    """
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])

    duplicates = check_duplicates(cfg)
    confirmed = [d for d in duplicates if d.startswith("Likely duplicate:")]

    if not confirmed:
        return []

    fixes = []
    for issue in confirmed[:max_merges]:
        # Parse "Likely duplicate: slug-a <-> slug-b"
        parts = issue.replace("Likely duplicate: ", "").split(" <-> ")
        if len(parts) != 2:
            continue
        slug_a, slug_b = parts[0].strip(), parts[1].strip()
        path_a = concepts_dir / f"{slug_a}.md"
        path_b = concepts_dir / f"{slug_b}.md"

        if not path_a.exists() or not path_b.exists():
            continue

        post_a = frontmatter.load(str(path_a))
        post_b = frontmatter.load(str(path_b))

        # Pick primary — rule-based, no LLM needed:
        # ASCII slug preferred over CJK slug; longer content wins
        import re as _re
        a_is_ascii = not bool(_re.search(r'[\u4e00-\u9fff]', slug_a))
        b_is_ascii = not bool(_re.search(r'[\u4e00-\u9fff]', slug_b))

        if a_is_ascii and not b_is_ascii:
            choose_b = False
        elif b_is_ascii and not a_is_ascii:
            choose_b = True
        else:
            choose_b = len(post_b.content) > len(post_a.content)

        if choose_b:
            primary_path, secondary_path = path_b, path_a
            primary_slug, secondary_slug = slug_b, slug_a
        else:
            primary_path, secondary_path = path_a, path_b
            primary_slug, secondary_slug = slug_a, slug_b

        primary = frontmatter.load(str(primary_path))
        secondary = frontmatter.load(str(secondary_path))

        # Merge content (叠加进化)
        if secondary.content.strip() and secondary.content.strip() not in primary.content:
            primary.content += f"\n\n---\n*Merged from [[{secondary_slug}]]:*\n\n{secondary.content}"

        # Merge tags
        old_tags = set(primary.metadata.get("tags", []))
        new_tags = set(secondary.metadata.get("tags", []))
        primary.metadata["tags"] = sorted(old_tags | new_tags)

        from datetime import datetime, timezone
        primary.metadata["updated"] = datetime.now(timezone.utc).isoformat()
        primary.metadata["merged_from"] = primary.metadata.get("merged_from", [])
        primary.metadata["merged_from"].append(secondary_slug)

        primary_path.write_text(frontmatter.dumps(primary), encoding="utf-8")

        # Rewrite all [[secondary_slug]] links to [[primary_slug]]
        _rewrite_links(concepts_dir, secondary_slug, primary_slug)

        # Delete secondary
        secondary_path.unlink()
        fixes.append(f"Merged {secondary_slug} → {primary_slug}")

    # Rebuild index + update taxonomy if any merges happened
    if fixes:
        from .compile import rebuild_index
        rebuild_index(base_dir)
        _refresh_taxonomy_after_merge(base_dir)

    return fixes


def _refresh_taxonomy_after_merge(base_dir: Path | None = None):
    """Update taxonomy.json to reflect merged articles.

    Removes deleted slugs and adds any new slugs not yet in the tree.
    Preserves the locked flag and category structure.
    """
    import json
    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    concepts_dir = Path(cfg["paths"]["concepts"])
    tax_path = meta_dir / "taxonomy.json"

    if not tax_path.exists():
        return

    taxonomy = json.loads(tax_path.read_text())
    existing_slugs = {f.stem for f in concepts_dir.glob("*.md")}

    # Collect all slugs currently in taxonomy
    def _collect_slugs(nodes):
        all_s = set()
        for n in nodes:
            all_s.update(n.get("article_slugs", []))
            all_s.update(_collect_slugs(n.get("children", [])))
        return all_s

    def _remove_dead_slugs(nodes):
        for n in nodes:
            n["article_slugs"] = [s for s in n.get("article_slugs", []) if s in existing_slugs]
            _remove_dead_slugs(n.get("children", []))

    categories = taxonomy.get("categories", [])
    _remove_dead_slugs(categories)

    # Find slugs not in any category
    assigned = _collect_slugs(categories)
    unassigned = existing_slugs - assigned
    if unassigned:
        # Add to "Other" category
        other = None
        for c in categories:
            if c.get("id") in ("other", "其他"):
                other = c
                break
        if other:
            other["article_slugs"] = list(set(other.get("article_slugs", [])) | unassigned)
        else:
            categories.append({
                "id": "other",
                "label": {"en": "Other", "zh": "其他", "ja": "その他"},
                "article_slugs": list(unassigned),
                "children": [],
            })

    taxonomy["categories"] = categories
    tax_path.write_text(json.dumps(taxonomy, indent=2, ensure_ascii=False), encoding="utf-8")


def _rewrite_links(concepts_dir: Path, old_slug: str, new_slug: str):
    """Rewrite all [[old_slug]] references to [[new_slug]] across the wiki."""
    for md_file in concepts_dir.glob("*.md"):
        content = md_file.read_text()
        # Match [[old_slug]] and [[old_slug|display text]]
        new_content = re.sub(
            rf"\[\[{re.escape(old_slug)}(\|[^\]]+)?\]\]",
            lambda m: f"[[{new_slug}{m.group(1) or ''}]]",
            content,
        )
        if new_content != content:
            md_file.write_text(new_content, encoding="utf-8")


def check_uncategorized(cfg: dict, base_dir: Path | None = None) -> list[str]:
    """Find articles that fall into 'Other' in the current taxonomy."""
    from .taxonomy import build_taxonomy
    categories = build_taxonomy(base_dir, lang="en")
    issues = []
    for cat in categories:
        if cat["id"] == "other":
            for a in cat.get("articles", []):
                issues.append(f"Uncategorized article: {a['slug']}")
    return issues


def fix_uncategorized(base_dir: Path | None = None) -> list[str]:
    """Regenerate taxonomy to re-classify uncategorized articles.

    Since taxonomy is now LLM-generated (not hardcoded), the fix is
    simply to regenerate it — the LLM will find the right categories.
    """
    uncategorized = check_uncategorized(load_config(base_dir), base_dir)
    if not uncategorized:
        return []

    from .taxonomy import generate_taxonomy
    generate_taxonomy(base_dir)
    return [f"Regenerated taxonomy to re-classify {len(uncategorized)} uncategorized article(s)"]


def fix_broken_links(base_dir: Path | None = None, max_stubs: int = 10) -> list[str]:
    """Generate stub articles for broken wiki-link targets.

    Only creates stubs for truly unresolvable links (after alias resolution).
    Strategy A: Use LLM to generate a trilingual stub from referencing context.
    Strategy B: If LLM fails, create a minimal placeholder stub.
    Returns list of fix descriptions.
    """
    from .resolve import load_aliases, resolve_link

    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    link_pattern = re.compile(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")
    existing_slugs = {f.stem.lower() for f in concepts_dir.glob("*.md")}
    aliases = load_aliases(meta_dir)

    # Collect broken links grouped by target slug
    # target_slug -> [(source_slug, context_snippet)]
    missing: dict[str, list[tuple[str, str]]] = {}

    for md_file in concepts_dir.glob("*.md"):
        content = md_file.read_text()
        for match in link_pattern.finditer(content):
            raw_target = match.group(1).strip()
            # Skip if resolvable via aliases
            resolved = resolve_link(raw_target, aliases)
            if resolved and resolved in existing_slugs:
                continue
            # Also skip if simple normalization works
            simple = raw_target.lower().replace(" ", "-")
            if simple in existing_slugs:
                continue
            target_slug = simple
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
    """Attempt to auto-fix common lint issues using LLM.

    Pipeline order:
    1. Clean garbage stubs (remove before other fixes)
    2. Fix missing metadata (summary, tags)
    3. Fix broken links (generate stubs)
    4. Merge duplicates
    5. Regenerate taxonomy for uncategorized
    """
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])
    fixes = []

    # 1. Clean garbage first
    garbage = clean_garbage(base_dir)
    if garbage:
        fixes.append(f"Cleaned {len(garbage)} garbage article(s): {', '.join(garbage[:5])}")

    # 2. Fix dirty tags (prompt leaks, sentences in tag fields)
    tag_clean = fix_dirty_tags(base_dir)
    fixes.extend(tag_clean)

    # 3. Fix missing metadata
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

    # Merge duplicate articles
    merge_fixes = merge_duplicates(base_dir)
    fixes.extend(merge_fixes)

    # Fix uncategorized articles by regenerating taxonomy
    tag_fixes = fix_uncategorized(base_dir)
    fixes.extend(tag_fixes)

    return fixes
