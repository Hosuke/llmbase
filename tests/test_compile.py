"""Tests for compilation pipeline."""

import json
from pathlib import Path

import frontmatter

from tools.compile import rebuild_index, _merge_into, _split_sections, _assemble_sections


def test_rebuild_index(tmp_kb):
    entries = rebuild_index(tmp_kb)

    assert len(entries) == 3

    slugs = {e["slug"] for e in entries}
    assert "kong" in slugs
    assert "si-di" in slugs
    assert "ren" in slugs

    # Check index.json written
    meta_dir = Path(tmp_kb) / "wiki" / "_meta"
    index = json.loads((meta_dir / "index.json").read_text())
    assert len(index) == 3

    # Check aliases.json written
    aliases = json.loads((meta_dir / "aliases.json").read_text())
    assert aliases["空"] == "kong"

    # Check backlinks.json written
    backlinks = json.loads((meta_dir / "backlinks.json").read_text())
    assert "kong" in backlinks  # si-di links to kong via [[空]]


def test_rebuild_creates_backlinks(tmp_kb):
    rebuild_index(tmp_kb)
    meta_dir = Path(tmp_kb) / "wiki" / "_meta"
    backlinks = json.loads((meta_dir / "backlinks.json").read_text())

    # si-di references kong via [[kong|Emptiness]] and [[空]]
    assert "kong" in backlinks
    assert "si-di" in backlinks["kong"]


def test_split_sections():
    content = """Some preamble text.

## English

English content here.

## 中文

中文内容在这里。

## 日本語

日本語の内容。"""

    sections = _split_sections(content)
    assert "english" in sections
    assert "中文" in sections
    assert "日本語" in sections
    assert "English content here." in sections["english"]
    assert "中文内容在这里。" in sections["中文"]


def test_assemble_sections():
    sections = {
        "_preamble": "",
        "english": "Hello world",
        "中文": "你好世界",
        "日本語": "こんにちは世界",
    }
    result = _assemble_sections(sections)
    assert "## English" in result
    assert "## 中文" in result
    assert "## 日本語" in result
    assert "Hello world" in result


def test_merge_into_adds_content(tmp_kb):
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    article_path = concepts_dir / "kong.md"

    article = {
        "content": "## English\n\nNew insight about emptiness.\n\n## 中文\n\n关于空性的新见解。非常长的新内容，比原来的更详细更丰富，超过了原来的1.2倍。",
        "tags": ["mahayana"],
    }

    _merge_into(article_path, article)

    post = frontmatter.load(str(article_path))
    assert "mahayana" in post.metadata["tags"]


def test_merge_into_no_duplicate_content(tmp_kb):
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    article_path = concepts_dir / "kong.md"
    original = article_path.read_text()

    # Merge identical content → should not change
    article = {"content": "", "tags": []}
    _merge_into(article_path, article)

    assert article_path.read_text() == original
