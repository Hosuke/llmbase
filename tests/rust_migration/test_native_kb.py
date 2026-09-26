"""Installed-candidate contracts for native KnowledgeBase reads and local ingest writes."""
import importlib
import json
from pathlib import Path

import frontmatter
import pytest

from llmwiki import _native, operations
from llmwiki.agent_api import KnowledgeBase, create_agent_server
from llmwiki.ingest import list_raw
from llmwiki.sections import parse_sections


@pytest.fixture(autouse=True)
def native_candidate():
    if not _native.available():
        pytest.skip("native candidate extension is not built")


@pytest.fixture
def library(tmp_path):
    (tmp_path / "wiki/concepts").mkdir(parents=True)
    (tmp_path / "wiki/_meta").mkdir()
    (tmp_path / "raw/book").mkdir(parents=True)
    (tmp_path / "wiki/concepts/alpha.md").write_text("---\ntitle: null\nsummary: 42\ntags: [經, 7, false]\nsources: {kind: original}\n---\n序𠮷é\n## 所出\n正文\n### 字例\n𠮷é\n")
    (tmp_path / "wiki/concepts/beta.md").write_text("---\ntitle: Beta\ntags: 經學\n---\nsecond")
    (tmp_path / "raw/book/index.md").write_text("---\ntitle: null\nsource: /private/example/file\ncustom: {safe: 7, private: /private/example/key, nothing: null}\nitems: [one, /private/example/key, null, false]\n---\n原典")
    (tmp_path / "wiki/_meta/aliases.json").write_text('{"x-y":"beta","xy":"alpha","named-key":"alpha","escape":"../secret"}')
    return tmp_path


def forbid_python_read(*args, **kwargs):
    raise AssertionError("legacy Python core read was used")


def test_get_list_stats_and_raw_list_use_native_without_python_loaders(library, monkeypatch):
    expected = {}
    with monkeypatch.context() as legacy:
        legacy.setattr(_native, "available", lambda: False)
        for name, args in [("kb_get", {"slug": "alpha"}), ("kb_list", {"tag": "經"}), ("kb_stats", {})]:
            expected[name] = operations.dispatch(name, library, args)
        expected["raw"] = list_raw(library)
    monkeypatch.setattr(frontmatter, "load", forbid_python_read)
    monkeypatch.setattr(Path, "read_text", forbid_python_read)
    monkeypatch.setattr(Path, "read_bytes", forbid_python_read)
    for name, args in [("kb_get", {"slug": "alpha"}), ("kb_list", {"tag": "經"}), ("kb_stats", {})]:
        assert operations.dispatch(name, library, args) == expected[name]
        assert _native.last_trace()["core"] == "rust"
    assert list_raw(library) == expected["raw"]
    assert not (library / "wiki/_meta/llmbase.sqlite3").exists()


def test_kb_runtime_cfg_changes_and_distinct_envelopes(library, monkeypatch):
    kb = KnowledgeBase(library)
    assert kb.get_article("alpha") == {"status": "ok", "slug": "alpha", "title": None,
        "summary": 42, "tags": ["經", 7, False], "content": "序𠮷é\n## 所出\n正文\n### 字例\n𠮷é"}
    assert kb.get_article("Named Key") == {"status": "error", "message": "Article not found: Named Key"}
    assert operations.dispatch("kb_get", library, {"slug": "Named Key"})["slug"] == "alpha"
    alternate = library / "alternate"
    alternate.mkdir()
    (alternate / "only.md").write_text("---\ntitle: false\nsummary: [one, two]\ntags: {key: 9}\n---\nother body")
    kb.cfg["paths"]["concepts"] = str(alternate)
    monkeypatch.setattr(frontmatter, "load", forbid_python_read)
    assert kb.list_articles()["articles"] == [{"slug": "only", "title": False, "summary": ["one", "two"], "tags": {"key": 9}}]
    assert kb.get_article("only")["title"] is False
    assert [a["slug"] for a in operations.dispatch("kb_list", library)["articles"]] == ["alpha", "beta"]


@pytest.mark.parametrize("body", [
    "序𠮷é\n## 所出\n正文\n### 字例\n𠮷é\n",
    "# A\n## 子\n```python\n## hidden\n```\n## 子\n",
    "# A\r\n### C#\r\n# B\r\n",
    "~~~\n# hidden\n~~~~\n   ## 開\n    ### not a heading\n",
    "前\u2028## 節\u2028後\n",
    "## 《同》\n## 同\n## 同\n",
    "## \u200b《標 題》\n### 子𠮷é\n",
])
def test_native_sections_match_legacy_anchors_and_scalar_offsets(library, body):
    post = frontmatter.Post(body, title="fixture")
    path = library / "wiki/concepts/sections.md"
    path.write_text(frontmatter.dumps(post))
    canonical = frontmatter.load(path).content
    expected = parse_sections(canonical)
    result = operations.dispatch("kb_get_sections", library, {"slug": "sections"})
    assert result["sections"] == expected
    for section in expected:
        extracted = operations.dispatch("kb_get", library, {"slug": "sections", "section": section["anchor"]})
        assert extracted["content"] == canonical[section["start"]:section["end"]]


def test_alias_file_order_missing_sections_and_path_escape(library):
    assert operations.dispatch("kb_get", library, {"slug": "x.y!"})["slug"] == "beta"
    assert operations.dispatch("kb_get", library, {"slug": "alpha", "section": "missing"}) == {
        "found": True, "slug": "alpha", "title": None, "summary": 42, "tags": ["經", 7, False],
        "sources": {"kind": "original"}, "section": "missing", "section_found": False}
    assert operations.dispatch("kb_get", library, {"slug": "escape"}) == {"found": False, "slug": "escape"}
    assert operations.dispatch("kb_get", library, {"slug": "../../outside"})["found"] is False


def test_live_custom_registry_handler_and_flask_factory_stay_mutable(library, monkeypatch):
    client = create_agent_server(library).test_client()
    original = operations.get("kb_list")
    monkeypatch.setitem(operations._REGISTRY, "kb_list", operations.Operation(
        "kb_list", "custom", lambda base_dir, **kwargs: {"articles": [], "custom": str(base_dir)}, required_scopes=("read",)))
    response = client.post("/api/op/kb_list", json={})
    assert response.status_code == 200
    assert response.get_json()["result"]["custom"] == str(library)
    monkeypatch.setitem(operations._REGISTRY, "kb_list", original)
    assert len(client.post("/api/op/kb_list", json={}).get_json()["result"]["articles"]) == 2


def test_native_markdown_persistence_preserves_unknown_metadata_modes_and_hooks(library, tmp_path, monkeypatch):
    ingest = importlib.import_module("llmwiki.ingest")
    hooks = importlib.import_module("llmwiki.hooks")
    source = tmp_path / "input.md"
    source.write_text("---\ntitle: null\nunknown: {a: [1, null, false]}\n---\nexact body 𠮷\n")
    source.chmod(0o640)
    (library / "raw/malformed.md").write_text("---\ntitle: [broken\n---\nbad")
    events = []
    monkeypatch.setattr(hooks, "_registry", {})
    hooks.register("ingested", lambda **kwargs: events.append(kwargs))
    monkeypatch.setattr(ingest, "_slugify", lambda text: "custom-slug")
    monkeypatch.setattr(ingest.shutil, "copy2", forbid_python_read)
    original_load = frontmatter.load
    def external_only(path, *args, **kwargs):
        assert Path(path) == source
        return original_load(path, *args, **kwargs)
    with monkeypatch.context() as persistence:
        persistence.setattr(frontmatter, "load", external_only)
        persistence.setattr(Path, "write_text", forbid_python_read)
        dest = ingest.ingest_file(str(source), library)
    assert dest == library / "raw/custom-slug/input.md"
    post = original_load(dest)
    assert post["title"] is None and post["unknown"] == {"a": [1, None, False]}
    assert post["compiled"] is False and post["type"] == "local_file"
    assert post.content == "exact body 𠮷"
    assert dest.stat().st_mode & 0o777 == 0o640
    assert events == [{"source": "file", "title": "input", "path": str(dest)}]
    assert _native.last_trace()["writer_lock"] is True
    assert not (library / "wiki/_meta/llmbase.sqlite3").exists()


def test_non_utf8_local_text_bytes_are_preserved(library, tmp_path):
    from llmwiki.ingest import ingest_file
    source = tmp_path / "encoding.txt"
    original = b"caf\xe9\r\nkeep bytes\x00"
    source.write_bytes(original)
    dest = ingest_file(str(source), library)
    assert dest.read_bytes() == original
    assert (dest.parent / "index.md").exists()


def test_native_writer_lock_blocks_read_and_write_and_failure_does_not_emit(library, tmp_path, monkeypatch):
    import fcntl
    from llmwiki.ingest import ingest_file
    hooks = importlib.import_module("llmwiki.hooks")
    source = tmp_path / "blocked.md"
    source.write_text("content")
    events = []
    monkeypatch.setattr(hooks, "_registry", {})
    hooks.register("ingested", lambda **kwargs: events.append(kwargs))
    with (library / "wiki/_meta/llmbase.writer.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        with pytest.raises(RuntimeError, match="busy|writer"):
            ingest_file(str(source), library)
        with pytest.raises(RuntimeError, match="busy|writer"):
            operations.dispatch("kb_list", library)
    assert events == []
    assert not (library / "raw/blocked/blocked.md").exists()


def test_health_report_uses_live_config_and_native_json_read(library, monkeypatch):
    kb = KnowledgeBase(library)
    assert kb.health_report()["report"] is None
    alternate = library / "other-meta"
    alternate.mkdir()
    (alternate / "health.json").write_text(json.dumps({"unknown": [1, None, False]}))
    kb.cfg["paths"]["meta"] = str(alternate)
    monkeypatch.setattr(Path, "read_text", forbid_python_read)
    assert kb.health_report() == {"status": "ok", "report": {"unknown": [1, None, False]}}


def test_write_registry_holds_native_lease_and_nested_dispatch_reuses_it(library, monkeypatch):
    import subprocess
    import sys
    from concurrent.futures import ThreadPoolExecutor
    from llmwiki.search import search_raw
    from llmwiki.worker import job_lock

    def handler(base_dir):
        assert job_lock.locked()
        _native.dispatch("compat_kb_raw_write", {"path": "written.md", "content": "writtenmarker"}, base_dir)
        assert search_raw("writtenmarker", base_dir=base_dir)[0]["source"] == "written"
        assert len(operations.dispatch("kb_list", base_dir)["articles"]) == 2
        with ThreadPoolExecutor(max_workers=1) as pool:
            future = pool.submit(_native.dispatch, "compat_kb_raw_write",
                                 {"path": "other-thread.md", "content": "blocked"}, base_dir)
            with pytest.raises(RuntimeError, match="writer|busy"):
                future.result(timeout=5)
        child = subprocess.run([sys.executable, "-c", "from llmwiki import _native; import sys\n"
            "try: _native.dispatch('compat_kb_raw_write', {'path':'other-process.md','content':'blocked'},sys.argv[1])\n"
            "except RuntimeError: print('busy')\nelse: raise SystemExit('writer exclusion failed')", str(base_dir)],
            cwd=base_dir, capture_output=True, text=True, timeout=10)
        assert child.returncode == 0, child.stderr
        assert child.stdout.strip() == "busy"
        return {"actual_callback": True}

    before = _native.backend_info()["writer_sessions"]
    monkeypatch.setitem(operations._REGISTRY, "kb_native_writer_test",
        operations.Operation("kb_native_writer_test", "fixture", handler, writes=True))
    assert operations.dispatch("kb_native_writer_test", library) == {"actual_callback": True}
    assert _native.backend_info()["writer_sessions"] == before + 1
    assert not job_lock.locked()
    assert not (library / "raw/other-thread.md").exists()
    assert not (library / "raw/other-process.md").exists()


def test_writer_callback_identity_exceptions_and_reentrant_lease_release(library):
    obj = object()
    assert _native.with_writer(library, lambda: _native.with_writer(library, lambda: obj)) is obj
    def failing():
        raise LookupError("original callback exception")
    with pytest.raises(LookupError, match="original callback exception"):
        _native.with_writer(library, failing)
    assert _native.dispatch("compat_kb_raw_write", {"path": "after-exception.md", "content": "released"}, library)["result"]["path"]


@pytest.mark.parametrize("arguments,locked", [({}, False), ({"file_back": False}, False),
    ({"file_back": True}, True), ({"promote": True}, True), ({"file_back": True, "deep": False}, True)])
def test_ask_file_back_and_promotion_acquire_real_writer_lease(library, monkeypatch, arguments, locked):
    from llmwiki.worker import job_lock
    def handler(base_dir, **kwargs):
        assert job_lock.locked() is locked
        return kwargs
    monkeypatch.setitem(operations._REGISTRY, "kb_ask",
        operations.Operation("kb_ask", "live fixture handler", handler))
    before = _native.backend_info()["writer_sessions"]
    assert operations.dispatch("kb_ask", library, arguments) == arguments
    assert _native.backend_info()["writer_sessions"] == before + int(locked)
