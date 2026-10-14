#!/usr/bin/env python3
"""Read-only installed-wheel probe on a copied corpus; not full bake acceptance."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import time


ROUTES = ("/api/trails", "/api/taxonomy?lang=zh", "/api/xici?lang=zh")


def inventory(root):
    """Hash every complete Markdown/JSON file and the sanitized root config."""
    aggregate = hashlib.sha256()
    result = {"files": 0, "bytes": 0, "markdown_files": 0, "markdown_bytes": 0}
    for path in sorted(root.rglob("*")):
        if not path.is_file() or (path.suffix.lower() not in {".md", ".json"}
                                  and path != root / "config.yaml"):
            continue
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        size = path.stat().st_size
        entry = [path.relative_to(root).as_posix(), size, digest]
        aggregate.update(json.dumps(entry, ensure_ascii=False).encode("utf-8") + b"\n")
        result["files"] += 1
        result["bytes"] += size
        if path.suffix.lower() == ".md":
            result["markdown_files"] += 1
            result["markdown_bytes"] += size
    result["aggregate_sha256"] = aggregate.hexdigest()
    return result


def error_record(error):
    return {"type": type(error).__name__, "message": str(error)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--label", required=True)
    args = parser.parse_args()
    if not args.corpus.is_absolute() or not args.out.is_absolute():
        parser.error("--corpus and --out must be absolute paths")
    root, output = args.corpus.resolve(), args.out.resolve()
    if not root.is_dir() or output.is_relative_to(root):
        parser.error("corpus must exist and output must be outside it")

    sys.dont_write_bytecode = True
    for name in ("LLMBASE_API_SECRET", "LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "PORT"):
        os.environ.pop(name, None)
    os.environ["LLMBASE_REQUIRE_NATIVE"] = "1"
    import llmwiki
    from llmwiki import _core, _native, http_auth, web
    from llmwiki.config import load_config

    package, extension = Path(llmwiki.__file__).resolve(), Path(_core.__file__).resolve()
    prefix, backend = Path(sys.prefix).resolve(), _native.backend_info()
    if not all(path.is_relative_to(prefix) for path in (package, extension)):
        raise RuntimeError("llmwiki and its extension must come from the installed environment")
    if not backend.get("native") or backend.get("core") != "rust":
        raise RuntimeError("installed Rust backend required")
    report = {"label": args.label, "python": sys.version, "executable": sys.executable,
              "prefix": str(prefix), "llmwiki_module": str(package), "core_module": str(extension),
              "backend": backend, "corpus": str(root), "before": inventory(root),
              "snapshot": {}, "requests": [],
              "boundary": "One snapshot and three anonymous Flask GETs on a copy; no write, index or bake."}

    config = load_config(root)
    for key in ("raw", "wiki", "concepts", "outputs", "meta"):
        assert Path(config["paths"][key]).resolve().is_relative_to(root), key
    started = time.perf_counter()
    try:
        result = http_auth.visibility_snapshot(root, config)
        report["snapshot"].update(ok=True, has_nonpublic=result["has_nonpublic"],
                                  counts={kind: len(result[kind]) for kind in ("compiled", "raw", "note")})
    except Exception as error:
        report["snapshot"].update(ok=False, error=error_record(error))
    report["snapshot"]["elapsed_ms"] = (time.perf_counter() - started) * 1000

    try:
        client = web.create_web_app(root).test_client()
    except Exception as error:
        report["factory_error"] = error_record(error)
        client = None
    for route in ROUTES:
        row, started = {"route": route, "status": None}, time.perf_counter()
        if client is None:
            row["error"] = report["factory_error"]
        else:
            try:
                response = client.get(route)
                # Hash even JSON responses: no cached prose or corpus text is emitted.
                row.update(status=response.status_code, content_type=response.content_type,
                           bytes=len(response.data), body_sha256=hashlib.sha256(response.data).hexdigest())
            except Exception as error:
                row["error"] = error_record(error)
        row["elapsed_ms"] = (time.perf_counter() - started) * 1000
        report["requests"].append(row)
    report["after"] = inventory(root)
    report["source_unchanged"] = report["before"] == report["after"]
    output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(str(output), flush=True)
    if not report["source_unchanged"]:
        raise SystemExit("copied corpus content changed during read-only probe")


if __name__ == "__main__":
    main()
