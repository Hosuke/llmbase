"""Actual installed-extension pipeline contracts; no model/network required."""
import hashlib
import json
import math
import os
import subprocess
import sys
from pathlib import Path

import pytest

from llmwiki import _native
from llmwiki.chunk_cache import ChunkCache
from llmwiki.pipeline import log, state

pytestmark = pytest.mark.skipif(not _native.available(), reason="native candidate required")


def test_native_pipeline_missing_reads_are_side_effect_free(tmp_path):
    root = tmp_path / "never-created"
    assert list(log.iter_events(root, "s", "k")) == []
    assert state.rebuild_state(root, "s", "k").status == "pending"
    cache = ChunkCache(root)
    assert cache.get("c", "h") is None
    cache.clear("c")
    assert not root.exists()
    assert _native.last_trace()["operation"] == "compat_pipeline_cache_clear"


def test_native_append_timestamp_override_is_live_and_skips_torn_utf8(tmp_path, monkeypatch):
    monkeypatch.setattr(log, "_now_iso", lambda: "first")
    log.append(tmp_path, "s", "法華", {"event": "start", "ts": "forged"})
    assert _native.last_trace()["core"] == "rust"
    path = log.log_path(tmp_path, "s", "法華")
    with path.open("ab") as stream:
        stream.write(b'\n[]\n{"event":"broken"\xe4\xb8')
    monkeypatch.setattr(log, "_now_iso", lambda: "second")
    log.append(tmp_path, "s", "法華", {"event": "ok"})
    assert [e["ts"] for e in log.iter_events(tmp_path, "s", "法華")] == ["first", "second"]
    assert state.rebuild_state(tmp_path, "s", "法華").finished_at == "second"
    assert b'\xe4\xb8\n' in path.read_bytes()


def test_native_state_and_cache_do_not_use_python_file_readers(tmp_path, monkeypatch):
    log.append(tmp_path, "s", "k", {"event": "start", "meta_init": {"x": 1}})
    log.append(tmp_path, "s", "k", {"event": "meta_update", "x": None})
    cache = ChunkCache(tmp_path)
    cache.put("slot", "first", "a\r\nb\r")

    def forbidden(*args, **kwargs):
        raise AssertionError("Python filesystem reader used")

    monkeypatch.setattr(Path, "read_text", forbidden)
    monkeypatch.setattr(Path, "read_bytes", forbidden)
    assert state.rebuild_state(tmp_path, "s", "k").meta == {"x": None}
    assert _native.last_trace()["operation"] == "compat_pipeline_state"
    assert cache.get("slot", "first") == "a\nb\n"
    assert cache.get("slot", "second") is None


def test_native_cache_invalid_utf8_is_error_and_full_hash_is_unchanged(tmp_path):
    cache = ChunkCache(tmp_path, subdir="chunks")
    cache.put("slot", "hash", "ok")
    path = cache.root / hashlib.sha256(b"slot").hexdigest() / (hashlib.sha256(b"hash").hexdigest() + ".md")
    path.write_bytes(b"a\xff")
    with pytest.raises(UnicodeDecodeError) as raised:
        cache.get("slot", "hash")
    assert raised.value.object == b"a\xff"
    assert raised.value.start == 1


def test_iter_events_override_is_used_by_state_and_tail(tmp_path, monkeypatch):
    records = [{"event": "start", "ts": "live"}, {"event": "failed", "err": {"code": 7}}]
    calls = []

    def custom(*args):
        calls.append(args)
        return iter(records)

    monkeypatch.setattr(log, "iter_events", custom)
    assert state.rebuild_state(tmp_path, "s", "k").last_err == {"code": 7}
    assert log.tail(tmp_path, "s", "k", 1) == records[-1:]
    assert len(calls) == 2
    assert _native.last_trace() is None


def test_custom_cache_writer_executes_live_and_clears_receipt(tmp_path, monkeypatch):
    import llmwiki.chunk_cache as module
    cache = ChunkCache(tmp_path)
    cache.put("c", "h", "native")
    calls = []
    monkeypatch.setattr(module, "atomic_write_text", lambda path, content: calls.append((path, content)))
    cache.put("c", "h", "callback")
    assert calls == [(cache._key_path("c", "h"), "callback")]
    assert _native.last_trace() is None
    assert cache.get("c", "h") == "native"


def test_native_big_integer_and_ordered_metadata_round_trip(tmp_path):
    log.append(tmp_path, "s", "k", {"event": "start"})
    log.append(tmp_path, "s", "k", {"event": "partial", "reason": {"z": 2**80, "a": True}})
    assert list(log.iter_events(tmp_path, "s", "k"))[1]["reason"]["z"] == 2**80
    assert state.rebuild_state(tmp_path, "s", "k").last_err == f"partial: {{'z': {2**80}, 'a': True}}"


def test_stage_kernel_fence_live_force_break_kill_and_pid_reuse(tmp_path):
    from llmwiki.pipeline.lock import StageLock
    code = """
import sys
from llmwiki.pipeline.lock import StageLock
from llmwiki.pipeline import log
lock = StageLock(sys.argv[1], 's', 'k')
assert lock.acquire()
log.append(sys.argv[1], 's', 'k', {'event':'start'})
print('held', flush=True)
sys.stdin.read()
"""
    child = subprocess.Popen([sys.executable, "-c", code, str(tmp_path)],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        assert child.stdout.readline().strip() == "held"
        second = StageLock(tmp_path, "s", "k")
        assert second.acquire() is False
        second.force_break()
        assert second.acquire() is False  # PID file deletion does not release the lease.
        # Preserve a stale native marker whose PID is demonstrably live/reused.
        second.path.write_text(json.dumps({"pid": os.getpid(), "host": "local",
            "started_at": "old", "lock_protocol": "kernel-v1"}), encoding="utf-8")
        child.kill()
        child.wait(timeout=5)
        assert second.acquire() is True
        try:
            records = list(log.iter_events(tmp_path, "s", "k"))
            assert records[-1]["event"] == "interrupted"
            assert records[-1]["prior_pid"] == os.getpid()
            parallel = StageLock(tmp_path, "s", "other-key")
            assert parallel.acquire()
            parallel.release()
        finally:
            second.release()
    finally:
        if child.poll() is None:
            child.kill()
        child.wait(timeout=5)


def test_stage_failed_acquire_releases_kernel_lease(tmp_path, monkeypatch):
    from llmwiki.pipeline.lock import StageLock
    first = StageLock(tmp_path, "s", "k")
    def broken(ttl):
        raise OSError("simulated pidfile publication error")
    monkeypatch.setattr(first, "_acquire_pidfile", broken)
    with pytest.raises(OSError):
        first.acquire()
    second = StageLock(tmp_path, "s", "k")
    assert second.acquire()
    second.release()


def test_existing_extended_json_log_round_trip_preserves_plain_storage(tmp_path, monkeypatch):
    path = log.log_path(tmp_path, "s", "k")
    path.parent.mkdir(parents=True)
    original = b'{"event":"start","meta_init":{"value":NaN,"positive":Infinity,"negative":-Infinity,"literal":"NaN Infinity -Infinity"}}\n'
    path.write_bytes(original)
    rebuilt = state.rebuild_state(tmp_path, "s", "k")
    assert math.isnan(rebuilt.meta["value"])
    assert rebuilt.meta["positive"] == float("inf")
    assert rebuilt.meta["negative"] == float("-inf")
    assert rebuilt.meta["literal"] == "NaN Infinity -Infinity"
    assert path.read_bytes() == original
    monkeypatch.setattr(log, "_now_iso", lambda: "native-tail")
    log.append(tmp_path, "s", "k", {"event": "failed", "err": float("nan"), "values": [float("inf"), float("-inf")]})
    records = list(log.iter_events(tmp_path, "s", "k"))
    assert len(records) == 2 and math.isnan(records[1]["err"])
    assert log.tail(tmp_path, "s", "k", 1)[0]["values"] == [float("inf"), float("-inf")]
    assert math.isnan(state.rebuild_state(tmp_path, "s", "k").last_err)
    written = path.read_bytes()
    assert written.startswith(original)
    # No internal marker or new on-disk format: the stdlib reads the actual line.
    native_line = written[len(original):].decode().strip()
    assert native_line == '{"event":"failed","err":NaN,"values":[Infinity,-Infinity],"ts":"native-tail"}'
    assert math.isnan(json.loads(native_line)["err"])


def test_nonfinite_state_summaries_callback_events_and_cache_strings(tmp_path, monkeypatch):
    for value, expected in [(float("nan"), "nan"), (float("inf"), "inf"), (float("-inf"), "-inf")]:
        log.append(tmp_path, "s", expected, {"event": "start", "meta_init": {"n": value}})
        log.append(tmp_path, "s", expected, {"event": "partial", "reason": {"number": value}})
        assert state.rebuild_state(tmp_path, "s", expected).last_err == "partial: {'number': " + expected + "}"
    monkeypatch.setattr(log, "iter_events", lambda *args: iter([
        {"event": "start", "ts": float("nan")}, {"event": "interrupted", "by": float("-inf")}]))
    rebuilt = state.rebuild_state(tmp_path, "custom", "k")
    assert math.isnan(rebuilt.started_at)
    assert rebuilt.last_err == "interrupted: -inf"
    cache = ChunkCache(tmp_path)
    text = '{"NaN":"Infinity","number":NaN,"marker":"$serde_json::private::Number"}\\"𠮷'
    cache.put("NaN", "Infinity", text)
    assert cache.get("NaN", "Infinity") == text
    assert cache._key_path("NaN", "Infinity").read_text() == text
