"""Tests for lint checks and auto-fix pipeline."""

import json
from pathlib import Path

import frontmatter

from tools.compile import rebuild_index
from tools.lint import (
    check_broken_links,
    check_orphans,
    check_missing_metadata,
    check_stubs,
    check_dirty_tags,
    check_duplicates,
    _find_duplicate_candidates,
    clean_garbage,
)
from tools.config import load_config


def test_check_broken_links_with_aliases(tmp_kb):
    """Links resolvable via aliases should NOT be flagged."""
    rebuild_index(tmp_kb)  # generates aliases.json
    cfg = load_config(tmp_kb)
    issues = check_broken_links(cfg)

    # [[空]] in si-di → resolves to kong via alias → NOT broken
    broken_texts = " ".join(issues)
    assert "[[空]]" not in broken_texts

    # [[sunyata]] and [[Nagarjuna]] have no articles → truly broken
    assert any("sunyata" in i for i in issues)
    assert any("Nagarjuna" in i or "nagarjuna" in i.lower() for i in issues)


def test_check_orphans(tmp_kb):
    rebuild_index(tmp_kb)
    cfg = load_config(tmp_kb)
    issues = check_orphans(cfg)

    # ren has no incoming links → orphan
    assert any("ren" in i for i in issues)


def test_check_missing_metadata(tmp_kb):
    cfg = load_config(tmp_kb)

    # All articles have metadata → no issues
    issues = check_missing_metadata(cfg)
    assert len(issues) == 0

    # Remove summary from one article
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    post = frontmatter.load(str(concepts_dir / "ren.md"))
    post.metadata["summary"] = ""
    (concepts_dir / "ren.md").write_text(frontmatter.dumps(post))

    issues = check_missing_metadata(cfg)
    assert any("ren" in i for i in issues)


def test_check_stubs_detects_template(tmp_kb):
    """Unfilled LLM templates should be detected."""
    cfg = load_config(tmp_kb)
    concepts_dir = Path(cfg["paths"]["concepts"])

    # Create a garbage stub
    post = frontmatter.Post("Short")
    post.metadata["title"] = "English Title / 中文标题"  # template
    post.metadata["summary"] = "test"
    post.metadata["tags"] = ["stub"]
    (concepts_dir / "garbage.md").write_text(frontmatter.dumps(post))

    issues = check_stubs(cfg)
    assert any("garbage" in i for i in issues)


def test_check_stubs_detects_prompt_leak(tmp_kb):
    cfg = load_config(tmp_kb)
    concepts_dir = Path(cfg["paths"]["concepts"])

    post = frontmatter.Post("Some content here that is long enough")
    post.metadata["title"] = "Test Article"
    post.metadata["summary"] = "The user says we need to write about this"
    post.metadata["tags"] = ["test"]
    (concepts_dir / "leak.md").write_text(frontmatter.dumps(post))

    issues = check_stubs(cfg)
    assert any("leak" in i for i in issues)


def test_check_dirty_tags(tmp_kb):
    cfg = load_config(tmp_kb)
    concepts_dir = Path(cfg["paths"]["concepts"])

    post = frontmatter.load(str(concepts_dir / "ren.md"))
    post.metadata["tags"] = ["confucianism", "2-4 tags. we need to interpret the content"]
    (concepts_dir / "ren.md").write_text(frontmatter.dumps(post))

    issues = check_dirty_tags(cfg)
    assert any("ren" in i for i in issues)


def test_clean_garbage(tmp_kb):
    cfg = load_config(tmp_kb)
    concepts_dir = Path(cfg["paths"]["concepts"])

    post = frontmatter.Post("x")
    post.metadata["title"] = "English Title / 中文标题"
    post.metadata["tags"] = []
    (concepts_dir / "junk.md").write_text(frontmatter.dumps(post))

    removed = clean_garbage(tmp_kb)
    assert "junk" in removed
    assert not (concepts_dir / "junk.md").exists()


def test_find_duplicate_candidates_cjk():
    """CJK title matching should detect duplicates."""
    articles = [
        {"slug": "benevolence", "title": "Benevolence / 仁",
         "tags": {"ethics", "confucianism"}, "summary": ""},
        {"slug": "ren", "title": "Ren / 仁",
         "tags": {"ethics", "confucianism"}, "summary": ""},
    ]

    candidates = _find_duplicate_candidates(articles)
    assert len(candidates) >= 1
    slugs = {s for pair in candidates for s in pair}
    assert "benevolence" in slugs
    assert "ren" in slugs


def test_find_duplicate_candidates_no_false_positive():
    """Different concepts should NOT be flagged."""
    articles = [
        {"slug": "kong", "title": "Emptiness / 空",
         "tags": {"buddhism"}, "summary": ""},
        {"slug": "ren", "title": "Benevolence / 仁",
         "tags": {"confucianism"}, "summary": ""},
    ]

    candidates = _find_duplicate_candidates(articles)
    assert len(candidates) == 0
