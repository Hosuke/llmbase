"""Lint checks — detect issues in the knowledge base."""

import inspect
import json
import re
import sys
from pathlib import Path

import frontmatter

from ..config import load_config, ensure_dirs
from ..llm import chat


# ─── Customizable constants ──────────────────────────────────────
# Downstream can override to adjust lint behaviour.
#
#     import llmwiki.lint.checks as checks
#     checks.ALLOW_CJK_SLUGS = True   # don't flag CJK slugs as issues
#

# When True, CJK-character slugs (e.g. "仁" instead of "ren") are accepted
# as valid and will NOT be reported by check_stubs.  Default False preserves
# the original pinyin-slug convention.
ALLOW_CJK_SLUGS: bool = False


SYSTEM_PROMPT = """You are a knowledge base quality analyst. Your job is to review wiki articles
and identify issues, inconsistencies, and opportunities for improvement.

Be specific and actionable in your findings. Reference article titles and specific content."""



def _load_articles(concepts_dir: Path) -> list[dict]:
    """Parse every article's frontmatter + content exactly once.

    Returned dicts are shared by all lint checks, so each file is read and
    parsed a single time per lint() pass instead of once per check. The native
    branch runs the same parse in Rust; each article ``path`` is restored to a
    Path value at this Python boundary. The legacy branch remains only for
    unbuilt source checkouts under the ``_native.available()`` policy.
    """
    from .. import _native
    if _native.available():
        loaded = _native.maintenance_transform("load", {"concepts_dir": str(concepts_dir)})
        for article in loaded:
            article["path"] = Path(article["path"])
        return loaded
    articles = []
    if not concepts_dir.exists():
        return articles
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        articles.append({
            "path": md_file,
            "slug": md_file.stem,
            "title": post.metadata.get("title", md_file.stem),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []) or [],
            "content": post.content,
            "metadata": post.metadata,
        })
    return articles


def lint(base_dir: Path | None = None) -> dict:
    """Run all lint checks on the wiki.

    With the native extension, Rust owns the snapshot sequencing, the eight
    checks, the total and the emit stage (maintenance_run). Module-level check
    callables are resolved live at every Rust callback stage: defaults return
    null so Rust executes the check, while a genuinely replaced callable runs
    once as an explicit Python callback with only the arguments its signature
    accepts.
    """
    from .. import _native

    cfg = load_config(base_dir)
    ensure_dirs(cfg)

    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    if not _native.available():
        # Unbuilt source checkout only; installed candidates fail closed above.
        from ..resolve import load_aliases

        articles = _load_articles(concepts_dir)
        existing_slugs = {a["slug"].lower() for a in articles}
        aliases = load_aliases(meta_dir)

        results = {
            "structural": check_structural(cfg, articles=articles),
            "broken_links": check_broken_links(cfg, articles=articles, existing_slugs=existing_slugs, aliases=aliases),
            "orphans": check_orphans(cfg, articles=articles),
            "missing_metadata": check_missing_metadata(cfg, articles=articles),
            "dirty_tags": check_dirty_tags(cfg, articles=articles),
            "duplicates": check_duplicates(cfg, articles=articles),
            "stubs": check_stubs(cfg, articles=articles),
            "uncategorized": check_uncategorized(cfg, base_dir),
        }

        # Count total issues
        total = sum(len(v) for v in results.values())
        results["total_issues"] = total

        from ..hooks import emit
        emit("after_lint_check", total_issues=total, results=results)

        return results

    from .. import hooks

    articles = _load_articles(concepts_dir)
    existing_slugs = {a["slug"].lower() for a in articles}
    # JSON DTO: only the article path field is converted explicitly; arbitrary
    # metadata is never stringified, so unsupported value types fail loudly.
    snapshot = []
    for article in articles:
        row = dict(article)
        if "path" in row:
            row["path"] = str(row["path"])
        snapshot.append(row)
    prepared = _native.maintenance_transform("prepare", {
        "concepts_dir": str(concepts_dir),
        "meta_dir": str(meta_dir),
        "articles": snapshot,
        "allow_cjk_slugs": ALLOW_CJK_SLUGS,
    })
    aliases = dict(prepared["aliases"])

    def host(stage: str, payload: dict):
        if stage == "emit":
            # The single actual hook dispatch with the original event/kwargs.
            hooks.emit(payload["event"], total_issues=payload["total_issues"],
                       results=payload["results"])
            return None
        live = getattr(sys.modules[__name__], "check_" + stage)
        if live is _DEFAULT_CHECKS[stage]:
            if stage != "uncategorized":
                return None  # Null: Rust executes the default check.
            # The taxonomy boundary stays lazy and explicit: categories are
            # built only when no replacement handles this stage, then Rust
            # executes the uncategorized check on the prepared snapshot.
            from .. import taxonomy
            categories = taxonomy.build_taxonomy(base_dir, lang="en")
            _native.python_boundary("maintenance_run", "taxonomy.build_taxonomy")
            value = _native.maintenance_transform(
                "check", dict(payload, name="uncategorized", categories=categories))
            return {"override": True, "value": value}
        candidates = {"cfg": cfg, "base_dir": base_dir}
        if stage != "uncategorized":
            # One shared Path-valued article snapshot for the Python callbacks.
            candidates["articles"] = articles
        if stage == "broken_links":
            candidates["existing_slugs"] = existing_slugs
            candidates["aliases"] = aliases
        result = _invoke_check(live, candidates)
        _native.python_boundary("maintenance_run", "check_" + stage)
        return {"override": True, "value": result}

    return _native.run_maintenance("lint", prepared, host)


def _invoke_check(check, candidates: dict):
    """Invoke a replaced check with exactly the arguments its signature accepts.

    Arguments are bound before invocation: only supported optional kwargs are
    retained unless the callable accepts **kwargs, required parameters must be
    satisfiable, and positional-only parameters stay positional. A binding
    failure raises before the body runs; a TypeError raised inside the body
    propagates unchanged and is never retried with fewer arguments. If
    introspection is unavailable, the historical full keyword shape is invoked
    exactly once and its error propagates.
    """
    try:
        signature = inspect.signature(check)
    except (TypeError, ValueError):
        return check(**candidates)
    parameters = list(signature.parameters.values())
    if any(parameter.kind is inspect.Parameter.VAR_KEYWORD for parameter in parameters):
        return check(**candidates)
    positional = []
    keyword = {}
    for parameter in parameters:
        if parameter.name not in candidates:
            continue
        if parameter.kind is inspect.Parameter.POSITIONAL_ONLY:
            positional.append(candidates[parameter.name])
        elif parameter.kind is not inspect.Parameter.VAR_POSITIONAL:
            keyword[parameter.name] = candidates[parameter.name]
    bound = signature.bind(*positional, **keyword)
    return check(*bound.args, **bound.kwargs)



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



def check_structural(cfg: dict, articles: list[dict] | None = None) -> list[str]:
    """Check for structural issues."""
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    if not (meta_dir / "_index.md").exists():
        issues.append("Missing master index file (_index.md)")

    if not (meta_dir / "index.json").exists():
        issues.append("Missing JSON index (index.json)")

    if articles is None:
        article_count = len(list(concepts_dir.glob("*.md")))
    else:
        article_count = len(articles)
    if article_count == 0:
        issues.append("No articles in the wiki")

    return issues



def check_broken_links(
    cfg: dict,
    articles: list[dict] | None = None,
    existing_slugs: set[str] | None = None,
    aliases: dict | None = None,
) -> list[str]:
    """Find broken wiki-links [[target]] that don't have corresponding articles.

    Uses alias resolution so that [[参禅]] correctly resolves to can-chan.md
    instead of being falsely flagged as broken.
    """
    from ..resolve import load_aliases, resolve_link

    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    link_pattern = re.compile(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")

    if articles is None:
        articles = _load_articles(concepts_dir)
    if existing_slugs is None:
        existing_slugs = {a["slug"].lower() for a in articles}
    if aliases is None:
        aliases = load_aliases(meta_dir)

    for art in articles:
        content = art["content"]
        slug = art["slug"]
        for match in link_pattern.finditer(content):
            raw_target = match.group(1).strip()
            resolved = resolve_link(raw_target, aliases)
            if resolved and resolved in existing_slugs:
                continue
            simple = raw_target.lower().replace(" ", "-")
            if simple in existing_slugs:
                continue
            issues.append(f"Broken link in {slug}: [[{match.group(1)}]]")

    return issues



def check_orphans(cfg: dict, articles: list[dict] | None = None) -> list[str]:
    """Find articles that are not linked to from any other article."""
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    backlinks_path = meta_dir / "backlinks.json"
    if not backlinks_path.exists():
        return ["Backlinks map not built yet"]

    backlinks = json.loads(backlinks_path.read_text())
    linked_slugs = set(backlinks.keys())

    if articles is None:
        articles = _load_articles(concepts_dir)

    for art in articles:
        slug = art["slug"]
        if slug.lower() not in linked_slugs:
            issues.append(f"Orphan article (no incoming links): {slug}")

    return issues



def check_missing_metadata(cfg: dict, articles: list[dict] | None = None) -> list[str]:
    """Find articles with missing or incomplete metadata."""
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])

    if articles is None:
        articles = _load_articles(concepts_dir)

    for art in articles:
        slug = art["slug"]
        meta = art["metadata"]
        if not meta.get("title"):
            issues.append(f"Missing title: {slug}")
        if not meta.get("summary"):
            issues.append(f"Missing summary: {slug}")
        if not meta.get("tags"):
            issues.append(f"Missing tags: {slug}")

    return issues



def check_dirty_tags(cfg: dict, articles: list[dict] | None = None) -> list[str]:
    """Find articles with malformed tags (LLM prompt leaks, sentences, etc.).

    Valid tags should be short (< 30 chars), lowercase, no sentences.
    Dirty tags look like: "2-4 tags. we need to interpret the article's content"
    """
    issues = []
    concepts_dir = Path(cfg["paths"]["concepts"])

    if articles is None:
        articles = _load_articles(concepts_dir)

    for art in articles:
        tags = art["tags"]
        slug = art["slug"]

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



def check_stubs(cfg: dict, articles: list[dict] | None = None) -> list[str]:
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

    if articles is None:
        articles = _load_articles(concepts_dir)

    for art in articles:
        title = art["title"] if isinstance(art["title"], str) else ""
        summary = art["summary"] if isinstance(art["summary"], str) else ""
        content = art["content"].strip()
        slug = art["slug"]
        meta = art["metadata"]

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
        elif not ALLOW_CJK_SLUGS and cjk_re.match(slug):
            issues.append(f"CJK slug (should be pinyin): {slug}")
        elif len(content) < 50 and not meta.get("stub"):
            issues.append(f"Near-empty article: {slug} ({len(content)} chars)")

    return issues



def check_uncategorized(cfg: dict, base_dir: Path | None = None) -> list[str]:
    """Find articles that fall into 'Other' in the current taxonomy."""
    from ..taxonomy import build_taxonomy
    categories = build_taxonomy(base_dir, lang="en")
    issues = []
    for cat in categories:
        if cat["id"] == "other":
            for a in cat.get("articles", []):
                issues.append(f"Uncategorized article: {a['slug']}")
    return issues



def check_duplicates(cfg: dict, articles: list[dict] | None = None) -> list[str]:
    """Detect duplicate articles using scored heuristics.

    High-confidence pairs (score >= 3, e.g. identical CJK title) are
    confirmed without LLM. No LLM call needed — avoids thinking-token issues.
    """
    concepts_dir = Path(cfg["paths"]["concepts"])
    if not concepts_dir.exists():
        return []

    if articles is None:
        articles = _load_articles(concepts_dir)

    dup_articles = [
        {
            "slug": a["slug"],
            "title": a["title"] if isinstance(a["title"], str) else a["slug"],
            "tags": set(str(t).lower() for t in a["tags"]),
            "summary": a["summary"] if isinstance(a["summary"], str) else "",
        }
        for a in articles
    ]

    if len(dup_articles) < 2:
        return []

    from .dedup import _find_duplicate_candidates
    candidates = _find_duplicate_candidates(dup_articles)
    issues = []
    for slug_a, slug_b in candidates:
        issues.append(f"Likely duplicate: {slug_a} <-> {slug_b}")

    return issues


# Default check identities, captured once after the definitions above and
# before any downstream patching. lint() resolves the live module attribute at
# every Rust callback stage; only a genuinely replaced callable is invoked as
# a Python callback, everything else executes as a native Rust check.
_DEFAULT_CHECKS = {
    "structural": check_structural,
    "broken_links": check_broken_links,
    "orphans": check_orphans,
    "missing_metadata": check_missing_metadata,
    "dirty_tags": check_dirty_tags,
    "duplicates": check_duplicates,
    "stubs": check_stubs,
    "uncategorized": check_uncategorized,
}


