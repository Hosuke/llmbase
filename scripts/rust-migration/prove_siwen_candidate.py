#!/usr/bin/env python3
"""Finite, offline Siwen candidate proof against a copied tree and synthetic KB.

Run with the isolated Siwen interpreter from the copied Siwen cwd. No source
checkout llmwiki, production data, model credentials, or HTTP listener is used.

Provenance inputs (at least one required):
- --baseline: JSON describing the ORIGINAL downstream checkout (legacy local
  mode; the original tree is hashed before and after the proof).
- --source-manifest: "sha256  relpath" manifest of the COPIED source tree
  (portable mode for remote hosts; the copy is verified before and after, and
  the receipt defers original-checkout preservation to a separate local
  receipt instead of claiming to hash an inaccessible original).
"""
import argparse
import hashlib
import importlib.metadata
import json
import runpy
import socket
import subprocess
import sys
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--snapshot", type=Path, required=True)
    parser.add_argument("--run-root", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--source-manifest", type=Path)
    parser.add_argument("--wheel", type=Path, required=True)
    args = parser.parse_args()
    if not args.baseline and not args.source_manifest:
        raise SystemExit("provenance input required: --baseline (original checkout) or --source-manifest (copied source)")
    if args.run_root.exists():
        raise SystemExit("proof run root must be new")
    args.run_root.mkdir(parents=True)
    provenance = {"mode": None}
    if args.baseline:
        baseline = json.loads(args.baseline.read_text())
        source = Path(baseline["source"])
        before = {name: digest(source / name) for name in baseline["files"]}
        assert before == baseline["files"], "original downstream changed since snapshot"
        provenance.update(mode="local-original", baseline=baseline, source=source, before=before)
    elif args.source_manifest:
        entries = {}
        for line in args.source_manifest.read_text().splitlines():
            if not line.strip():
                continue
            checksum, name = line.split("  ", 1)
            entries[name[2:] if name.startswith("./") else name] = checksum
        def verify_copy():
            found = {str(p.relative_to(args.snapshot)) for p in args.snapshot.rglob("*")
                     if p.is_file() and "__pycache__" not in p.parts and not p.name.endswith(".pyc")
                     and ".pytest_cache" not in p.parts and "build" not in p.parts
                     and not any(part.endswith(".egg-info") for part in p.parts)}
            assert found == set(entries), "copied source tree file set changed"
            bad = [name for name in sorted(found) if digest(args.snapshot / name) != entries[name]]
            assert not bad, f"copied source files changed: {bad[:3]}"
        verify_copy()
        provenance.update(mode="copied-source-manifest", manifest=str(args.source_manifest),
                          entries=len(entries), verify_copy=verify_copy)
    # The only network-shaped behavior in this proof is Flask's in-process client.
    def offline(*unused, **kwargs):
        raise AssertionError("external network attempted by offline fixture")
    socket.socket.connect = offline

    import llmwiki
    from llmwiki import _core, _native, compile as compiler, query, taxonomy
    assert not (args.snapshot / "llmwiki").exists()
    sys.path.insert(0, str(args.snapshot))
    import siwen
    from siwen import patches
    from siwen.kb import SiwenKB
    from siwen.web import SIWEN_STATIC_DIR, create_siwen_web_app
    from siwen.bake import (BakeRun, FIXED_JSON_ENDPOINTS, QUERY_JSON_ENDPOINTS,
        _copy_static_assets, _verify_clean, bake_json_tree, bake_html_tree)
    from llmwiki.config import load_config
    import frontmatter

    prefix = Path(sys.prefix).resolve()
    assert Path(llmwiki.__file__).resolve().is_relative_to(prefix)
    assert Path(_core.__file__).resolve().is_relative_to(prefix)
    assert _native.backend_info()["core"] == "rust"
    fixture = runpy.run_path(str(args.snapshot / "tests/conftest.py"))["tmp_kb"].__wrapped__
    base = fixture(args.run_root / "library")
    raw = base / "raw/lunyu-chapter/index.md"
    raw.parent.mkdir(parents=True)
    raw.write_text(frontmatter.dumps(frontmatter.Post(
        "# 學而\n\n子曰：「學而時習之，不亦說乎？」𠮷\n",
        title="論語·學而", type="wikisource", plugin="wikisource", work="論語",
        chapter="第一章", url="https://example.test/lunyu/chapter-1",
        source_path="/private/synthetic/raw.xml", wenguan_internal="synthetic-internal")), encoding="utf-8")
    raw_hash = digest(raw)
    book = base / "books/hosuke-laozi/德-01-第一章論德.md"
    book.parent.mkdir(parents=True)
    book.write_text("---\nchapter_title: 第一章論德\nchapter_no: 1\nsection: 德經\n---\n## 原文\n上德不德，是以有德。\n## 對校\n上德不德。\n## 疏\n此為合成驗證文本。\n", encoding="utf-8")
    (base / "wiki/_meta/xici-zh.json").write_text(json.dumps({"text":"學者，覺也。", "lang":"zh", "themes":[], "article_count":4}, ensure_ascii=False))
    cfg = load_config(base)
    receipts = {}
    assert compiler.get_section_headers(cfg) == [("文言", "")]
    assert raw in compiler._find_uncompiled(Path(cfg["paths"]["raw"]))
    receipts["find_uncompiled"] = _native.last_trace()
    existing = compiler._list_existing_concepts(Path(cfg["paths"]["concepts"]))
    assert isinstance(existing, list) and len(existing) == 4
    receipts["list_existing_concepts"] = _native.last_trace()
    compiler.rebuild_index(base)
    receipts["rebuild_index"] = _native.last_trace()
    assert all(receipt and receipt["core"] == "rust" for receipt in receipts.values())

    callbacks = []
    old_generator = taxonomy.TAXONOMY_GENERATOR
    try:
        for marker in ("第一類", "第二類"):
            def generator(articles, active_cfg, marker=marker):
                callbacks.append({"marker":marker,"article_count":len(articles)})
                return [{"id":"fixture-category","name":{"zh":marker},"articles":[a["slug"] for a in articles],"children":[]}]
            taxonomy.TAXONOMY_GENERATOR = generator
            result = taxonomy.generate_taxonomy(base)
            assert result["generated"] is True
            assert result["categories"][0]["name"]["zh"] == marker
    finally:
        taxonomy.TAXONOMY_GENERATOR = old_generator
    assert [call["marker"] for call in callbacks] == ["第一類", "第二類"]

    model_callbacks = []
    old_chat, old_context, old_system = query.chat, query.chat_with_context, query.SYSTEM_PROMPT
    try:
        query.chat = lambda *a, **kw: "論語·學而\n史記·項羽本紀\n老子·道經\n楚辭·離騷"
        def answer(question, context, **kwargs):
            model_callbacks.append({"system":kwargs["system"],"context_count":len(context)})
            return "<think>synthetic hidden reasoning</think>學而時習之，溫故知新也。"
        query.chat_with_context = answer
        kb = SiwenKB(base)
        for marker in ("第一回文言命令", "第二回文言命令"):
            query.SYSTEM_PROMPT = marker
            _native.clear_trace()
            result = kb.ask("論語何以論學？", file_back=False, promote=False)
            assert result["status"] == "ok" and "<think>" not in result["answer"]
            assert result["consulted"] and marker in model_callbacks[-1]["system"]
            receipts["query_last"] = _native.last_trace()
    finally:
        query.chat, query.chat_with_context, query.SYSTEM_PROMPT = old_chat, old_context, old_system
    assert len(model_callbacks) == 2

    app = create_siwen_web_app(base)
    app.testing = True
    assert app.config["llmbase"]["base_dir"] == base
    assert "api_compile" in app.view_functions
    hook_calls = []
    @app.after_request
    def after(response):
        hook_calls.append(response.status_code)
        response.headers["X-Candidate-Fixture"] = "active"
        return response
    app.add_url_rule("/api/candidate-fixture", "candidate_fixture", lambda: {"extension":"live"})
    client = app.test_client()
    assert client.get("/api/candidate-fixture").json == {"extension":"live"}
    assert any(rule.rule == "/api/classics" for rule in app.url_map.iter_rules())
    required = list(FIXED_JSON_ENDPOINTS) + [f"{path}?{query}" for path,query in QUERY_JSON_ENDPOINTS]
    status = {}
    for path in required:
        _native.clear_trace()
        response = client.get(path)
        status[path] = response.status_code
        assert response.status_code == 200, (path,response.status_code)
        assert response.headers["X-Candidate-Fixture"] == "active"
        if path == "/api/sources":
            assert response.json["documents"]
            receipts["flask_sources"] = _native.last_trace()
    assert len(client.get("/api/articles").json["articles"]) == 4
    assert client.get("/api/classics").json["books"]
    assert client.get("/api/books/hosuke-laozi").json["chapter_count"] == 1
    assert client.get("/api/books/hosuke-laozi/德-01").json["原文"]
    assert SIWEN_STATIC_DIR.is_dir(), "copy existing Siwen public assets before the proof"
    out = args.run_root / "baked"
    _copy_static_assets(out)
    run = BakeRun(client, out)
    ids = bake_json_tree(run)
    bake_html_tree(run, "full", ids)
    _verify_clean(out)
    unexpected = [item for item in run.skipped if not item.endswith("(HTTP 404 optional)")]
    assert not unexpected, unexpected
    assert ids["articles"] and ids["classics"] and ids["book_chapters"]["hosuke-laozi"]
    assert run.html_count > 10 and run.json_count > len(required)
    assert digest(raw) == raw_hash
    if provenance["mode"] == "local-original":
        baseline, source = provenance["baseline"], provenance["source"]
        after = {name: digest(source / name) for name in baseline["files"]}
        assert after == provenance["before"]
        status_after = subprocess.check_output(["git","-C",str(source),"status","--short"],text=True)
        provenance_receipt = {"provenance_mode":"local-original",
            "original_downstream_files_unchanged":True,
            "original_status_unchanged":status_after==baseline["status_before"]}
    else:
        provenance["verify_copy"]()
        provenance_receipt = {"provenance_mode":"copied-source-manifest",
            "copied_source_manifest":provenance["manifest"],
            "copied_source_files":provenance["entries"],
            "copied_source_unchanged":True,
            "original_preservation":"remote host cannot access the original checkout; attested by a separate local preservation receipt"}
    receipt = {
        "llmwiki_version":importlib.metadata.version("llmwiki"), "siwen_version":importlib.metadata.version("siwen"),
        "llmwiki_module":llmwiki.__file__, "siwen_module":siwen.__file__, "extension":_core.__file__,
        "extension_sha256":digest(Path(_core.__file__)), "wheel_sha256":digest(args.wheel),
        "native_receipts":receipts, "taxonomy_callback_calls":callbacks,
        "query_callback_count":len(model_callbacks), "query_context_counts":[v["context_count"] for v in model_callbacks],
        "model_transport":"explicit offline Python callback", "external_network":"blocked",
        "required_endpoint_status":status, "flask_hook_calls":len(hook_calls),
        "bake":{"json_count":run.json_count,"html_count":run.html_count,"skipped":run.skipped,
                "article_count":len(ids["articles"]),"classic_count":len(ids["classics"]),
                "book_chapters":ids["book_chapters"],"verified_clean":True},
        "synthetic_raw_bytes_unchanged":True, **provenance_receipt,
        "boundary":"Siwen custom compiler, Flask rendering and custom taxonomy remain Python extensions; receipts prove only named native operations",
    }
    (args.run_root / "receipt.json").write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps({"result":"passed","receipt":str(args.run_root/"receipt.json"),"bake":receipt["bake"]},ensure_ascii=False))


if __name__ == "__main__":
    main()
