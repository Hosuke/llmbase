#!/usr/bin/env python3
"""Evaluate real legacy/native lexical retrieval on a fixed corpus and record performance."""

from __future__ import annotations

import argparse
import hashlib
import importlib
import json
import math
import os
from pathlib import Path
import platform
import resource
import signal
import statistics
import subprocess
import sys
import tempfile
import threading
import time


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FIXTURE = ROOT / "tests/rust_migration/fixtures/retrieval"


def measured_command(command: list[str], timeout: float = 30) -> tuple[str, dict]:
    """Measure one actual child, without mixing earlier child RSS high-water marks.

    File-backed stdout avoids pipe deadlock while wait4 obtains this child's usage.
    This is POSIX-only, matching the supported macOS/Linux evaluation runners.
    The timer is a timeout guard, not a sampling profiler.
    """
    if not hasattr(os, "wait4"):
        raise RuntimeError("per-child resource measurements require POSIX wait4")
    timed_out = threading.Event()
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        started = time.perf_counter_ns()
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)

        def terminate() -> None:
            timed_out.set()
            try:
                os.kill(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(timeout, terminate)
        timer.daemon = True
        timer.start()
        try:
            _, status, usage = os.wait4(process.pid, 0)
            elapsed = (time.perf_counter_ns() - started) / 1e6
            process.returncode = os.waitstatus_to_exitcode(status)
        finally:
            timer.cancel()
            timer.join()
        stdout.seek(0)
        stderr.seek(0)
        output = stdout.read().decode("utf-8")
        error = stderr.read().decode("utf-8", errors="replace")
    if timed_out.is_set():
        raise TimeoutError(f"native command exceeded {timeout:g} seconds")
    if process.returncode:
        raise RuntimeError(f"native command exit {process.returncode}: {error[:1200]}")
    scale = 1 if sys.platform == "darwin" else 1024
    return output, {"wall_ms": elapsed, "user_cpu_ms": usage.ru_utime * 1000,
                    "system_cpu_ms": usage.ru_stime * 1000,
                    "peak_rss_bytes": usage.ru_maxrss * scale,
                    "major_page_faults": usage.ru_majflt,
                    "minor_page_faults": usage.ru_minflt}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_fixture(fixture: Path) -> tuple[dict, dict, list[dict]]:
    manifest = json.loads((fixture / "manifest.json").read_text(encoding="utf-8"))
    for name, expected in manifest["files"].items():
        if sha256(fixture / name) != expected:
            raise ValueError(f"frozen fixture hash mismatch: {name}")
    corpus = json.loads((fixture / "corpus.json").read_text(encoding="utf-8"))
    queries = json.loads((fixture / "queries.json").read_text(encoding="utf-8"))["queries"]
    documents = corpus["documents"]
    ids = {d["id"] for d in documents}
    if len(ids) != manifest["document_count"] or len(documents) != len(ids):
        raise ValueError("duplicate or missing document IDs")
    if len({q["id"] for q in queries}) != len(queries):
        raise ValueError("duplicate query IDs")
    for query in queries:
        if not query["relevant"] or not set(query["relevant"]) <= ids:
            raise ValueError(f"invalid relevance: {query['id']}")
        if query["split"] not in {"held_out", "tuning"}:
            raise ValueError("invalid split")
    for split in ("held_out", "tuning"):
        if sum(q["split"] == split for q in queries) != manifest[f"{split}_query_count"]:
            raise ValueError(f"changed split size: {split}")
    return manifest, corpus, queries


def materialize(corpus: dict, base: Path) -> dict[tuple[str, str], Path]:
    """Materialize documents mechanically; never change corpus text or relevance."""
    import yaml

    paths = {"raw": "raw", "wiki": "wiki", "concepts": "wiki/concepts",
             "outputs": "wiki/outputs", "meta": "wiki/_meta"}
    for relative in paths.values():
        (base / relative).mkdir(parents=True, exist_ok=True)
    (base / "config.yaml").write_text(yaml.safe_dump({
        "paths": paths, "llm": {"max_tokens": 512}, "entities": {"enabled": False},
        "compile": {"backlinks": True}, "worker": {"enabled": False},
    }, allow_unicode=True, sort_keys=True), encoding="utf-8")
    result = {}
    for document in corpus["documents"]:
        for kind in ("raw", "compiled"):
            path = (base / "raw" / document["id"] / "index.md" if kind == "raw"
                    else base / "wiki/concepts" / f"{document['id']}.md")
            path.parent.mkdir(parents=True, exist_ok=True)
            metadata = {key: document[key] for key in ("title", "aliases", "chapter", "tags", "summary")}
            metadata.update({"source": f"https://example.invalid/retrieval/{document['id']}",
                             "type": "fixture", "work_id": document["id"],
                             "evaluation_id": document["id"],
                             "unknown_extension": {"preserve": True, "language": document["language"]},
                             "created": "2026-09-20T00:00:00+00:00"})
            path.write_text("---\n" + yaml.safe_dump(metadata, allow_unicode=True, sort_keys=True)
                            + "---\n" + document[kind] + "\n", encoding="utf-8")
            result[(kind, document["id"])] = path
    return result


class Legacy:
    def __init__(self, base: Path, source: Path, manifest: dict):
        source = source.resolve()
        hashes = {name: sha256(source / name) for name in manifest["legacy_module_sha256"]}
        if hashes != manifest["legacy_module_sha256"]:
            raise ValueError("legacy source differs from frozen baseline; choose the original checkout")
        sys.path.insert(0, str(source))
        self.module = importlib.import_module("llmwiki.search")
        if Path(self.module.__file__).resolve() != source / "llmwiki/search.py":
            raise ValueError("legacy import came from the wrong package")
        if self.module.SEARCH_TOKENIZER is not None:
            raise ValueError("legacy tokenizer is patched")
        self.base = base
        self.identity = {"backend": "legacy", "module": str(self.module.__file__),
                         "source_sha256": hashes, "interpreter": sys.executable,
                         "python": sys.version, "index_build_ms": None,
                         "citation_resolution": "unsupported; legacy results have no evidence objects"}

    def search(self, text: str, kind: str) -> list[dict]:
        method = self.module.search_raw if kind == "raw" else self.module.search
        hits = method(text, top_k=10, base_dir=self.base)
        for hit in hits:
            hit["evaluation_id"] = hit["source"] if kind == "raw" else hit["slug"]
            if "path" in hit:
                hit["path"] = str(Path(hit["path"]).relative_to(self.base))
        return hits


class Native:
    def __init__(self, base: Path, binary: Path, paths: dict, library_id: str):
        self.binary, self.base, self.library_id = binary.resolve(), base, library_id
        self.paths = paths
        self.resolution_cache = {}
        self.measurements = []
        self.identity = {"backend": "native", "binary": str(self.binary),
                         "binary_sha256": sha256(self.binary),
                         "binary_bytes": self.binary.stat().st_size,
                         "command_contract": "--base-dir BASE --library-id ID --json op NAME --args JSON"}
        start = time.perf_counter_ns()
        self.identity["index_report"] = self.invoke("index", {})
        self.identity["index_build_ms"] = (time.perf_counter_ns() - start) / 1e6
        sources = self.invoke("list", {})["sources"]
        self.source_ids = {}
        for source in sources:
            relative = Path(source["rel_path"])
            identity = relative.parent.name if source["kind"] == "raw" else relative.stem
            if (source["kind"], identity) not in paths:
                raise ValueError(f"native source cannot map to fixture: {relative}")
            self.source_ids[source["source_id"]] = (source["kind"], identity)
        if len(sources) != len(paths):
            raise ValueError("native index omitted or duplicated fixture sources")

    def invoke(self, operation: str, arguments: dict) -> dict:
        output, measurement = measured_command([str(self.binary), "--base-dir", str(self.base),
                                 "--library-id", self.library_id, "--json", "op", operation,
                                 "--args", json.dumps(arguments, ensure_ascii=False)])
        self.measurements.append({"operation": operation, **measurement})
        return json.loads(output)

    def search(self, text: str, kind: str) -> list[dict]:
        result = self.invoke("search", {"query": text, "kind": kind, "top_k": 10})
        if result["library_id"] != self.library_id:
            raise ValueError("native returned another library")
        if result["mode"] != "lexical":
            raise ValueError("lexical baseline cannot include an unreported hybrid backend")
        hits = result["results"]
        for hit in hits:
            source_kind, identity = self.source_ids[hit["source_id"]]
            if source_kind != kind or hit["source_kind"] != kind:
                raise ValueError("native kind scope mismatch")
            hit["evaluation_id"] = identity
        return hits

    def validate_evidence(self, hit: dict, kind: str) -> dict:
        evidence = hit["evidence"]
        key = json.dumps(evidence, ensure_ascii=False, sort_keys=True)
        if key in self.resolution_cache:
            return self.resolution_cache[key]
        path = self.paths[(kind, hit["evaluation_id"])]
        original = path.read_bytes()
        if evidence["library_id"] != self.library_id or evidence["source_id"] != hit["source_id"]:
            raise ValueError("citation has wrong library/source")
        if evidence["content_hash"] != hashlib.sha256(original).hexdigest():
            raise ValueError("citation has wrong revision content hash")
        if evidence["offset_unit"] != "utf8_byte":
            raise ValueError("unsupported native offset unit")
        start, end = evidence["start"], evidence["end"]
        if not 0 <= start < end <= len(original):
            raise ValueError("citation span outside original")
        if original[start:end].decode("utf-8") != evidence["exact_quote"]:
            raise ValueError("citation byte slice does not match exact_quote")
        resolved = self.invoke("evidence", {"evidence": evidence})
        if resolved["stale"] or resolved["deleted"] or resolved["evidence"] != evidence:
            raise ValueError("citation did not resolve to its current fixture revision")
        answer = {"resolved": True, "passage_id": evidence["passage_id"], "offset_unit": evidence["offset_unit"]}
        self.resolution_cache[key] = answer
        return answer


def percentile(values: list[float], fraction: float) -> float:
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def distribution(values: list[float]) -> dict:
    return {"samples": len(values), "min": min(values), "p50": statistics.median(values),
            "p95": percentile(values, .95), "max": max(values)}


def resource_summary(measurements: list[dict]) -> dict:
    return {key: distribution([row[key] for row in measurements]) for key in
            ("wall_ms", "user_cpu_ms", "system_cpu_ms", "peak_rss_bytes",
             "major_page_faults", "minor_page_faults")}


def index_sizes(base: Path) -> dict:
    files = {str(path.relative_to(base)): path.stat().st_size
             for path in (base / "wiki/_meta").rglob("*") if path.is_file()}
    return {"files": files, "total_bytes": sum(files.values()),
            "database_bytes": sum(size for name, size in files.items()
                                  if name.endswith(".sqlite3")),
            "wal_bytes": sum(size for name, size in files.items() if name.endswith("-wal")),
            "shm_bytes": sum(size for name, size in files.items() if name.endswith("-shm"))}


def check_index_report(report: dict, **expected: int) -> None:
    for field in ("added", "updated", "deleted", "moved", "unchanged"):
        if report[field] != expected.get(field, 0):
            raise ValueError(f"incremental {field}: expected {expected.get(field, 0)}, got {report}")


def native_performance(corpus: dict, binary: Path, repeats: int) -> dict:
    """Isolate mutable performance probes from the immutable quality corpus."""
    phases = {name: [] for name in ("startup_version", "fresh_index", "warm_noop_index",
                                   "incremental_add", "incremental_update", "incremental_delete")}
    iterations = []
    version = None
    count = len(corpus["documents"]) * 2
    for iteration in range(repeats):
        version, startup = measured_command([str(binary.resolve()), "--version"])
        phases["startup_version"].append(startup)
        with tempfile.TemporaryDirectory(prefix="llmbase-retrieval-performance-") as temporary:
            base = Path(temporary).resolve()
            paths = materialize(corpus, base)
            original_hashes = {str(path.relative_to(base)): sha256(path) for path in paths.values()}
            adapter = Native(base, binary, paths, corpus["library_id"])
            check_index_report(adapter.identity["index_report"], added=count)
            phases["fresh_index"].append(adapter.measurements[0])
            originals = {row["source_id"]: (row["content_hash"], row["revision"])
                         for row in adapter.invoke("list", {})["sources"]}
            record = {"iteration": iteration + 1, "fresh_index": {
                "resources": adapter.measurements[0], "report": adapter.identity["index_report"],
                "size": index_sizes(base)}}

            def measure_index(phase: str, **expected: int) -> None:
                before = index_sizes(base)
                report = adapter.invoke("index", {})
                measurement = adapter.measurements[-1]
                check_index_report(report, **expected)
                after = index_sizes(base)
                phases[phase].append(measurement)
                record[phase] = {"resources": measurement, "report": report,
                                 "size": after, "index_growth_bytes": after["total_bytes"] - before["total_bytes"]}
                active = {row["source_id"]: (row["content_hash"], row["revision"])
                          for row in adapter.invoke("list", {})["sources"]}
                if any(active.get(key) != value for key, value in originals.items()):
                    raise ValueError(f"{phase} changed an unrelated original source revision")

            measure_index("warm_noop_index", unchanged=count)
            probe = base / "raw/__performance_probe__/index.md"
            probe.parent.mkdir()
            probe.write_text("---\ntitle: Performance probe\n---\nqzvperfprobe English 原典𠮷。\n", encoding="utf-8")
            record["probe_added_bytes"] = probe.stat().st_size
            measure_index("incremental_add", added=1, unchanged=count)
            found = adapter.invoke("search", {"query": "qzvperfprobe", "kind": "raw", "top_k": 10})
            if len(found["results"]) != 1:
                raise ValueError("added performance source was not independently searchable")
            probe.write_text(probe.read_text(encoding="utf-8") + "Updated revision 原文。\n", encoding="utf-8")
            record["probe_updated_bytes"] = probe.stat().st_size
            measure_index("incremental_update", updated=1, unchanged=count)
            probe.unlink()  # Only this runtime-created synthetic file is removed.
            measure_index("incremental_delete", deleted=1, unchanged=count)
            if adapter.invoke("search", {"query": "qzvperfprobe", "kind": "raw", "top_k": 10})["results"]:
                raise ValueError("deleted performance source remained searchable")
            if original_hashes != {str(path.relative_to(base)): sha256(path) for path in paths.values()}:
                raise ValueError("performance benchmark changed an original fixture copy")
            record["original_sources_bytes_and_revisions_unchanged"] = True
            record["deleted_source_not_searchable"] = True
            iterations.append(record)
    return {"protocol": "fresh-index-and-cli-startup-v2", "repeats": repeats,
            "binary_version": version.strip(), "os_page_cache_evicted": False,
            "cold_definition": "fresh empty library index per iteration; recently materialized source files; OS cache uncontrolled",
            "warm_definition": "same existing index after its initial build; a fresh CLI process still launches for every operation",
            "startup_definition": "--version process launch and CLI argument parsing only; no library/database open",
            "incremental_definition": "one synthetic raw source added, edited, then deleted in each independent 60-source fixture copy",
            "rss_scope": "wait4 peak resident set of each native child, excluding Python harness; macOS bytes/Linux KiB normalized to bytes",
            "latency_scope": "native child launch through exit including storage scans and commit; excludes corpus materialization, JSON decoding, and post-operation validation",
            "p95_method": "nearest rank; small sample counts are descriptive, not a confidence interval",
            "model_calls": 0, "provider_request_elapsed_ms": 0,
            "model_service_latency_ms": None, "model_latency_status": "not measured: lexical operations do not call a provider",
            "summary": {phase: resource_summary(values) for phase, values in phases.items()},
            "iterations": iterations}


def summarize(rows: list[dict]) -> dict:
    if not rows:
        return {}
    warm = [value for row in rows for value in row["warm_ms"]]
    first = [row["first_pass_ms"] for row in rows]
    return {"query_count": len(rows),
            "recall_at_5": statistics.mean(row["recall_at_5"] for row in rows),
            "mrr_at_10": statistics.mean(row["reciprocal_rank_at_10"] for row in rows),
            "first_pass_p50_ms": statistics.median(first), "first_pass_p95_ms": percentile(first, .95),
            "warm_p50_ms": statistics.median(warm), "warm_p95_ms": percentile(warm, .95),
            "required_failures": [row["query_id"] for row in rows
                                  if row["required"] and row["recall_at_5"] < 1]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["legacy", "native"], required=True)
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--legacy-root", type=Path)
    parser.add_argument("--native-bin", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--performance-repeats", type=int, default=10,
                        help="independent native fresh-index and incremental repetitions (default: 10)")
    parser.add_argument("--artifact-label", default="build profile unspecified; binary hash is authoritative")
    parser.add_argument("--compare", type=Path, help="frozen same-fixture legacy JSON; regression or required-case failure exits 2")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    if not 1 <= args.performance_repeats <= 100:
        parser.error("--performance-repeats must be between 1 and 100")
    if args.output.exists():
        parser.error("output already exists; preserve the original result and choose a new name")
    if args.backend == "legacy" and not args.legacy_root:
        parser.error("--legacy-root required")
    if args.backend == "native" and not args.native_bin:
        parser.error("--native-bin required")
    manifest, corpus, queries = load_fixture(args.fixture)
    evaluator_hash = sha256(Path(__file__))
    rows = []
    evaluation_started = time.perf_counter_ns()
    with tempfile.TemporaryDirectory(prefix="llmbase-retrieval-") as temporary:
        base = Path(temporary).resolve()
        paths = materialize(corpus, base)
        original_hashes = {str(path.relative_to(base)): sha256(path) for path in paths.values()}
        setup = time.perf_counter_ns()
        adapter = (Legacy(base, args.legacy_root, manifest) if args.backend == "legacy"
                   else Native(base, args.native_bin, paths, corpus["library_id"]))
        setup_ms = (time.perf_counter_ns() - setup) / 1e6
        for kind in ("raw", "compiled"):
            for query in queries:
                start = time.perf_counter_ns()
                hits = adapter.search(query["text"], kind)
                elapsed = (time.perf_counter_ns() - start) / 1e6
                first_resources = adapter.measurements[-1] if isinstance(adapter, Native) else None
                warm = []
                warm_resources = []
                for _ in range(args.repeats):
                    start = time.perf_counter_ns()
                    repeated = adapter.search(query["text"], kind)
                    warm.append((time.perf_counter_ns() - start) / 1e6)
                    if isinstance(adapter, Native):
                        warm_resources.append(adapter.measurements[-1])
                    if [h["evaluation_id"] for h in repeated] != [h["evaluation_id"] for h in hits]:
                        raise ValueError("warm run changed deterministic ranking")
                ranked_ids = list(dict.fromkeys(hit["evaluation_id"] for hit in hits))
                relevant = set(query["relevant"])
                rank = next((i for i, identity in enumerate(ranked_ids[:10], 1) if identity in relevant), None)
                row = {"query_id": query["id"], "split": query["split"], "kind": kind,
                       "language": query["language"], "case": query["case"], "query": query["text"],
                       "required": query.get("required", False), "relevant": query["relevant"],
                       "ranked_document_ids": ranked_ids, "raw_hit_count": len(hits), "hits": hits,
                       "recall_at_5": len(relevant & set(ranked_ids[:5])) / len(relevant),
                       "reciprocal_rank_at_10": 1 / rank if rank else 0,
                       "first_pass_ms": elapsed, "warm_ms": warm,
                       "first_pass_native_resources": first_resources,
                       "warm_native_resources": warm_resources}
                if "quote" in query:
                    row["expected_quote"] = query["quote"]
                    row["expected_quote_present_in_relevant_source"] = all(
                        query["quote"] in paths[(kind, identity)].read_text(encoding="utf-8")
                        for identity in relevant)
                    if not row["expected_quote_present_in_relevant_source"]:
                        raise ValueError(f"invalid manually fixed quote: {query['id']} ({kind})")
                if isinstance(adapter, Native):
                    row["citation_resolution"] = [adapter.validate_evidence(hit, kind) for hit in hits]
                else:
                    row["citation_resolution"] = None
                rows.append(row)
        after_hashes = {str(path.relative_to(base)): sha256(path) for path in paths.values()}
        if original_hashes != after_hashes:
            raise ValueError("backend altered the frozen input files")
        index_files = {str(path.relative_to(base)): path.stat().st_size
                       for path in (base / "wiki/_meta").rglob("*") if path.is_file()}
        identity = adapter.identity
        citation_count = len(adapter.resolution_cache) if isinstance(adapter, Native) else None
        native_measurements = adapter.measurements if isinstance(adapter, Native) else []
    quality_wall_ms = (time.perf_counter_ns() - evaluation_started) / 1e6
    performance = (native_performance(corpus, args.native_bin, args.performance_repeats)
                   if isinstance(adapter, Native) else None)
    if isinstance(adapter, Native) and sha256(args.native_bin) != identity["binary_sha256"]:
        raise ValueError("native artifact changed during evaluation")
    if sha256(Path(__file__)) != evaluator_hash:
        raise ValueError("evaluator source changed during evaluation")
    summary = {kind: {split: summarize([r for r in rows if r["kind"] == kind and r["split"] == split])
                      for split in ("held_out", "tuning")} for kind in ("raw", "compiled")}
    failures = []
    if args.compare:
        baseline = json.loads(args.compare.read_text(encoding="utf-8"))
        if baseline["fixture_sha256"] != manifest["files"]:
            raise ValueError("comparison uses another fixture/relevance set")
        for kind in ("raw", "compiled"):
            actual, previous = summary[kind]["held_out"], baseline["summary"][kind]["held_out"]
            for metric in ("recall_at_5", "mrr_at_10"):
                if actual[metric] + 1e-12 < previous[metric]:
                    failures.append(f"{kind} {metric}: {actual[metric]} < {previous[metric]}")
            failures.extend(f"{kind} required case failed: {qid}" for qid in actual["required_failures"])
    rss_scale = 1 if sys.platform == "darwin" else 1024
    result = {"schema_version": 2, "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "fixture_sha256": manifest["files"], "manifest_sha256": sha256(args.fixture / "manifest.json"),
              "identity": identity, "artifact_label": args.artifact_label,
              "evaluator_sha256": evaluator_hash, "hostname": platform.node(),
              "platform": platform.platform(), "setup_ms": setup_ms,
              "rss_peak_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * rss_scale,
              "child_rss_peak_bytes": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * rss_scale,
              "latency_scope": "first-pass/warm OS cache, no forced cold-cache flush; legacy in-process, native includes fresh CLI launch per query; import/index measured separately",
              "model_calls": 0, "semantic_quality": "not evaluated", "index_files": index_files,
              "model_vs_local": {"quality_wall_ms": quality_wall_ms,
                                 "quality_native_child_wall_ms": sum(row["wall_ms"] for row in native_measurements),
                                 "local_search_wall_ms": sum(row["first_pass_ms"] + sum(row["warm_ms"]) for row in rows),
                                 "local_search_native_child_wall_ms": sum(row["wall_ms"] for row in native_measurements if row["operation"] == "search"),
                                 "provider_request_elapsed_ms": 0,
                                 "provider_requests": 0,
                                 "scope": "local_search excludes indexing/evidence validation; quality includes them and fixture setup; actual provider latency is not evaluated"},
              "native_operation_resources": {
                  operation: resource_summary([row for row in native_measurements if row["operation"] == operation])
                  for operation in sorted({row["operation"] for row in native_measurements})},
              "performance": performance,
              "index_bytes": sum(index_files.values()), "input_file_sha256": original_hashes,
              "inputs_unchanged": True, "unique_citations_resolved": citation_count,
              "summary": summary, "rows": rows, "comparison_failures": failures,
              "comparison_path": str(args.compare) if args.compare else None}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as output:
        output.write(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"output": str(args.output), "summary": summary, "comparison_failures": failures},
                     ensure_ascii=False, indent=2))
    return 2 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
