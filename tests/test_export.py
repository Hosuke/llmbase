"""Tests for structured export API."""

from pathlib import Path

from tools.compile import rebuild_index
from tools.export import export_article, export_by_tag, export_graph


def test_export_article(tmp_kb):
    rebuild_index(tmp_kb)
    result = export_article("kong", tmp_kb)

    assert result is not None
    assert result["slug"] == "kong"
    assert result["title"] == "Emptiness / 空"
    assert "buddhism" in result["tags"]
    assert "english" in result["content"]
    assert "zh" in result["content"]
    assert isinstance(result["backlinks"], list)
    assert isinstance(result["outgoing_links"], list)
    assert isinstance(result["sources"], list)


def test_export_article_alias(tmp_kb):
    """Export by Chinese name should resolve via alias."""
    rebuild_index(tmp_kb)
    result = export_article("空", tmp_kb)

    assert result is not None
    assert result["slug"] == "kong"


def test_export_article_not_found(tmp_kb):
    result = export_article("nonexistent", tmp_kb)
    assert result is None


def test_export_article_content_split(tmp_kb):
    rebuild_index(tmp_kb)
    result = export_article("kong", tmp_kb)

    assert "Emptiness" in result["content"]["english"]
    assert "空" in result["content"]["zh"]


def test_export_article_backlinks(tmp_kb):
    rebuild_index(tmp_kb)
    result = export_article("kong", tmp_kb)

    # si-di links to kong → backlink
    bl_slugs = [bl["slug"] for bl in result["backlinks"]]
    assert "si-di" in bl_slugs


def test_export_article_outgoing_links(tmp_kb):
    rebuild_index(tmp_kb)
    result = export_article("si-di", tmp_kb)

    out_slugs = [o["slug"] for o in result["outgoing_links"]]
    assert "kong" in out_slugs


def test_export_article_related_by_tags(tmp_kb):
    rebuild_index(tmp_kb)
    result = export_article("kong", tmp_kb)

    # kong and si-di share "buddhism" tag
    related_slugs = [r["slug"] for r in result["related_by_tags"]]
    assert "si-di" in related_slugs


def test_export_by_tag(tmp_kb):
    result = export_by_tag("buddhism", tmp_kb)

    assert result["tag"] == "buddhism"
    assert result["count"] == 2
    slugs = [a["slug"] for a in result["articles"]]
    assert "kong" in slugs
    assert "si-di" in slugs


def test_export_by_tag_empty(tmp_kb):
    result = export_by_tag("nonexistent-tag", tmp_kb)
    assert result["count"] == 0


def test_export_graph(tmp_kb):
    rebuild_index(tmp_kb)
    result = export_graph("kong", depth=1, base_dir=tmp_kb)

    assert result["root"] == "kong"
    node_slugs = [n["slug"] for n in result["nodes"]]
    assert "kong" in node_slugs
    # si-di links to kong → should be in depth-1 graph
    assert "si-di" in node_slugs


def test_export_graph_depth_0(tmp_kb):
    rebuild_index(tmp_kb)
    result = export_graph("kong", depth=0, base_dir=tmp_kb)

    assert result["count"] == 1
    assert result["nodes"][0]["slug"] == "kong"
