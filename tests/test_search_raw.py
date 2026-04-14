"""Tests for raw/ fallback search."""

import os
from pathlib import Path

import frontmatter

from tools.search import search_raw


def _write_raw(raw_dir: Path, subdir: str, title: str, content: str, source: str = ""):
    d = raw_dir / subdir
    d.mkdir(parents=True, exist_ok=True)
    post = frontmatter.Post(content)
    post.metadata["title"] = title
    if source:
        post.metadata["source"] = source
    (d / "index.md").write_text(frontmatter.dumps(post), encoding="utf-8")


def test_search_raw_finds_verbatim(tmp_kb, monkeypatch):
    monkeypatch.chdir(tmp_kb)
    raw_dir = Path(tmp_kb) / "raw"
    _write_raw(raw_dir, "foxue-core", "佛学核心概念",
               "## 八正道\n正见、正思维、正语、正业、正命、正精进、正念、正定。",
               source="https://example.org/foxue")

    results = search_raw("八正道", top_k=5, base_dir=tmp_kb)
    assert results, "expected at least one hit for 八正道"
    top = results[0]
    assert top["source"] == "foxue-core"
    assert top["source_url"] == "https://example.org/foxue"
    assert "八正道" in top["snippet"]
    assert top["score"] > 0


def test_search_raw_empty_when_no_raw(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    (tmp_path / "config.yaml").write_text(
        f"paths:\n  raw: {tmp_path}/raw\n  wiki: {tmp_path}/wiki\n"
        f"  outputs: {tmp_path}/wiki/outputs\n  meta: {tmp_path}/wiki/_meta\n"
        f"  concepts: {tmp_path}/wiki/concepts\n"
    )
    assert search_raw("anything", base_dir=tmp_path) == []


def test_search_raw_scrubs_local_paths(tmp_kb):
    """Local-file ingest stores absolute paths in `source`; never leak them."""
    raw_dir = Path(tmp_kb) / "raw"
    _write_raw(raw_dir, "local-doc", "Local Doc",
               "## 主题\n主题 主题 主题 关键词。",
               source="/Users/someone/private/notes.md")

    results = search_raw("主题", top_k=5, base_dir=tmp_kb)
    assert results
    for r in results:
        assert r["source_url"] == "", f"leaked local path: {r['source_url']}"


def test_search_raw_top_k_edges(tmp_kb):
    raw_dir = Path(tmp_kb) / "raw"
    for i in range(3):
        _write_raw(raw_dir, f"doc-{i}", f"Doc {i}", "## 涅槃\n涅槃 涅槃。")

    # Negative top_k must not trigger Python slice semantics (results[:-1]).
    assert search_raw("涅槃", top_k=-1, base_dir=tmp_kb) == []
    # top_k=0 honours caller intent (empty list, not forced to 1).
    assert search_raw("涅槃", top_k=0, base_dir=tmp_kb) == []


def test_search_raw_no_absolute_paths_in_results(tmp_kb):
    raw_dir = Path(tmp_kb) / "raw"
    _write_raw(raw_dir, "doc", "Doc", "## 主題\n主題 主題 主題。")
    results = search_raw("主題", top_k=5, base_dir=tmp_kb)
    assert results
    for r in results:
        # rel_path stays relative; absolute filesystem path must not be exposed.
        assert "path" not in r
        assert not r["rel_path"].startswith("/")


def test_search_raw_ranks_by_tf_idf(tmp_kb):
    raw_dir = Path(tmp_kb) / "raw"
    _write_raw(raw_dir, "a", "A", "## 涅槃\n涅槃 涅槃 涅槃 寂静。")
    _write_raw(raw_dir, "b", "B", "## 杂\n偶尔提及涅槃一次。")

    results = search_raw("涅槃", top_k=5, base_dir=tmp_kb)
    sources = [r["source"] for r in results]
    assert sources.index("a") < sources.index("b"), "higher tf document should rank first"
