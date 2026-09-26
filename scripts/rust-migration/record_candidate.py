#!/usr/bin/env python3
"""Record installed native provenance plus actual pipeline/query execution.

Invoke using the candidate interpreter from outside the source package after a
passing full test log. The output is immutable and contains no model credentials.
"""
import argparse
import hashlib
import importlib
import importlib.metadata
import json
import math
from pathlib import Path
import re
import sys
import tempfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    for name in ("source", "wheel", "test-log", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), "receipt output must be new"
    assert args.test_log.with_suffix(".exit").read_text().strip() == "0"
    summary = re.search(r"(\d+) passed(?:, \d+ warnings?)? in ([\d.]+)s", args.test_log.read_text())
    assert summary, "a successful full pytest summary is required"

    import llmwiki
    from llmwiki import _core, _native, query
    from llmwiki.pipeline import log, state
    from llmwiki.pipeline.lock import StageLock
    from llmwiki.chunk_cache import ChunkCache
    prefix = Path(sys.prefix).resolve()
    assert Path(llmwiki.__file__).resolve().is_relative_to(prefix)
    assert Path(_core.__file__).resolve().is_relative_to(prefix)
    modules = {}
    names = ["_native", "search", "compile", "query", "llm", "agent_api", "ingest",
             "operations", "web", "http_auth", "process_adapter", "pipeline.log", "pipeline.state", "pipeline.lock", "chunk_cache"]
    for name in names:
        installed = Path(importlib.import_module("llmwiki." + name).__file__)
        source = args.source / "llmwiki" / Path(*name.split(".")).with_suffix(".py")
        modules[name] = {"installed":str(installed), "installed_sha256":sha(installed),
                         "source_sha256_at_receipt":sha(source)}
        assert sha(installed) == sha(source), name

    receipts = []
    with tempfile.TemporaryDirectory() as temporary:
        base = Path(temporary)
        log.append(base, "fixture", "slot", {"event":"start", "meta_init":{
            "number":2**80, "nan":float("nan"), "infinity":float("inf"), "negative":float("-inf")}})
        receipts.append(_native.last_trace())
        log.append(base, "fixture", "slot", {"event":"ok"})
        result = state.rebuild_state(base, "fixture", "slot")
        assert result.status == "ok" and result.meta["number"] == 2**80
        assert math.isnan(result.meta["nan"])
        assert result.meta["infinity"] == float("inf") and result.meta["negative"] == float("-inf")
        persisted = json.loads(log.log_path(base, "fixture", "slot").read_text().splitlines()[0])
        assert math.isnan(persisted["meta_init"]["nan"])
        receipts.append(_native.last_trace())
        cache = ChunkCache(base)
        cache.put("slot", "content-v1", "literal原文𠮷 NaN Infinity")
        receipts.append(_native.last_trace())
        assert cache.get("slot", "content-v1") == "literal原文𠮷 NaN Infinity"
        assert cache.get("slot", "content-v2") is None
        receipts.append(_native.last_trace())
        stage = StageLock(base, "fixture", "lease")
        assert stage.acquire()
        receipts.append(json.loads(stage._native_lease.receipt()))
        stage.release()
        assert not list(base.rglob("*.sqlite3"))

        concept = base / "wiki/concepts/alpha.md"
        concept.parent.mkdir(parents=True)
        concept.write_text("---\ntitle: Alpha\nsummary: alpha knowledge\n---\nAlpha knowledge from a synthetic document.")
        old_context = query.chat_with_context
        calls = []
        try:
            def answer(question, context, **kwargs):
                calls.append(len(context))
                return "Synthetic answer from the explicit callback."
            query.chat_with_context = answer
            assert query.query("alpha knowledge", base_dir=base, file_back=False) == "Synthetic answer from the explicit callback."
            receipt = _native.last_trace()
            assert receipt["orchestrator"] == "rust" and receipt["operation"] == "compat_query"
            receipts.append(receipt)
        finally:
            query.chat_with_context = old_context
        assert calls and all(count > 0 for count in calls)

    dist = importlib.metadata.distribution("llmwiki")
    extensions = [str(path) for path in dist.files if str(path).startswith("llmwiki/_core.")]
    assert len(extensions) == 1
    result = {"version":dist.version, "python":sys.version, "prefix":str(prefix),
        "package":llmwiki.__file__, "wheel":str(args.wheel), "wheel_sha256":sha(args.wheel),
        "extension":_core.__file__, "extension_sha256":sha(Path(_core.__file__)),
        "extension_record_entries":extensions, "backend":_native.backend_info(), "modules":modules,
        "actual_native_receipts":receipts, "pipeline_creates_no_database":True,
        "extended_json_plain_storage_roundtrip":True,
        "query_model_boundary":"explicit offline Python callback; query sequencing/storage Rust",
        "full_test_result":{"passed":int(summary[1]), "failed":0, "seconds":float(summary[2]),
                            "log":str(args.test_log), "log_sha256":sha(args.test_log)}}
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"receipt":str(args.output), "wheel_sha256":result["wheel_sha256"],
                      "extension_sha256":result["extension_sha256"], "tests":result["full_test_result"]}))


if __name__ == "__main__":
    main()
