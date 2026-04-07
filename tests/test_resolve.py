"""Tests for alias resolution system."""

from pathlib import Path

from tools.resolve import build_aliases, resolve_link, save_aliases, load_aliases


def test_build_aliases_from_articles(tmp_kb):
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    aliases = build_aliases(concepts_dir)

    # Slug → self
    assert aliases["kong"] == "kong"
    assert aliases["si-di"] == "si-di"
    assert aliases["ren"] == "ren"

    # Title parts → slug
    assert aliases["空"] == "kong"
    assert aliases["emptiness"] == "kong"
    assert aliases["四諦"] == "si-di"
    assert aliases["仁"] == "ren"
    assert aliases["benevolence"] == "ren"


def test_resolve_link_exact(tmp_kb):
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    aliases = build_aliases(concepts_dir)

    assert resolve_link("kong", aliases) == "kong"
    assert resolve_link("空", aliases) == "kong"
    assert resolve_link("Emptiness", aliases) == "kong"  # case-insensitive
    assert resolve_link("  kong  ", aliases) == "kong"  # whitespace


def test_resolve_link_not_found(tmp_kb):
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    aliases = build_aliases(concepts_dir)

    assert resolve_link("nonexistent", aliases) is None
    assert resolve_link("", aliases) is None
    assert resolve_link(None, aliases) is None


def test_resolve_link_fuzzy(tmp_kb):
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    aliases = build_aliases(concepts_dir)

    # Fuzzy: strip punctuation
    assert resolve_link("「空」", aliases) == "kong"
    assert resolve_link("「仁」", aliases) == "ren"


def test_save_load_aliases(tmp_kb):
    meta_dir = Path(tmp_kb) / "wiki" / "_meta"
    aliases = {"空": "kong", "kong": "kong"}

    save_aliases(aliases, meta_dir)
    loaded = load_aliases(meta_dir)

    assert loaded["空"] == "kong"
    assert loaded["kong"] == "kong"


def test_resolve_with_simplified_traditional(tmp_kb):
    """Test that opencc variants are generated if available."""
    concepts_dir = Path(tmp_kb) / "wiki" / "concepts"
    aliases = build_aliases(concepts_dir)

    # 四諦 (traditional) should resolve
    assert resolve_link("四諦", aliases) == "si-di"

    # 四谛 (simplified) should also resolve if opencc is installed
    try:
        from opencc import OpenCC
        assert resolve_link("四谛", aliases) == "si-di"
    except ImportError:
        pass  # opencc not installed, skip
