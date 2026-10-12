"""Finite synthetic installed-wheel benchmark; no Siwen import or real corpus.

Prepare once, then run both interpreters against the same immutable fixtures.
Timings are observations; structural scan rejection belongs to regression tests.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import sys
import time


TEXT = "---\ntitle: 公開樣本\nsummary: 公開短篇之驗\ntags: []\n---\n正文。\n"
COUNTS = (100, 1000, 10000)


def manifest(root):
    digest = hashlib.sha256()
    paths = sorted(root.rglob("*.md"))
    for path in paths:
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return {"files": len(paths), "sha256": digest.hexdigest()}


def prepare(root):
    if root.exists():
        raise SystemExit("fixture root must be new")
    for count in COUNTS:
        concepts = root / str(count) / "wiki/concepts"
        concepts.mkdir(parents=True)
        for index in range(count):
            (concepts / f"p{index}.md").write_text(TEXT, encoding="utf-8")
    print(json.dumps(manifest(root)))


def probe(root, output, label):
    from llmwiki import _native
    from llmwiki.web import create_web_app

    if sys.getprofile() is not None or any(name == "siwen" or name.startswith("siwen.") for name in sys.modules):
        raise SystemExit("benchmark must not load Siwen or a profiler")
    backend = _native.backend_info()
    if not backend.get("native") or backend.get("core") != "rust":
        raise SystemExit("installed Rust backend required")
    before = manifest(root)
    requests = []
    bakes = []
    for count in COUNTS:
        app = create_web_app(root / str(count))
        app.testing = True
        client = app.test_client()
        for route in ("/api/articles/p0", "/api/articles/p0/sections"):
            for repeat in range(3):
                start_dispatch = _native.backend_info()["successful_dispatches"]
                started = time.perf_counter()
                response = client.get(route)
                elapsed = time.perf_counter() - started
                if response.status_code != 200:
                    raise SystemExit(f"unexpected {response.status_code}: {route}")
                row = {"documents": count, "route": route, "repeat": repeat,
                       "seconds": elapsed, "status": response.status_code,
                       "bytes": len(response.data), "body_sha256": hashlib.sha256(response.data).hexdigest(),
                       "dispatch_delta": _native.backend_info()["successful_dispatches"] - start_dispatch}
                requests.append(row)
                print(json.dumps(row), flush=True)
        # Exhaust every detail+sections request at two bounded scales. This is
        # synthetic route enumeration, not acceptance of a real site's BakeRun.
        if count <= 1000:
            digest = hashlib.sha256()
            started = time.perf_counter()
            start_dispatch = _native.backend_info()["successful_dispatches"]
            for index in range(count):
                for suffix in ("", "/sections"):
                    response = client.get(f"/api/articles/p{index}{suffix}")
                    if response.status_code != 200:
                        raise SystemExit("synthetic bake request failed")
                    digest.update(response.data)
                    digest.update(b"\0")
            bakes.append({"documents": count, "requests": count * 2,
                          "seconds": time.perf_counter() - started,
                          "body_sha256": digest.hexdigest(),
                          "dispatch_delta": _native.backend_info()["successful_dispatches"] - start_dispatch})
            print(json.dumps({"synthetic_bake": bakes[-1]}), flush=True)
    after = manifest(root)
    if before != after:
        raise SystemExit("source fixtures changed during read-only benchmark")
    result = {"label": label, "python": sys.version, "backend": backend,
              "profile_enabled": False, "siwen_loaded": False,
              "before": before, "after": after, "requests": requests, "synthetic_bakes": bakes}
    output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")


def compare(baseline, candidate, output):
    old = json.loads(baseline.read_text())
    new = json.loads(candidate.read_text())
    if not (old["before"] == old["after"] == new["before"] == new["after"]):
        raise SystemExit("before/after source hashes differ")
    if len(old["requests"]) != 18 or len(new["requests"]) != 18:
        raise SystemExit("incomplete request matrix")
    for left, right in zip(old["requests"], new["requests"], strict=True):
        for key in ("documents", "route", "repeat", "status", "bytes", "body_sha256"):
            if left[key] != right[key]:
                raise SystemExit(f"response contract changed: {key}")
        if right["dispatch_delta"] != 0:
            raise SystemExit("candidate detail still dispatches a full native snapshot")
    rows = []
    for count in COUNTS:
        for route in ("/api/articles/p0", "/api/articles/p0/sections"):
            def median(data):
                return statistics.median(row["seconds"] for row in data["requests"]
                                         if row["documents"] == count and row["route"] == route)
            previous, current = median(old), median(new)
            rows.append({"documents": count, "route": route,
                         "baseline_median_ms": previous * 1000,
                         "candidate_median_ms": current * 1000,
                         "observed_speedup": previous / current})
    if len(old["synthetic_bakes"]) != 2 or len(new["synthetic_bakes"]) != 2:
        raise SystemExit("incomplete synthetic bake matrix")
    bakes = []
    for left, right in zip(old["synthetic_bakes"], new["synthetic_bakes"], strict=True):
        if any(left[key] != right[key] for key in ("documents", "requests", "body_sha256")):
            raise SystemExit("synthetic bake output mismatch")
        if right["dispatch_delta"] != 0:
            raise SystemExit("candidate synthetic bake still dispatches native snapshots")
        bakes.append({"documents": left["documents"], "requests": left["requests"],
                      "baseline_seconds": left["seconds"], "candidate_seconds": right["seconds"],
                      "observed_speedup": left["seconds"] / right["seconds"]})
    output.write_text(json.dumps({"source_unchanged": True, "response_bytes_equal": True,
                                 "requests": rows, "synthetic_bakes": bakes}, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("prepare", "probe", "compare"))
    parser.add_argument("--root", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--label", default="candidate")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--candidate", type=Path)
    args = parser.parse_args()
    if args.mode == "prepare":
        prepare(args.root)
    elif args.mode == "probe":
        probe(args.root, args.out, args.label)
    else:
        compare(args.baseline, args.candidate, args.out)
