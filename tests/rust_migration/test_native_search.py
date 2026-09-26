"""Run against an installed candidate, outside the checkout, with --import-mode=importlib.

Source-only runs skip these tests; LLMBASE_REQUIRE_NATIVE=1 turns absence into a failure.
They trap the Python search/file-reading implementation to prove real native delegation.
"""

import importlib
import json
from pathlib import Path

import pytest

from llmwiki import _native

search_module = importlib.import_module("llmwiki.search")


@pytest.fixture(autouse=True)
def native_candidate(monkeypatch):
    if not _native.available():
        pytest.skip("native candidate extension is not built")
    monkeypatch.setattr(search_module, "SEARCH_TOKENIZER", None)
    monkeypatch.setattr(search_module, "STOPWORDS", set(search_module.STOPWORDS))
    monkeypatch.setattr(search_module, "CJK_STOPWORDS", set(search_module.CJK_STOPWORDS))


@pytest.fixture
def library(tmp_path):
    concepts = tmp_path / "wiki/concepts"
    concepts.mkdir(parents=True)
    (tmp_path / "wiki/outputs").mkdir()
    raw = tmp_path / "raw/book"
    raw.mkdir(parents=True)
    (concepts / "alpha.md").write_text("---\ntitle: Alpha\nsummary: first\n---\nalpha alpha 學而時習之 𠮷野\n")
    (concepts / "beta.md").write_text("---\ntitle: Beta\n---\nalpha café Go语言\n")
    (raw / "index.md").write_text("---\ntitle: 原典\nsource: /private/example/book\n---\n學而時習之 𠮷野 alpha\n")
    return tmp_path


def reject_python_core(*args, **kwargs):
    raise AssertionError("Python legacy core or frontmatter loader was called")


def test_actual_native_dispatch_reads_and_ranks_without_python_core(library, monkeypatch):
    before = _native.backend_info()["successful_dispatches"]
    monkeypatch.setattr(search_module, "_legacy_search", reject_python_core)
    monkeypatch.setattr(search_module, "_legacy_search_raw", reject_python_core)
    monkeypatch.setattr(search_module.frontmatter, "load", reject_python_core)
    monkeypatch.setattr(search_module, "_tokenize", reject_python_core)
    compiled = search_module.search("alpha", base_dir=library)
    raw = search_module.search_raw("𠮷野", base_dir=library)
    assert compiled[0]["slug"] == "alpha"
    assert compiled[0]["score"] == 2.386
    assert raw[0]["source_url"] == ""
    assert "path" not in raw[0]
    assert _native.backend_info()["successful_dispatches"] == before + 2
    trace = _native.last_trace()
    assert trace["core"] == "rust"
    assert trace["tokenizer"] == "rust_legacy"
    assert trace["storage"] == "native_filesystem_snapshot"
    assert trace["indexed"] is False
    assert not (library / "wiki/_meta").exists()


@pytest.mark.parametrize("query", ["alpha", "學而時習之", "𠮷野", "Go语言", "café", "alpha alpha", "the", ""])
def test_result_values_match_legacy_dialect(library, query):
    assert search_module.search(query, base_dir=library) == search_module._legacy_search(query, base_dir=library)
    assert search_module.search_raw(query, base_dir=library) == search_module._legacy_search_raw(query, base_dir=library)


def test_callback_overrides_change_midprocess_and_reach_native_rank(library, monkeypatch):
    calls = []

    def callback_for(target):
        def tokenize(text):
            calls.append(text)
            return ["synthetic"] if text == "unmatched-query" or target in text else []
        return tokenize

    monkeypatch.setattr(search_module, "_legacy_search", reject_python_core)
    monkeypatch.setattr(search_module.frontmatter, "load", reject_python_core)
    monkeypatch.setattr(search_module, "SEARCH_TOKENIZER", callback_for("Beta"))
    assert [hit["slug"] for hit in search_module.search("unmatched-query", base_dir=library)] == ["beta"]
    assert len(calls) == 3  # Query and both core-read documents actually entered Python callback.
    assert _native.last_trace()["tokenizer"] == "python_callback"
    monkeypatch.setattr(search_module, "SEARCH_TOKENIZER", callback_for("Alpha"))
    assert [hit["slug"] for hit in search_module.search("unmatched-query", base_dir=library)] == ["alpha"]
    assert len(calls) == 6
    monkeypatch.setattr(search_module, "SEARCH_TOKENIZER", lambda text: (_ for _ in ()).throw(LookupError("callback failure")))
    with pytest.raises(LookupError, match="callback failure"):
        search_module.search("unmatched-query", base_dir=library)


def test_stopwords_and_external_source_edits_are_read_each_call(library):
    assert search_module.search("alpha", base_dir=library)
    search_module.STOPWORDS.add("alpha")
    assert search_module.search("alpha", base_dir=library) == []
    search_module.STOPWORDS.remove("alpha")
    (library / "wiki/concepts/new.md").write_text("newlyadded marker")
    assert search_module.search("newlyadded", base_dir=library)[0]["slug"] == "new"
    search_module.CJK_STOPWORDS.add("學")
    assert search_module.search("學", base_dir=library) == []
    search_module.CJK_STOPWORDS.remove("學")
    assert search_module.search("學", base_dir=library)


def test_legacy_text_mode_newlines_match_without_changing_original_bytes(library):
    source = library / "wiki/concepts/crlf.md"
    original = b"---\r\ntitle: CRLF\r\n---\r\nalpha\r\nbeta\rgamma\r\n"
    source.write_bytes(original)
    assert search_module.search("alpha", base_dir=library) == search_module._legacy_search("alpha", base_dir=library)
    assert source.read_bytes() == original


def test_malformed_raw_skips_and_empty_queries_do_not_parse_compiled(library, monkeypatch):
    malformed = "---\ntitle: [unterminated\n---\nalpha"
    (library / "raw/bad.md").write_text(malformed)
    (library / "raw/null.md").write_text("---\ntitle: null\nsummary: null\ntags: false\n---\nalpha")
    assert search_module.search_raw("alpha", base_dir=library) == search_module._legacy_search_raw("alpha", base_dir=library)
    (library / "wiki/concepts/bad.md").write_text(malformed)
    assert search_module.search("the", base_dir=library) == []
    assert search_module.search("", base_dir=library) == []
    with pytest.raises(ValueError):
        search_module.search("alpha", base_dir=library)
    monkeypatch.setattr(search_module, "SEARCH_TOKENIZER", lambda text: [])
    assert search_module.search("custom-empty", base_dir=library) == []
    monkeypatch.setattr(search_module, "SEARCH_TOKENIZER", reject_python_core)
    assert search_module.search_raw("alpha", top_k=0, base_dir=library) == []


def test_native_callback_snapshot_rejects_mutation(library, monkeypatch):
    def mutate_during_tokenize(text):
        if text != "query":
            (library / "wiki/concepts/alpha.md").write_text("changed")
        return ["match"]
    monkeypatch.setattr(search_module, "SEARCH_TOKENIZER", mutate_during_tokenize)
    with pytest.raises(RuntimeError, match="snapshot changed"):
        search_module.search("query", base_dir=library)


def test_missing_extension_fails_native_required_and_candidate_profile(monkeypatch):
    original_import = importlib.import_module
    def missing(name, *args, **kwargs):
        if name == "llmwiki._core":
            raise ModuleNotFoundError("missing", name=name)
        return original_import(name, *args, **kwargs)
    monkeypatch.setattr(importlib, "import_module", missing)
    monkeypatch.setenv("LLMBASE_REQUIRE_NATIVE", "1")
    with pytest.raises(_native.NativeCapabilityError):
        _native.available()
    monkeypatch.delenv("LLMBASE_REQUIRE_NATIVE")
    monkeypatch.setattr(_native, "_installed_candidate_requires_native", lambda: True)
    with pytest.raises(_native.NativeCapabilityError):
        _native.available()


def test_extension_errors_cannot_trigger_legacy_fallback(library, monkeypatch):
    monkeypatch.setattr(search_module, "_legacy_search", reject_python_core)
    extension = importlib.import_module("llmwiki._core")
    with pytest.raises(ValueError):
        extension.dispatch(str(library), "default", "compat_search", "not-json")
    with pytest.raises(RuntimeError, match="unknown or unimplemented operation"):
        _native.dispatch("deliberately-unsupported", {}, library)
    assert _native.last_trace() is None


def test_candidate_identity_is_machine_readable():
    info = _native.backend_info()
    assert info["native"] is True
    assert info["core"] == "rust"
    assert info["abi"] == "abi3-py311"
    assert info["application"] == "llmbase_core::application::Application"
    assert Path(info["extension_path"]).is_file()
    assert json.loads(json.dumps(info))["crate"] == "llmbase-python"
