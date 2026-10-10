#!/usr/bin/env python3
"""Offline C2 diagnostic against an installed wheel and a frozen Siwen copy.

Exit 0 requires every checked contract to pass; 2 means observed contract
failures and 1 means a broken/incomplete probe. A completed diagnostic is not
necessarily an accepted site. Local native FFI is privileged, not public auth.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import secrets
import socket
import sys

from prove_dual_site_candidate import document_hashes, library_scope, require_identity


def check_response(response, *, public_marker=None, forbidden=(), detail=False):
    """Deny/error alone is insufficient for a working public positive control."""
    data = response.get_json(silent=True)
    body = json.dumps(data, ensure_ascii=False) if data is not None else response.get_data(as_text=True)
    leaks = [marker for marker in forbidden if marker in body]
    allowed = {401, 403, 404} if detail and public_marker is None else {200}
    positive = public_marker is None or public_marker in body
    return {"http_status": response.status_code, "leaked_markers": leaks,
            "positive_control": positive if public_marker is not None else None,
            "status": "passed" if response.status_code in allowed and positive and not leaks else "failed"}


def require_rejection(callback, exception, message):
    try:
        callback()
    except exception as exc:
        if message not in str(exc):
            raise AssertionError(f"unexpected denial: {exc}") from exc
        return {"type": type(exc).__name__, "message": str(exc)}
    raise AssertionError("invalid evidence was accepted")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def check_bake(files, controls, forbidden, validator):
    def decoded(text):
        try:
            return json.dumps(json.loads(text), ensure_ascii=False)
        except ValueError:
            return text
    normalized = {name: decoded(text) for name, text in files.items()}
    leaked = {name: [marker for marker in forbidden if marker in text or marker in name]
              for name, text in normalized.items() if any(marker in text or marker in name for marker in forbidden)}
    positives = {name: marker in normalized.get(name, "") for name, marker in controls.items()}
    return {"status": "passed" if positives and all(positives.values()) and not leaked and validator["accepted"] else "failed",
            "public_output_controls": positives, "leaked_files": leaked, "downstream_validator": validator}


def main():
    if not __debug__:
        raise RuntimeError("diagnostic requires assertions enabled; unset PYTHONOPTIMIZE")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", type=Path, required=True)
    parser.add_argument("--run-root", type=Path, required=True)
    args = parser.parse_args()
    args.snapshot = args.snapshot.resolve()
    args.run_root = args.run_root.resolve()
    args.run_root.mkdir(parents=True, exist_ok=False)
    report = {"schema_version": 1, "status": "incomplete", "sites": {},
              "boundary": "Sequential synthetic fixtures, frozen downstream code; no production or public evidence API acceptance."}

    def save():
        (args.run_root / "receipt.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")

    # Discard inherited provider/operator settings. The generated test credential
    # stays in process memory and is never written into fixtures or receipts.
    for key in list(os.environ):
        if key.endswith(("API_KEY", "API_SECRET")) or key.startswith("LLMBASE_") or key in {"LLM_MODEL", "LLM_BASE_URL", "OPENAI_BASE_URL"}:
            os.environ.pop(key, None)
    os.environ["LLMBASE_REQUIRE_NATIVE"] = "1"
    os.environ["LLMBASE_API_SECRET"] = secrets.token_hex(32)

    def offline(*unused, **kwargs):
        raise AssertionError("network attempted by offline diagnostic")
    socket.socket.connect = offline

    try:
        import frontmatter
        import llmwiki
        from llmwiki import _core, _native
        from llmwiki import compile as compiler
        from llmwiki.config import load_config
        assert Path(llmwiki.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
        assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
        assert _native.backend_info()["core"] == "rust"
        assert not (args.snapshot / "llmwiki").exists()
        sys.path.insert(0, str(args.snapshot))
        from siwen import patches  # noqa: F401
        from siwen.web import create_siwen_web_app
        from siwen.bake import BakeRun, _verify_clean, static_api_path
        fixture = runpy.run_path(str(args.snapshot / "tests/conftest.py"))["tmp_kb"].__wrapped__

        sites = []
        for name in ("siwen", "taixu"):
            with library_scope(name):
                base = fixture(args.run_root / name)
                markers = {kind: f"C2{name.upper()}{kind.upper()}SENTINEL" for kind in ("public", "private")}
                for kind, marker in markers.items():
                    metadata = {"title": marker, "summary": marker, "type": "wikisource",
                                "work": marker, "chapter": "第一章", "tags": ["驗"],
                                "private": kind == "private", "created": "2026-10-10", "updated": "2026-10-10"}
                    text = frontmatter.dumps(frontmatter.Post(
                        f"# {marker}\n學而時習之。𠀀 e\u0301。 {marker} original revision.\n", **metadata))
                    raw = base / f"raw/{kind}-proof/index.md"
                    raw.parent.mkdir(parents=True)
                    # Original evidence offsets include frontmatter and CRLF.
                    raw.write_bytes(text.replace("\n", "\r\n").encode("utf-8"))
                    (base / f"wiki/concepts/{kind}-proof.md").write_text(text, encoding="utf-8")
                cfg = load_config(base)
                def dispatch(op, payload=None, *, base=base, name=name, cfg=cfg):
                    return _native.dispatch(op, payload, base, library_id=name, paths=cfg["paths"])
                dispatch("index")
                compiler.rebuild_index(base)
                require_identity(dispatch("doctor")["library_id"], name)
                app = create_siwen_web_app(base)
                app.testing = True
                app.config["llmbase"]["allow_unauthenticated_local"] = False
                sites.append({"name": name, "base": base, "markers": markers, "dispatch": dispatch,
                              "client": app.test_client(), "before": document_hashes(base)})

        for site, other in ((sites[0], sites[1]), (sites[1], sites[0])):
            with library_scope(site["name"]):
                call, base = site["dispatch"], site["base"]
                pub, secret = site["markers"]["public"], site["markers"]["private"]
                forbidden = [secret, *other["markers"].values()]
                receipt = report["sites"][site["name"]] = {"http": {}, "evidence": {}, "bake": {}}
                evidence = receipt["evidence"]
                original = (base / "raw/public-proof/index.md").read_bytes()
                hits = call("search", {"query": pub, "kind": "raw"})["results"]
                assert len(hits) == 1, hits
                ev = hits[0]["evidence"]
                require_identity(ev["library_id"], site["name"])
                assert ev["offset_unit"] == "utf8_byte"
                assert type(ev["start"]) is int and type(ev["end"]) is int
                assert 0 <= ev["start"] < ev["end"] <= len(original)
                assert original[ev["start"]:ev["end"]].decode("utf-8") == ev["exact_quote"]
                assert "𠀀" in ev["exact_quote"] and "e\u0301" in ev["exact_quote"]
                assert sha(original) == ev["content_hash"]
                resolved = call("evidence", {"evidence": ev})
                assert resolved["evidence"] == ev and not resolved["stale"] and not resolved["deleted"]
                evidence.update(status="passed", principal="privileged local FFI", original=ev,
                                byte_span_exact=True, content_hash_exact=True)
                evidence["cross_library_denial"] = require_rejection(
                    lambda: other["dispatch"]("evidence", {"evidence": ev}), PermissionError, "library scope denied")
                forged = {**ev, "exact_quote": ev["exact_quote"] + " forged"}
                evidence["forged_quote_denial"] = require_rejection(
                    lambda: call("evidence", {"evidence": forged}), RuntimeError, "citation does not match original revision")

                # Public controls must return their actual content. Private detail
                # must be explicitly denied; an empty 200/500 is not accepted.
                paths = [("/api/articles", pub, False), ("/api/sources", pub, False),
                         ("/api/classics", pub, False),
                         ("/api/articles/public-proof", pub, False),
                         ("/api/sources/public-proof", pub, False),
                         ("/api/classics/public-proof", pub, False),
                         ("/classics/public-proof", pub, False),
                         ("/api/articles/private-proof", None, True),
                         ("/api/sources/private-proof", None, True),
                         ("/api/classics/private-proof", None, True),
                         ("/api/classics/private-proof/sections", None, True),
                         ("/classics/private-proof", None, True)]
                for path, positive, detail in paths:
                    receipt["http"][path] = check_response(site["client"].get(path),
                        public_marker=positive, forbidden=forbidden + ([] if detail else ["private-proof"]), detail=detail)

                # Exercise the real serializer with public and private routes;
                # use synthetic content markers in addition to its own validator.
                out = args.run_root / f"bake-{site['name']}"
                bake = BakeRun(site["client"], out)
                for path in ("/api/classics", "/api/classics/public-proof", "/api/classics/private-proof"):
                    bake.bake_json(path)
                for path in ("/classics/public-proof", "/classics/private-proof"):
                    bake.bake_html(path)
                files = {str(path.relative_to(out)): path.read_text() for path in out.rglob("*") if path.is_file()}
                try:
                    _verify_clean(out)
                    validator = {"accepted": True}
                except SystemExit as exc:
                    validator = {"accepted": False, "exit": exc.code}
                controls = {static_api_path("/api/classics", ""): pub,
                            static_api_path("/api/classics/public-proof", ""): pub,
                            "classics/public-proof/index.html": pub}
                receipt["bake"] = {**check_bake(files, controls, [*forbidden, "private-proof"], validator),
                                   "scope": "3 classic JSON requests + 2 classic HTML requests; historical assets",
                                   "json_count": bake.json_count, "html_count": bake.html_count,
                                   "skipped": bake.skipped}
                assert document_hashes(base) == site["before"], "read/bake mutated source fixtures"
                receipt["read_only_document_hashes"] = site["before"]

                # Deliberate fixture changes: old citation keeps its exact revision.
                raw = base / "raw/public-proof/index.md"
                updated = original.replace(b"original revision", b"second revision")
                assert updated != original
                raw.write_bytes(updated)
                call("index")
                stale = call("evidence", {"evidence": ev})
                assert stale["evidence"] == ev and stale["stale"] and stale["current_revision"] == ev["revision"] + 1
                old = call("get", {"source_id": ev["source_id"], "revision": ev["revision"]})
                assert old["content"].encode("utf-8") == original
                evidence["historical_revision_exact"] = True
                evidence["current_revision"] = stale["current_revision"]

                # Revoke both previously public documents without reindexing,
                # after the public route/cache has already been exercised.
                for path in (raw, base / "wiki/concepts/public-proof.md"):
                    text = path.read_bytes()
                    assert b"private: false" in text
                    path.write_bytes(text.replace(b"private: false", b"private: true", 1))
                receipt["revocation"] = {}
                for path in ("/api/articles/public-proof", "/api/sources/public-proof", "/api/classics/public-proof", "/classics/public-proof"):
                    receipt["revocation"][path] = check_response(site["client"].get(path),
                        forbidden=[pub, *forbidden], detail=True)
                after = document_hashes(base)
                changed = [name for name in site["before"] if site["before"][name] != after[name]]
                assert sorted(changed) == ["raw/public-proof/index.md", "wiki/concepts/public-proof.md"], changed
                receipt["intentional_fixture_changes"] = {name: {"before": site["before"][name], "after": after[name]} for name in changed}
                save()

        checks = [item for site in report["sites"].values()
                  for item in [*site["http"].values(), *site["revocation"].values(), site["bake"], site["evidence"]]]
        report["passed_checks"] = sum(item["status"] == "passed" for item in checks)
        report["failed_checks"] = sum(item["status"] == "failed" for item in checks)
        report["status"] = "passed" if not report["failed_checks"] else "contract_failures"
        return 0 if report["status"] == "passed" else 2
    except Exception as exc:
        report["error"] = {"type": type(exc).__name__, "message": str(exc)}
        raise
    finally:
        save()


if __name__ == "__main__":
    sys.exit(main())
