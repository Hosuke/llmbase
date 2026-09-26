"""Actual native structural primitives and live Python extension boundaries."""
import pytest
from llmwiki import _native, anchor, sections, split

pytestmark = pytest.mark.skipif(not _native.available(), reason="native candidate required")


def test_default_primitives_do_not_enter_legacy_implementations(monkeypatch):
    def forbidden(*a, **kw):
        raise AssertionError("default utility entered Python implementation")
    for module, names in [(sections, ("_legacy_normalize_title", "_legacy_make_anchor", "_legacy_iter_headings", "_legacy_parse_sections", "_legacy_find_section", "_legacy_extract_section_text")),
                          (anchor, ("_legacy_normalize_text", "_legacy_normalize_with_map", "_legacy_locate_span")),
                          (split, ("_legacy_split_by_heading",))]:
        for name in names:
            monkeypatch.setattr(module, name, forbidden)
    body = "序𠮷é\r\n## 第一章\r\n甲\r\n### 子\r\n乙\r\n## 第二章\r尾"
    tree = sections.parse_sections(body)
    assert _native.last_trace()["operation"] == "utility_sections"
    child = tree[0]["children"][0]
    assert sections.find_section(tree, child["anchor"]) is child
    assert sections.extract_section_text(body, tree, child["anchor"]) == "### 子\r\n乙\r\n"
    assert sections.make_anchor(2, ["第一章"]) == tree[0]["anchor"]
    assert sections.normalize_title("（第一章）") == "第一章"
    assert list(sections._iter_headings(body))[0][1] == len("序𠮷é\r\n")
    rows = split.split_by_heading(body, 2)
    assert isinstance(rows[0], split.Section) and rows[1].content == "尾"
    match = anchor.locate_span("序𠮷，甲 é\u3000乙！尾", "𠮷甲", "乙")
    assert isinstance(match, anchor.Anchor)
    assert "序𠮷，甲 é\u3000乙！尾"[match.start:match.end] == "𠮷，甲 é\u3000乙"
    assert _native.last_trace()["offset_unit"] == "unicode_scalars"


def test_runtime_anchor_and_heading_overrides_are_called_after_import(monkeypatch):
    calls = []
    def replacement(level, titles):
        calls.append((level, titles))
        return "custom-live-anchor"
    monkeypatch.setattr(sections, "make_anchor", replacement)
    assert sections.parse_sections("## 甲\n本文")[0]["anchor"] == "custom-live-anchor"
    assert calls == [(2, ["甲"])]
    assert _native.last_trace()["core"] == "python"
    monkeypatch.setattr(sections, "make_anchor", lambda *a: "second-live-anchor")
    assert sections.parse_sections("## 甲\n本文")[0]["anchor"] == "second-live-anchor"
    monkeypatch.setattr(split, "_iter_headings", lambda body: iter([(2, 0, "live title")]))
    assert split.split_by_heading("## 原題\n正文", 2)[0].title == "live title"
    monkeypatch.setattr(anchor, "normalize_text", lambda value, level: "甲" if value == "needle" else value)
    assert anchor.locate_span("甲乙", "needle").start == 0
    assert _native.last_trace()["core"] == "python"


def test_mutable_anchor_constants_and_native_map_contract(monkeypatch):
    monkeypatch.setattr(sections, "SLUG_MAX_CHARS", 1)
    monkeypatch.setattr(sections, "HASH_LEN", 2)
    tree = sections.parse_sections("## 緒論\n內容")
    assert tree[0]["anchor"].startswith("h2-緒-")
    assert len(tree[0]["anchor"].split("-")[-1]) == 2
    normalized, mapping = anchor._normalize_with_map("𠮷， a\x1fb", "punct_spaces")
    assert normalized == "𠮷ab" and mapping == [0, 3, 5]


def test_atomic_writes_use_native_io_and_exact_library_registration(tmp_path, monkeypatch):
    import json
    import math
    from llmwiki import atomic, config
    def forbidden(*args, **kwargs):
        raise AssertionError("Python tempfile writer entered")
    monkeypatch.setattr(atomic.tempfile, "mkstemp", forbidden)
    standalone = tmp_path / "independent/value.json"
    atomic.atomic_write_json(standalone, {"large": 2**100, "nan": float("nan"), "text": "𠮷"})
    assert _native.last_trace()["coordination"] == "standalone_file_lock"
    value = json.loads(standalone.read_text())
    assert value["large"] == 2**100 and math.isnan(value["nan"])
    cfg = config.load_config(tmp_path)
    scoped = tmp_path / "wiki/_meta/custom.json"
    atomic.atomic_write_text(scoped, "source𠮷\r\n")
    assert _native.last_trace()["coordination"] == "library_writer_and_file_lock"
    assert scoped.read_bytes() == "source𠮷\r\n".encode()
    _native.with_writer(tmp_path, lambda: atomic.atomic_write_text(scoped, "nested"), paths=cfg["paths"])
    assert scoped.read_text() == "nested"
    other = tmp_path / "other"
    other.mkdir()
    cfg2 = {"base_dir": str(other), "paths": dict(cfg["paths"])}
    _native.register_config(cfg2, other)
    with pytest.raises(_native.NativeCapabilityError, match="multiple configured libraries"):
        atomic.atomic_write_text(scoped, "ambiguous")
    _native.with_writer(tmp_path, lambda: atomic.atomic_write_text(scoped, "explicit"), paths=cfg["paths"])
    assert scoped.read_text() == "explicit"


def test_standalone_compile_helpers_do_not_register_inferred_library(tmp_path):
    from pathlib import Path
    import llmwiki.compile as compiler
    from llmwiki import atomic, config
    prior = set(_native._configured_roots)
    cfg = config.load_config(tmp_path)
    config.ensure_dirs(cfg)
    registered = set(_native._configured_roots) - prior
    assert registered == {(str(tmp_path.resolve()), "default")}
    concepts = Path(cfg["paths"]["concepts"])
    raw = Path(cfg["paths"]["raw"])
    meta = Path(cfg["paths"]["meta"])
    # Read helpers with standalone directory inputs share _native_compile's
    # transient context; none may publish an inferred library identity.
    assert compiler._list_existing_concepts(concepts) == []
    assert compiler._find_uncompiled(raw) == []
    assert compiler._load_index(meta) == []
    assert set(_native._configured_roots) - prior == registered
    phantom = str((tmp_path / "wiki").resolve())
    assert all(base != phantom for base, _library in _native._configured_roots)
    # Explicit cfg and the active compile context are preserved unchanged.
    assert compiler._native_compile("existing", cfg=cfg, concepts=concepts) == []
    token = compiler._active_compile_config.set(cfg)
    try:
        assert compiler._list_existing_concepts(concepts) == []
    finally:
        compiler._active_compile_config.reset(token)
    assert set(_native._configured_roots) - prior == registered
    # The reported sequence: an ordinary metadata write after a read helper.
    probe = meta / "probe.txt"
    atomic.atomic_write_text(probe, "exact original bytes")
    assert probe.read_bytes() == b"exact original bytes"


def test_inferred_helper_context_is_transient_for_custom_roots(tmp_path):
    import llmwiki.compile as compiler
    prior = set(_native._configured_roots)
    raw = tmp_path / "originals"
    concepts = tmp_path / "knowledge" / "entries"
    meta = tmp_path / "knowledge" / "_meta"
    for directory in (raw, concepts, meta):
        directory.mkdir(parents=True)
    assert compiler._find_uncompiled(raw) == []
    assert compiler._list_existing_concepts(concepts) == []
    assert compiler._load_index(meta) == []
    assert set(_native._configured_roots) == prior
