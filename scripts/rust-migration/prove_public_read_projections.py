#!/usr/bin/env python3
"""Finite installed-wheel Siwen route proof; observed timings, not bake acceptance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import sys
import tempfile
import time


REAL_ROUTES = ("/api/articles/ben-lai", "/api/articles/ben-lai/sections")
MIXED_ROUTES = ("/api/entities", "/api/stats", "/api/export/article/public")
PUBLIC, PERSON, BODY, PRIVATE = "PUBLIC_C6_TITLE", "PUBLIC_C6_PERSON", "PUBLIC_C6_BODY", "PRIVATE_C6_SENTINEL"
DETAIL_ROUTES = ("/api/articles/public", "/api/articles/public-alias", "/api/articles/private",
                 "/api/sources/public", "/api/sources/private",
                 "/api/articles/nonpublic", "/api/sources/nonpublic")


def manifest(root):
    digest, count, size = hashlib.sha256(), 0, 0
    for path in sorted(root.rglob("*")):
        if path.is_file():
            with path.open("rb") as stream:
                file_hash = hashlib.file_digest(stream, "sha256").hexdigest()
            name = path.relative_to(root).as_posix()
            digest.update(json.dumps([name, file_hash], ensure_ascii=False).encode() + b"\n")
            count, size = count + 1, size + path.stat().st_size
    return {"files": count, "bytes": size, "sha256": digest.hexdigest()}


def fixture(base):
    roots = {key: base / value for key, value in {"raw": "raw", "wiki": "wiki", "concepts": "wiki/concepts",
             "outputs": "wiki/outputs", "meta": "wiki/_meta"}.items()}
    for path in roots.values():
        path.mkdir(parents=True, exist_ok=True)
    (base / "config.yaml").write_text(json.dumps({"paths": {k: str(v) for k, v in roots.items()},
                                                "entities": {"enabled": False}}))
    for slug, flag, marker in (("public", "private: false", PUBLIC), ("private", "private: true", PRIVATE),
                               ("nonpublic", "public: false", PRIVATE)):
        text = f"---\ntitle: {marker}\nsummary: {marker}\ntags: [proof]\n{flag}\n---\n## 文言\n\n{BODY if slug == 'public' else PRIVATE}\n"
        (roots["concepts"] / f"{slug}.md").write_text(text, encoding="utf-8")
        (roots["raw"] / slug).mkdir()
        (roots["raw"] / slug / "index.md").write_text(text, encoding="utf-8")
    metadata = {"aliases": {"public-alias": "public", "private-alias": "private"},
                "index": [{"slug": "public", "title": PUBLIC}, {"slug": "private", "title": PRIVATE}],
                "backlinks": {"public": ["private"]},
                "entities": {"people": [{"name": PERSON, "articles": ["public"]},
                                        {"name": PRIVATE, "articles": ["private"]}], "events": [], "places": []}}
    for name, value in metadata.items():
        (roots["meta"] / f"{name}.json").write_text(json.dumps(value))


def check_fixture(report):
    rows = {row["route"]: row for row in report["fixture_requests"]}
    assert set(rows) == set(MIXED_ROUTES + DETAIL_ROUTES), "incomplete fixture routes"
    for route, row in rows.items():
        text = json.dumps(row["data"], ensure_ascii=False)
        assert PRIVATE not in text, (route, "private sentinel leaked")
        if route in MIXED_ROUTES:
            assert row["status"] == (401 if report["label"] == "baseline" else 200), (route, row)
        else:
            allowed = route.endswith(("/public", "/public-alias"))
            assert row["status"] == (200 if allowed else 404), (route, row)
            if allowed:
                assert PUBLIC in text and BODY in text, (route, "missing public control")
    if report["label"] == "candidate":
        assert PERSON in json.dumps(rows["/api/entities"]["data"])
        assert rows["/api/stats"]["data"]["article_count"] == 1
        assert rows["/api/stats"]["data"]["raw_count"] == 1
        exported = json.dumps(rows["/api/export/article/public"]["data"])
        assert PUBLIC in exported and BODY in exported, "missing public export controls"
        assert len(report["live_privacy"]) == 2
        for row in report["live_privacy"]:
            assert row["status"] == 404 and PUBLIC not in json.dumps(row["data"])
            assert row["size_preserved"] and row["mtime_preserved"] and row["bytes_changed"]
            assert "no-store" in row["cache_control"]


def probe(args):
    assert sys.version_info[:2] == (3, 12), "use the remote Python 3.12 environment"
    assert sys.getprofile() is None and sys.gettrace() is None, "global profilers must be disabled"
    root, source, output = args.root.resolve(), args.siwen.resolve(), args.out.resolve()
    assert root.is_dir() and source.is_dir() and not output.is_relative_to(root)
    for name in ("LLMBASE_API_SECRET", "LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "PORT"):
        os.environ.pop(name, None)
    os.environ["LLMBASE_REQUIRE_NATIVE"] = "1"
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(source))
    import llmwiki
    from llmwiki import _core, _native, http_auth
    import siwen
    from siwen.web import create_siwen_web_app
    assert Path(llmwiki.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
    assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
    assert Path(siwen.__file__).resolve().is_relative_to(source)
    backend = _native.backend_info()
    assert backend.get("native") and backend.get("core") == "rust"
    before, calls = manifest(root), [0]
    monitor, code = sys.monitoring, http_auth.visibility_snapshot.__code__
    tool = next(i for i in range(6) if monitor.get_tool(i) is None)
    monitor.use_tool_id(tool, "public-projection-proof")
    def entered(started_code, offset):
        assert started_code is code
        calls[0] += 1
    monitor.register_callback(tool, monitor.events.PY_START, entered)
    monitor.set_local_events(tool, code, monitor.events.PY_START)

    def measure(client, route, *, headers=None, full=False):
        dispatches, scans = _native.backend_info()["successful_dispatches"], calls[0]
        started = time.perf_counter()
        response = client.get(route, headers=headers or {})
        row = {"route": route, "seconds": time.perf_counter() - started, "status": response.status_code,
               "bytes": len(response.data), "body_sha256": hashlib.sha256(response.data).hexdigest(),
               "snapshot_calls": calls[0] - scans,
               "dispatch_delta": _native.backend_info()["successful_dispatches"] - dispatches}
        if full:
            row.update(data=response.get_json(silent=True) if response.is_json else response.get_data(as_text=True),
                       cache_control=response.headers.get("Cache-Control", ""))
        print(json.dumps({k: v for k, v in row.items() if k != "data"}), flush=True)
        return row

    report = {"label": args.label, "python": sys.version, "llmwiki_module": llmwiki.__file__,
              "siwen_module": siwen.__file__, "backend": backend, "corpus_before": before,
              "monitoring": "code-local PY_START: llmwiki.http_auth.visibility_snapshot; no global events",
              "real_requests": [], "fixture_requests": [], "live_privacy": [],
              "boundary": "Finite Flask requests only; observed timings are not a full bake acceptance."}
    try:
        client = create_siwen_web_app(root).test_client()
        for route in REAL_ROUTES:
            for repeat in range(3):
                row = measure(client, route)
                assert row["status"] == 200, row
                report["real_requests"].append({**row, "repeat": repeat})
        with tempfile.TemporaryDirectory(prefix="c6-public-fixture-", dir=root.parent) as directory:
            base = Path(directory)
            assert not base.is_relative_to(root)
            fixture(base)
            client = create_siwen_web_app(base).test_client()
            report["fixture_requests"] = [measure(client, route, full=True) for route in MIXED_ROUTES + DETAIL_ROUTES]
            if args.label == "candidate":
                for path, route in ((base / "wiki/concepts/public.md", "/api/articles/public"),
                                    (base / "raw/public/index.md", "/api/sources/public")):
                    stat, original = path.stat(), path.read_bytes()
                    changed = original.replace(b"private: false", b"private: true ")
                    path.write_bytes(changed)
                    os.utime(path, ns=(stat.st_atime_ns, stat.st_mtime_ns))
                    row = measure(client, route, full=True, headers={"If-None-Match": '"previous-public"',
                                  "If-Modified-Since": "Sun, 11 Oct 2026 00:00:00 GMT"})
                    row.update(size_preserved=path.stat().st_size == stat.st_size,
                               mtime_preserved=path.stat().st_mtime_ns == stat.st_mtime_ns, bytes_changed=changed != original)
                    report["live_privacy"].append(row)
            check_fixture(report)
    finally:
        monitor.set_local_events(tool, code, 0)
        monitor.register_callback(tool, monitor.events.PY_START, None)
        monitor.free_tool_id(tool)
        report["corpus_after"] = manifest(root)
        output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    assert report["corpus_before"] == report["corpus_after"], "copied corpus files changed"


def compare(args):
    old, new = (json.loads(path.read_text()) for path in (args.baseline, args.candidate))
    assert old["label"] == "baseline" and new["label"] == "candidate"
    assert old["corpus_before"] == new["corpus_before"], "different copied corpora"
    for report in (old, new):
        assert report["corpus_before"] == report["corpus_after"], "copied corpus files changed"
        assert [(row["route"], row["repeat"]) for row in report["real_requests"]] == [
            (route, repeat) for route in REAL_ROUTES for repeat in range(3)]
        check_fixture(report)
    for left, right in zip(old["real_requests"], new["real_requests"], strict=True):
        assert all(left[key] == right[key] for key in ("route", "repeat", "status", "bytes", "body_sha256"))
        assert right["status"] == 200
        assert left["snapshot_calls"] == (0 if left["route"].endswith("/sections") else 1)
        assert left["dispatch_delta"] == left["snapshot_calls"]
    for row in new["real_requests"] + new["fixture_requests"] + new["live_privacy"]:
        assert row["snapshot_calls"] == row["dispatch_delta"] == 0, row
    timings = [{"route": route, **{label + "_median_ms": 1000 * statistics.median(
        row["seconds"] for row in report["real_requests"] if row["route"] == route)
        for label, report in (("baseline", old), ("candidate", new))}} for route in REAL_ROUTES]
    args.out.write_text(json.dumps({"accepted": True, "response_bytes_equal": True,
        "copied_corpora_unchanged": True, "public_controls_and_private_denials": True,
        "timings": timings, "boundary": "Observed finite requests; no full bake claim."}, indent=2) + "\n")


if __name__ == "__main__":
    if not __debug__:
        raise RuntimeError("proof requires assertions enabled")
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    p = modes.add_parser("probe")
    for flag in ("root", "siwen", "out"):
        p.add_argument("--" + flag, type=Path, required=True)
    p.add_argument("--label", choices=("baseline", "candidate"), required=True)
    p = modes.add_parser("compare")
    for flag in ("baseline", "candidate", "out"):
        p.add_argument("--" + flag, type=Path, required=True)
    arguments = parser.parse_args()
    (probe if arguments.mode == "probe" else compare)(arguments)
