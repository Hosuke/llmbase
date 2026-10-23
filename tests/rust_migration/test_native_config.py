"""Real config IO/ordering through the installed native candidate."""
import builtins
import errno
import os
from pathlib import Path

import pytest

from llmwiki import _native, config

pytestmark = pytest.mark.skipif(not _native.available(), reason="native candidate required")


def test_native_read_and_mkdir_do_not_use_python_fallback_or_io(tmp_path, monkeypatch):
    (tmp_path / "config.yaml").write_text("paths: {raw: raw, wiki: wiki, outputs: out, meta: meta, concepts: concepts}\nunknown: kept\n")
    def poison(*args, **kwargs):
        raise AssertionError("Python filesystem algorithm called")
    with monkeypatch.context() as patch:
        patch.setattr(config, "_read_config_python", poison)
        patch.setattr(config, "_ensure_dirs_python", poison)
        patch.setattr(builtins, "open", poison)
        patch.setattr(Path, "mkdir", poison)
        patch.setattr(Path, "resolve", poison)
        cfg = config._read_config(tmp_path)
        assert _native.last_trace()["operation"] == "config_read"
        config.ensure_dirs(cfg)
        assert _native.last_trace()["operation"] == "config_ensure_dirs"
    assert cfg["unknown"] == "kept"
    assert all(Path(p).is_dir() for p in cfg["paths"].values())
    assert not list(tmp_path.rglob("*.sqlite3"))


@pytest.mark.parametrize("document,kind", [
    ("null", AttributeError), ("[]", AttributeError), ("42", AttributeError),
    ("paths: null", AttributeError), ("paths: []", AttributeError),
    ("paths: {raw: 42}", TypeError), ("paths: {raw: [bad]}", TypeError),
    ("paths: {raw: 2024-01-02}", TypeError),
])
def test_invalid_document_order_and_no_success_trace(tmp_path, document, kind):
    (tmp_path / "config.yaml").write_text(document)
    config._defaults(tmp_path)
    with pytest.raises(kind):
        config._read_config(tmp_path)
    assert _native.last_trace() is None
    assert sorted(p.name for p in tmp_path.iterdir()) == ["config.yaml"]


def test_live_parser_opaque_mapping_and_exact_mutation_order(tmp_path, monkeypatch):
    (tmp_path / "config.yaml").write_bytes(b"ignored\r\nby parser\r")
    events = []
    class LivePath:
        def __bool__(self):
            events.append("truth")
            return True
        def __fspath__(self):
            events.append("coerce")
            return "raw"
    class Paths(dict):
        def get(self, key, default=None):
            events.append(("get", key))
            return super().get(key, default)
        def __setitem__(self, key, value):
            events.append(("set", key))
            super().__setitem__(key, value)
    paths = Paths(raw=LivePath(), wiki=None, outputs="", meta=False, concepts=0)
    document = {"paths": paths, "same": paths}
    document["self"] = document
    def decode(text):
        assert text == "ignored\nby parser\n"
        return document
    monkeypatch.setattr(config.yaml, "safe_load", decode)
    assert config._read_config(tmp_path) is document
    assert document["self"] is document and document["same"] is paths
    assert events == [("get", "raw"), "truth", "coerce", ("set", "raw"),
                      ("get", "wiki"), ("get", "outputs"), ("get", "meta"), ("get", "concepts")]


@pytest.mark.parametrize("stage", ["parser", "defaults", "path", "mkdir"])
def test_callback_exception_identity_and_nested_trace_cleanup(tmp_path, monkeypatch, stage):
    marker = RuntimeError("original callback error")
    def fail(*args):
        _native.utility_transform("normalize_paragraphs", {"body": "", "terminators": ".", "wrappers": ""})
        raise marker
    if stage == "parser":
        (tmp_path / "config.yaml").write_text("{}")
        monkeypatch.setattr(config.yaml, "safe_load", fail)
    elif stage == "defaults":
        monkeypatch.setattr(config, "_defaults", fail)
    elif stage == "path":
        (tmp_path / "config.yaml").write_text("paths: {raw: raw}")
        monkeypatch.setattr(config, "_path_value", fail)
    else:
        monkeypatch.setattr(config, "_path_value", fail)
    with pytest.raises(RuntimeError) as caught:
        if stage == "mkdir":
            config.ensure_dirs({"paths": {"raw": "raw"}})
        else:
            config._read_config(tmp_path)
    assert caught.value is marker
    assert _native.last_trace() is None


def test_live_defaults_are_only_called_for_missing_file_and_not_normalized(tmp_path, monkeypatch):
    calls = []
    document = {"paths": {"raw": "keep-relative"}, "opaque": object()}
    def defaults(base):
        calls.append(base)
        return document
    monkeypatch.setattr(config, "_defaults", defaults)
    assert config._read_config(tmp_path) is document
    assert calls == [tmp_path]
    assert document["paths"]["raw"] == "keep-relative"
    (tmp_path / "config.yaml").write_text("unknown: true")
    assert config._read_config(tmp_path) == {"unknown": True, "base_dir": str(tmp_path)}
    assert calls == [tmp_path]


def test_native_io_retains_os_error_fields_and_partial_mkdir(tmp_path):
    (tmp_path / "config.yaml").mkdir()
    with pytest.raises(IsADirectoryError) as caught:
        config._read_config(tmp_path)
    assert caught.value.errno == 21
    assert str(caught.value.filename) == str(tmp_path / "config.yaml")
    assert isinstance(caught.value.filename, str)
    cfg = {"paths": {"raw": str(tmp_path / "raw"), "wiki": str(tmp_path / "blocker"), "outputs": str(tmp_path / "later")}}
    (tmp_path / "blocker").write_text("file")
    with pytest.raises(FileExistsError) as caught:
        config.ensure_dirs(cfg)
    assert caught.value.errno == 17
    assert str(caught.value.filename) == str(tmp_path / "blocker")
    assert isinstance(caught.value.filename, str)
    assert (tmp_path / "raw").is_dir() and not (tmp_path / "later").exists()
    assert _native.last_trace() is None


def test_native_reader_rejects_oversize_before_decoder(tmp_path, monkeypatch):
    (tmp_path / "config.yaml").write_bytes(b" " * (16 * 1024 * 1024 + 1))
    def decoder(_):
        raise AssertionError("oversize reached decoder")
    monkeypatch.setattr(config.yaml, "safe_load", decoder)
    with pytest.raises(ValueError, match="16 MiB"):
        config._read_config(tmp_path)
    assert _native.last_trace() is None


def test_load_registers_once_after_success_and_read_never_registers(tmp_path, monkeypatch):
    calls = []
    monkeypatch.setattr(_native, "register_config", lambda *args: calls.append(args))
    config._read_config(tmp_path)
    assert calls == []
    cfg = config.load_config(tmp_path)
    assert calls == [(cfg, tmp_path)]
    (tmp_path / "config.yaml").write_text("null")
    with pytest.raises(AttributeError):
        config.load_config(tmp_path)
    assert len(calls) == 1


@pytest.mark.skipif(os.name != "posix", reason="POSIX surrogateescape filenames")
def test_non_utf8_path_bytes_are_not_replaced(tmp_path):
    base = tmp_path / os.fsdecode(b"library-\xff")
    cfg = config._read_config(base)
    assert cfg["base_dir"] == str(base)
    assert cfg["paths"]["raw"] == str(base / "raw")
    # Some POSIX filesystems (including macOS APFS) reject these bytes. The
    # bridge must preserve that OS error and the original path, not replace it.
    try:
        (tmp_path / os.fsdecode(b"probe-\xff")).mkdir()
    except OSError as platform_error:
        assert platform_error.errno == errno.EILSEQ
        with pytest.raises(OSError) as caught:
            config.ensure_dirs(cfg)
        assert caught.value.errno == platform_error.errno
        assert caught.value.filename == str(base)
        assert _native.last_trace() is None
        return
    config.ensure_dirs(cfg)
    (base / "config.yaml").write_text("paths: {raw: raw}\n")
    assert config._read_config(base)["paths"]["raw"] == str(base / "raw")


def test_embedded_null_path_fails_with_no_trace(tmp_path):
    with pytest.raises(ValueError, match="null byte"):
        config._read_config(tmp_path / "bad\0name")
    assert _native.last_trace() is None


def test_default_profile_failure_clears_nested_path_receipt(tmp_path, monkeypatch):
    marker = RuntimeError("profile copy failed")
    class BadProfile:
        def __deepcopy__(self, memo):
            raise marker
    monkeypatch.setattr(config, "BUILTIN_LANGUAGES", BadProfile())
    with pytest.raises(RuntimeError) as caught:
        config._defaults(tmp_path)
    assert caught.value is marker
    assert _native.last_trace() is None


@pytest.mark.parametrize("present", [False, True])
def test_absolute_configuration_works_after_cwd_is_deleted(tmp_path, present):
    base = tmp_path / "library"
    base.mkdir()
    if present:
        (base / "config.yaml").write_text("paths: {raw: raw, wiki: wiki}\n")
    previous = Path.cwd()
    doomed = tmp_path / "cwd"
    doomed.mkdir()
    os.chdir(doomed)
    doomed.rmdir()
    try:
        cfg = config._read_config(base)
        assert cfg["base_dir"] == str(base)
        assert cfg["paths"]["raw"] == str(base / "raw")
        assert config._defaults(base)["paths"]["concepts"] == str(base / "wiki/concepts")
    finally:
        os.chdir(previous)
