"""Finite actual-old comparison for the installed B16 native config candidate."""

from __future__ import annotations

import datetime as _datetime
import hashlib
import importlib.util
import math
import os
from pathlib import Path
import sys
import tempfile

from llmwiki import _core, _native, config


HERE = Path(__file__).resolve().parent
ORACLE_SOURCE = HERE / "legacy_config.py"
ORACLE_SHA256 = "2a8fc0f92dcf27a7fed4f14f98ea1d91e4a61b17a4ef3920fb70928cd04fc482"
assert hashlib.sha256(ORACLE_SOURCE.read_bytes()).hexdigest() == ORACLE_SHA256
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
assert Path(config.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())

spec = importlib.util.spec_from_file_location(
    "llmwiki._legacy_config_oracle", ORACLE_SOURCE
)
assert spec is not None and spec.loader is not None
oracle = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = oracle
spec.loader.exec_module(oracle)


def graph_encode(root):
    """Encode value types, container identities, aliases, and cycles."""
    seen = {}

    def visit(value):
        if value is None or isinstance(value, (bool, int, str)):
            return (type(value).__module__, type(value).__qualname__, value)
        if isinstance(value, float):
            if math.isnan(value):
                payload = "nan"
            elif math.isinf(value):
                payload = "+inf" if value > 0 else "-inf"
            else:
                payload = value.hex()
            return ("builtins", "float", payload)
        if isinstance(value, bytes):
            return ("builtins", "bytes", value.hex())
        if isinstance(value, (_datetime.date, _datetime.datetime)):
            return (type(value).__module__, type(value).__qualname__, value.isoformat())

        identity = id(value)
        if identity in seen:
            return ("ref", seen[identity])
        node = len(seen)
        seen[identity] = node
        typename = (type(value).__module__, type(value).__qualname__)
        if isinstance(value, dict):
            return ("dict", node, typename, tuple((visit(k), visit(v)) for k, v in value.items()))
        if isinstance(value, list):
            return ("list", node, typename, tuple(visit(item) for item in value))
        if isinstance(value, tuple):
            return ("tuple", node, typename, tuple(visit(item) for item in value))
        if isinstance(value, (set, frozenset)):
            # YAML sets have hashable scalar keys. Sorting before traversal keeps
            # independently loaded graphs deterministic without losing node IDs.
            ordered = sorted(value, key=lambda item: (type(item).__module__,
                                                       type(item).__qualname__, repr(item)))
            return ("set", node, typename, tuple(visit(item) for item in ordered))
        return ("object", node, typename, repr(value))

    return visit(root)


def exception_signature(exc, root=None):
    def normalize(filename):
        if filename is None:
            return None
        text = os.fspath(filename)
        if root is not None:
            prefix = os.fspath(root)
            if text == prefix:
                return "<ROOT>"
            if text.startswith(prefix + os.sep):
                return "<ROOT>" + text[len(prefix):]
        return text

    return {
        "class": (type(exc).__module__, type(exc).__qualname__),
        "errno": getattr(exc, "errno", None),
        "filename": normalize(getattr(exc, "filename", None)),
        "filename2": normalize(getattr(exc, "filename2", None)),
    }


def invoke(function, *args, error_root=None):
    try:
        return {"kind": "value", "value": graph_encode(function(*args))}
    except BaseException as exc:
        return {"kind": "error", "error": exception_signature(exc, error_root)}


def assert_trace(operation, *, parser=None):
    trace = _native.last_trace()
    assert isinstance(trace, dict), (operation, trace)
    assert trace.get("core") == "rust", trace
    assert trace.get("operation") == operation, trace
    if parser is not None:
        assert trace.get("parser") == parser, trace


RICH_YAML = """\
paths:
  raw: raw/../raw-data
  wiki: knowledge
  outputs: ../outside-output
  meta: knowledge/state
  concepts: knowledge/../entries
date_value: 2024-02-29
positive_infinity: .inf
negative_infinity: -.Inf
not_a_number: .NaN
binary_value: !!binary AAH/
typed_keys:
  2025-01-02: date-key
  7: integer-key
set_value: !!set
  red: null
  blue: null
ordered_value: !!omap
  - first: 1
  - second: 2
shared_value: &shared
  nested: &nested [1, 2, three]
alias_one: *shared
alias_two: *shared
merge_source: &merge_source
  inherited: *nested
merged_value:
  <<: *merge_source
  own: true
cyclic_value: &cycle
  - *cycle
unicode_value: "未知𠮷é"
"""


def compare_read(base, parser):
    expected = invoke(oracle._read_config, base)
    actual = invoke(config._read_config, base)
    assert actual == expected, (base, expected, actual)
    if actual["kind"] == "value":
        assert_trace("config_read", parser=parser)
    else:
        assert _native.last_trace() is None


class RecordingMap(dict):
    def __init__(self, tag, events, values):
        super().__init__(values)
        self.tag = tag
        self.events = events

    def __getitem__(self, key):
        self.events.append((self.tag, key))
        return super().__getitem__(key)


def directory_config(root, events):
    values = {
        "raw": str(root / "01-raw"),
        "wiki": str(root / "02-wiki"),
        "outputs": str(root / "03-blocker" / "child"),
        "meta": str(root / "04-meta"),
        "concepts": str(root / "05-concepts"),
    }
    paths = RecordingMap("paths", events, values)
    return RecordingMap("cfg", events, {"paths": paths}), values


def run_partial_ensure(function, root):
    root.mkdir()
    (root / "03-blocker").write_bytes(b"regular file")
    events = []
    cfg, values = directory_config(root, events)
    outcome = invoke(function, cfg, error_root=root)
    side_effects = {
        key: Path(path).is_dir()
        for key, path in values.items()
    }
    return outcome, events, side_effects


before_registry = dict(_native._configured_roots)
with tempfile.TemporaryDirectory(prefix="llmbase-b16-config-") as temporary:
    root = Path(temporary)
    previous_cwd = Path.cwd()
    os.chdir(root)
    try:
        rich = root / "rich"
        rich.mkdir()
        # CRLF exercises the original text-mode universal-newline adaptation.
        (rich / "config.yaml").write_bytes(RICH_YAML.replace("\n", "\r\n").encode("utf-8"))
        compare_read(Path("rich"), "python_yaml")

        real = root / "real"
        real.mkdir()
        (real / "child").mkdir()
        (real / "config.yaml").write_text(
            "paths:\n"
            "  raw: raw\n"
            "  wiki: .\n"
            "  outputs: ../outputs\n"
            "  meta: missing/../state\n"
            "  concepts: concepts\n"
            "unknown: [kept, 9]\n",
            encoding="utf-8",
        )
        (root / "alias").symlink_to(real, target_is_directory=True)
        (root / "dangling").symlink_to(root / "absent-target", target_is_directory=True)
        (root / "loop-a").symlink_to("loop-b", target_is_directory=True)
        (root / "loop-b").symlink_to("loop-a", target_is_directory=True)

        path_cases = [
            ("relative", Path("real")),
            ("missing", Path("missing")),
            ("parent", Path("real/child/..")),
            ("missing-parent", Path("real/absent/..")),
            ("symlink", Path("alias")),
            ("dangling", Path("dangling")),
            ("loop", Path("loop-a")),
        ]
        for label, base in path_cases:
            try:
                exists = (base / "config.yaml").exists()
            except BaseException:
                exists = None
            expected = invoke(oracle._read_config, base)
            actual = invoke(config._read_config, base)
            assert actual == expected, (label, expected, actual)
            if actual["kind"] == "value":
                assert exists is not None
                assert_trace("config_read", parser="python_yaml" if exists else "defaults")
            else:
                assert _native.last_trace() is None

        old_partial = run_partial_ensure(oracle.ensure_dirs, root / "old-partial")
        new_partial = run_partial_ensure(config.ensure_dirs, root / "new-partial")
        assert new_partial == old_partial, (old_partial, new_partial)
        outcome, events, side_effects = new_partial
        assert outcome["kind"] == "error", outcome
        assert side_effects == {
            "raw": True, "wiki": True, "outputs": False,
            "meta": False, "concepts": False,
        }, side_effects
        assert events[-1] == ("paths", "outputs"), events
        assert _native.last_trace() is None

        success_root = root / "success"
        success_root.mkdir()
        success_cfg = {"paths": {
            key: str(success_root / key)
            for key in ("raw", "wiki", "outputs", "meta", "concepts")
        }}
        assert config.ensure_dirs(success_cfg) is None
        assert all(Path(path).is_dir() for path in success_cfg["paths"].values())
        assert_trace("config_ensure_dirs")
    finally:
        os.chdir(previous_cwd)

assert _native._configured_roots == before_registry
print(
    "B16 config acceptance passed: rich YAML graph/type parity; "
    "7 pathlib edge cases; ensure_dirs partial-error ordering and success trace; "
    "no config registration"
)
