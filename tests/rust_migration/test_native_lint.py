"""Installed native lint sequencing and actual live extension contracts."""
from pathlib import Path
import shutil
import sys

import frontmatter
import pytest

from llmwiki import _native, hooks, taxonomy
from llmwiki.config import ensure_dirs, load_config
from llmwiki.lint import checks

pytestmark = pytest.mark.skipif(not _native.available(), reason="native candidate required")


@pytest.fixture
def library(tmp_path, monkeypatch):
    source = Path(__file__).parent / "fixtures/downstream/library"
    base = tmp_path / "library"
    shutil.copytree(source, base)
    ensure_dirs(load_config(base))
    monkeypatch.setattr(hooks, "_registry", {})
    return base


def file_bytes(base):
    return {p.relative_to(base).as_posix(): p.read_bytes()
            for p in base.rglob("*") if p.is_file()}


def assert_run_trace():
    assert _native.last_trace() == {
        "core": "rust", "operation": "maintenance_run", "action": "lint"}


def test_default_checks_and_snapshot_execute_in_rust(library, monkeypatch):
    before = file_bytes(library)
    entered = []
    old_codes = {function.__code__ for function in checks._DEFAULT_CHECKS.values()}
    def observe(frame, event, arg):
        if event == "call" and frame.f_code in old_codes:
            entered.append(frame.f_code.co_name)
    def forbidden(*args, **kwargs):
        raise AssertionError("default lint parsed frontmatter in Python")
    monkeypatch.setattr(frontmatter, "load", forbidden)
    monkeypatch.setattr(taxonomy, "build_taxonomy", lambda base, lang: [])
    observed_hooks = []
    hooks.register("after_lint_check", lambda **kwargs: observed_hooks.append(kwargs))
    previous = sys.getprofile()
    try:
        sys.setprofile(observe)
        result = checks.lint(library)
    finally:
        sys.setprofile(previous)
    assert entered == []
    names = set(checks._DEFAULT_CHECKS)
    assert set(result) == names | {"total_issues"}
    assert all(isinstance(result[name], list) for name in names)
    assert result["total_issues"] == sum(len(result[name]) for name in names)
    assert observed_hooks == [{"total_issues": result["total_issues"], "results": result}]
    assert_run_trace()
    assert file_bytes(library) == before


def test_native_loader_preserves_path_metadata_and_does_not_register_phantom_root(library):
    cfg = load_config(library)
    roots_before = set(_native._configured_roots)
    before = file_bytes(library)
    articles = checks._load_articles(Path(cfg["paths"]["concepts"]))
    assert _native.last_trace()["operation"] == "maintenance_transform"
    assert _native.last_trace()["action"] == "load"
    assert isinstance(articles[0]["path"], Path)
    assert articles[0]["metadata"]["editorial_extension"]["revision_note"] == "保留未知欄"
    assert set(_native._configured_roots) == roots_before
    assert file_bytes(library) == before


def test_cfg_only_override_is_live_and_called_once(library, monkeypatch):
    calls = []
    def first(config):
        calls.append(config)
        return ["first callback"]
    monkeypatch.setattr(checks, "check_stubs", first)
    assert checks.lint(library)["stubs"] == ["first callback"]
    assert len(calls) == 1
    assert Path(calls[0]["paths"]["concepts"]) == library / "knowledge/entries"
    monkeypatch.setattr(checks, "check_stubs", lambda cfg: ["second callback"])
    assert checks.lint(library)["stubs"] == ["second callback"]
    assert len(calls) == 1
    assert_run_trace()


def test_live_cjk_policy_changes_native_default(library, monkeypatch):
    concepts = library / "knowledge/entries"
    (concepts / "學.md").write_text("---\ntitle: 學\nsummary: 義\ntags: [經]\n---\n" + "文言正文。" * 20)
    monkeypatch.setattr(checks, "ALLOW_CJK_SLUGS", False)
    assert "CJK slug (should be pinyin): 學" in checks.lint(library)["stubs"]
    monkeypatch.setattr(checks, "ALLOW_CJK_SLUGS", True)
    assert "CJK slug (should be pinyin): 學" not in checks.lint(library)["stubs"]
    assert_run_trace()


@pytest.mark.parametrize("kind", ["full", "subset", "positional_kwargs", "optional_gap", "opaque"])
def test_callback_shapes_keep_positional_meaning_and_optional_arguments(library, monkeypatch, kind):
    seen = []
    if kind == "full":
        def callback(config, articles=None, existing_slugs=None, aliases=None):
            seen.append((config, articles, existing_slugs, aliases))
            return [kind]
    elif kind == "subset":
        def callback(config, aliases):
            seen.append((config, aliases))
            return [kind]
    elif kind == "positional_kwargs":
        def callback(config, /, **kwargs):
            assert set(kwargs) == {"articles", "existing_slugs", "aliases"}
            seen.append((config, kwargs))
            return [kind]
    elif kind == "optional_gap":
        def callback(config, marker="default", articles=None, /):
            assert marker == "default"
            assert isinstance(articles[0]["path"], Path)
            seen.append((config, articles))
            return [kind]
    else:
        class Opaque:
            __signature__ = "not introspectable"
            def __call__(self, config, **kwargs):
                assert set(kwargs) == {"articles", "existing_slugs", "aliases"}
                seen.append((config, kwargs))
                return [kind]
        callback = Opaque()
    monkeypatch.setattr(checks, "check_broken_links", callback)
    assert checks.lint(library)["broken_links"] == [kind]
    assert len(seen) == 1
    assert Path(seen[0][0]["base_dir"]) == library
    assert_run_trace()


def test_replaced_loader_and_python_callbacks_share_path_valued_snapshot(library, monkeypatch):
    cfg = load_config(library)
    rows = checks._load_articles(Path(cfg["paths"]["concepts"]))
    rows[0]["slug"] = "live-slug"
    seen = []
    def loader(path):
        seen.append(path)
        return rows
    def structural(cfg, articles):
        assert articles is rows
        return []
    def broken(cfg, articles, existing_slugs, aliases):
        assert articles is rows and isinstance(articles[0]["path"], Path)
        assert existing_slugs == {"live-slug"} and isinstance(existing_slugs, set)
        assert isinstance(aliases, dict)
        return []
    monkeypatch.setattr(checks, "_load_articles", loader)
    monkeypatch.setattr(checks, "check_structural", structural)
    monkeypatch.setattr(checks, "check_broken_links", broken)
    checks.lint(library)
    assert seen == [Path(cfg["paths"]["concepts"])]
    assert_run_trace()


@pytest.mark.parametrize("error_type", [TypeError, KeyboardInterrupt])
def test_callback_exception_is_preserved_once_without_success_trace_or_hook(library, monkeypatch, error_type):
    expected = error_type("body sentinel")
    calls, emitted = [], []
    def callback(cfg):
        calls.append(True)
        _native.maintenance_transform("load", {"concepts_dir": cfg["paths"]["concepts"]})
        raise expected
    monkeypatch.setattr(checks, "check_stubs", callback)
    hooks.register("after_lint_check", lambda **kwargs: emitted.append(kwargs))
    with pytest.raises(error_type) as caught:
        checks.lint(library)
    assert caught.value is expected and calls == [True]
    assert emitted == [] and _native.last_trace() is None


def test_binding_error_never_executes_callback(library, monkeypatch):
    calls = []
    def callback(cfg, missing_required):
        calls.append(True)
        return []
    monkeypatch.setattr(checks, "check_stubs", callback)
    with pytest.raises(TypeError):
        checks.lint(library)
    assert calls == [] and _native.last_trace() is None


def test_stages_are_ordered_and_taxonomy_boundary_is_lazy(library, monkeypatch):
    order = []
    names = ["structural", "broken_links", "orphans", "missing_metadata",
             "dirty_tags", "duplicates", "stubs"]
    def replacement(name):
        def callback(cfg):
            order.append(name)
            return []
        return callback
    for name in names:
        monkeypatch.setattr(checks, "check_" + name, replacement(name))
    def categories(base, lang):
        assert base == library and lang == "en"
        order.append("taxonomy")
        return [{"id": "other", "articles": [{"slug": "xue"}]}]
    monkeypatch.setattr(taxonomy, "build_taxonomy", categories)
    hooks.register("after_lint_check", lambda **kwargs: order.append("emit"))
    assert checks.lint(library)["uncategorized"] == ["Uncategorized article: xue"]
    assert order == names + ["taxonomy", "emit"]
    order.clear()
    def uncategorized(config, *, base_dir):
        assert base_dir == library
        order.append("uncategorized")
        return ["replacement"]
    monkeypatch.setattr(checks, "check_uncategorized", uncategorized)
    assert checks.lint(library)["uncategorized"] == ["replacement"]
    assert order == names + ["uncategorized", "emit"]
    assert_run_trace()


def test_unsupported_callback_metadata_is_not_silently_stringified(library, monkeypatch):
    cfg = load_config(library)
    rows = checks._load_articles(Path(cfg["paths"]["concepts"]))
    rows[0]["metadata"]["unsupported"] = object()
    monkeypatch.setattr(checks, "_load_articles", lambda path: rows)
    with pytest.raises(TypeError, match="JSON serializable"):
        checks.lint(library)
    assert _native.last_trace() is None
