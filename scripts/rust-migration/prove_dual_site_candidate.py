#!/usr/bin/env python3
"""Finite, offline dual-site (斯文 / 太虛間) candidate proof.

Two synthetic libraries share one installed candidate wheel. The proof
alternates native and Flask operations between both Siwen app instances and
asserts content, identity, branding, links, atomic journals and baked outputs
belong only to the selected library. No network, credentials, production data
or HTTP listener. Run with the isolated candidate interpreter.
"""
import argparse
import hashlib
import importlib.metadata
import json
import runpy
import socket
import sqlite3
import sys
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


SITES = [
    {"key": "siwen", "library_id": "siwen", "branding": "斯文", "tagline": "以古法治今器",
     "origin": "https://siwen.example.test", "slug": "gong-mu",
     "title": "共目·斯文庫", "sentinel": "斯文本密語𠮷",
     "extra": ("lunyu-weizheng", "論語·為政", "子曰：「為政以德，譬如北辰。」")},
    {"key": "taixu", "library_id": "taixu", "branding": "太虛間", "tagline": "虛以納八宗，間以安一心。",
     "origin": "https://taixu.example.test", "slug": "gong-mu",
     "title": "共目·太虛庫", "sentinel": "太虛本密語𠀀",
     "extra": ("jingang-jing", "金剛經·般若", "一切有為法，如夢幻泡影。")},
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--snapshot", type=Path, required=True)
    parser.add_argument("--run-root", type=Path, required=True)
    parser.add_argument("--wheel", type=Path, required=True)
    args = parser.parse_args()
    if args.run_root.exists():
        raise SystemExit("proof run root must be new")
    args.run_root.mkdir(parents=True)

    def offline(*unused, **kwargs):
        raise AssertionError("external network attempted by offline fixture")
    socket.socket.connect = offline

    import frontmatter
    import yaml
    import llmwiki
    from llmwiki import _core, _native, atomic, query
    from llmwiki import compile as compiler
    from llmwiki.config import load_config
    assert not (args.snapshot / "llmwiki").exists()
    sys.path.insert(0, str(args.snapshot))
    from siwen import patches  # noqa: F401
    from siwen.kb import SiwenKB
    from siwen.web import SIWEN_STATIC_DIR, create_siwen_web_app
    from siwen.bake import (BakeRun, FIXED_JSON_ENDPOINTS, QUERY_JSON_ENDPOINTS,
                            _copy_static_assets, _verify_clean, bake_json_tree, bake_html_tree)

    prefix = Path(sys.prefix).resolve()
    assert Path(llmwiki.__file__).resolve().is_relative_to(prefix)
    assert Path(_core.__file__).resolve().is_relative_to(prefix)
    assert _native.backend_info()["core"] == "rust"
    fixture = runpy.run_path(str(args.snapshot / "tests/conftest.py"))["tmp_kb"].__wrapped__

    receipts = {"native": {}, "flask": {}, "journal": {}, "bake": {}, "private": {}}
    required = list(FIXED_JSON_ENDPOINTS) + [f"{p}?{q}" for p, q in QUERY_JSON_ENDPOINTS]

    def build_site(site):
        base = fixture(args.run_root / f"library-{site['key']}")
        cfg_path = base / "config.yaml"
        cfg_data = yaml.safe_load(cfg_path.read_text())
        cfg_data["branding"] = {"name": site["branding"], "name_short": site["branding"][0],
                                "tagline": site["tagline"], "canonical_origin": site["origin"]}
        cfg_path.write_text(yaml.dump(cfg_data, allow_unicode=True), encoding="utf-8")
        concepts = base / "wiki/concepts"
        # Same slug in BOTH libraries, distinct title/summary/body sentinels and
        # unknown metadata keys; non-BMP characters in the body.
        post = frontmatter.Post(f"正文唯此庫有：{site['sentinel']}。\n")
        post.metadata.update({"title": site["title"], "summary": site["sentinel"],
                              "tags": ["驗"], "created": "2026-04-08T00:00:00+00:00",
                              "updated": "2026-04-08T00:00:00+00:00",
                              "mystery_field_x9": "unknown-metadata", "wenguan_internal": f"internal-{site['key']}"})
        (concepts / f"{site['slug']}.md").write_text(frontmatter.dumps(post), encoding="utf-8")
        slug, title, body = site["extra"]
        extra = frontmatter.Post(f"{body}此篇屬{site['branding']}。\n")
        extra.metadata.update({"title": title, "summary": body, "tags": ["驗"],
                               "created": "2026-04-08T00:00:00+00:00", "updated": "2026-04-08T00:00:00+00:00"})
        (concepts / f"{slug}.md").write_text(frontmatter.dumps(extra), encoding="utf-8")
        # Private fixture: raw-only document of a non-wikisource internal type,
        # never compiled into a concept nor scanned as a classic.
        raw = base / "raw/private-scroll/index.md"
        raw.parent.mkdir(parents=True)
        raw.write_text(frontmatter.dumps(frontmatter.Post(
            f"私藏不可示人：{site['sentinel']}私。\n", title=f"私卷·{site['branding']}",
            type="internal-scroll", url=f"{site['origin']}/private/scroll",
            source_path="/private/synthetic/raw.xml")), encoding="utf-8")
        # Public raw classic (wikisource), carrying this site's canonical origin.
        pub = base / f"raw/public-{site['key']}/index.md"
        pub.parent.mkdir(parents=True)
        pub.write_text(frontmatter.dumps(frontmatter.Post(
            f"公開經文：{site['sentinel']}。\n", title=f"公開-{site['branding']}",
            type="wikisource", plugin="wikisource", work=site["extra"][1],
            chapter="第一章", url=f"{site['origin']}/classic/chapter-1")), encoding="utf-8")
        site["base"] = base
        site["cfg"] = load_config(base)
        return base

    for site in SITES:
        build_site(site)
        _native.clear_trace()
        compiler.rebuild_index(site["base"])
        trace = _native.last_trace()
        assert trace and trace["core"] == "rust"
        receipts["native"].setdefault("rebuild_index", {})[site["key"]] = trace
    other_of = {SITES[0]["key"]: SITES[1], SITES[1]["key"]: SITES[0]}

    # --- B4 exact-byte atomic journal through the installed facade ---------
    standalone = args.run_root / "standalone/note.txt"
    _native.clear_trace()
    atomic.atomic_write_text(standalone, "獨立無庫𠮷")
    assert _native.last_trace()["coordination"] == "standalone_file_lock"
    assert standalone.read_bytes() == "獨立無庫𠮷".encode()
    assert not list((args.run_root / "standalone").rglob("*.sqlite3*"))
    receipts["journal"]["standalone"] = {"coordination": "standalone_file_lock", "bytes_exact": True}

    def journal_rows(base):
        dbs = [p for p in base.rglob("*.sqlite3") if p.is_file()]
        if not dbs:
            return None, []
        assert len(dbs) == 1, dbs
        db = sqlite3.connect(str(dbs[0]))
        try:
            rows = db.execute(
                "SELECT id, library_id, relative_path, before_hash, after_hash,"
                " length(before_bytes), length(after_bytes), status FROM write_journal"
                " ORDER BY created_at, id").fetchall()
        finally:
            db.close()
        return dbs[0], rows

    def write_meta(site, payload):
        # Explicit non-default native library ids were attempted first and are
        # denied by the installed extension ("library scope denied", s17b log),
        # so journaling runs with the default native id; library identity is
        # distinguished by root (separate bases and separate core databases).
        target = site["base"] / "wiki/_meta/dual-proof.json"
        _native.with_writer(site["base"],
                            lambda: atomic.atomic_write_text(target, payload),
                            paths=site["cfg"]["paths"])
        return target

    first = SITES[0]
    payloads = [f"第一筆{first['sentinel']}", f"第二筆{first['sentinel']}"]
    target = write_meta(first, payloads[0])
    write_meta(first, payloads[1])
    db_path, rows = journal_rows(first["base"])
    assert len(rows) == 2, rows
    assert sorted(r[4] for r in rows) == sorted(sha(p.encode()) for p in payloads)
    assert sorted(r[6] for r in rows) == sorted(len(p.encode()) for p in payloads)
    assert all(row[1] == "default" for row in rows) and rows[0][0] != rows[1][0]
    assert target.read_text() == payloads[1]
    other = other_of[first["key"]]
    other_db, other_rows = journal_rows(other["base"])
    assert not [r for r in other_rows if "dual-proof" in r[2]], other_rows
    assert not (other["base"] / "wiki/_meta/dual-proof.json").exists()
    other_db_hash = digest(other_db) if other_db else None
    receipts["journal"][first["key"]] = {"db": str(db_path), "rows": [list(r) for r in rows],
        "applied_ids_distinct": True, "after_hashes_match": True,
        "library_id_mode": "default native id; explicit per-library ids denied by extension ('library scope denied', s17b log)"}
    receipts["journal"][other["key"] + "_untouched"] = {"dual_proof_rows": 0,
        "file_absent": True, "db_sha256": other_db_hash}

    # --- Alternate native search/get between both libraries ----------------
    old_chat, old_ctx, old_prompt = query.chat, query.chat_with_context, query.SYSTEM_PROMPT
    ask_evidence = {}
    try:
        for site in (SITES[0], SITES[1], SITES[0], SITES[1]):
            query.chat = lambda *a, **kw: site["title"]
            def answer(question, context, **kwargs):
                return f"<think>hidden</think>{site['sentinel']}之對。"
            query.chat_with_context = answer
            query.SYSTEM_PROMPT = f"系統-{site['branding']}"
            kb = SiwenKB(site["base"])
            _native.clear_trace()
            result = kb.ask("共目何義？", file_back=False, promote=False)
            trace = _native.last_trace()
            assert trace and trace["core"] == "rust"
            assert site["sentinel"] in result["answer"] and "<think>" not in result["answer"]
            assert other_of[site["key"]]["sentinel"] not in result["answer"]
            ask_evidence.setdefault(site["key"], []).append(
                {"answer": result["answer"], "trace_core": trace["core"]})
    finally:
        query.chat, query.chat_with_context, query.SYSTEM_PROMPT = old_chat, old_ctx, old_prompt
    receipts["native"]["ask"] = ask_evidence

    # --- Alternate Flask requests between both app instances ---------------
    assert SIWEN_STATIC_DIR.is_dir()
    for site in SITES:
        app = create_siwen_web_app(site["base"])
        app.testing = True
        client = app.test_client()
        site["client"] = client
        branding = client.get("/api/branding").json
        assert branding["name"] == site["branding"], branding
        articles = client.get("/api/articles").json["articles"]
        blob = json.dumps(articles, ensure_ascii=False)
        assert site["slug"] in blob and site["title"] in blob, blob[:200]
        assert other_of[site["key"]]["title"] not in blob
        assert "私卷" not in blob, "private raw document leaked into /api/articles"
        classics = client.get("/api/classics").json
        cblob = json.dumps(classics, ensure_ascii=False)
        assert classics["books"] and site["extra"][1] in cblob
        assert other_of[site["key"]]["extra"][1] not in cblob
        assert "私卷" not in cblob, "private internal document leaked into /api/classics"
        sources_blob = json.dumps(client.get("/api/sources").json, ensure_ascii=False)
        assert site["origin"] in sources_blob and other_of[site["key"]]["origin"] not in sources_blob
        receipts["private"][site["key"]] = {"articles_exclude_private": "私卷" not in blob,
            "classics_exclude_private": "私卷" not in cblob,
            "sources_lists_private_raw": "私卷" in sources_blob,
            "baked_content_trees_exclude_private": None}
        page = client.get(f"/wiki/{site['slug']}").data.decode()
        assert site["branding"] in page and site["origin"] in page
        assert other_of[site["key"]]["origin"] not in page and other_of[site["key"]]["branding"] not in page
        status = {}
        for path in required:
            response = client.get(path)
            status[path] = response.status_code
            assert response.status_code == 200, (site["key"], path, response.status_code)
        receipts["flask"][site["key"]] = {"branding": branding, "shared_slug_title": site["title"],
            "article_count": len(articles), "required_status": status,
            "origin_in_page": site["origin"] in page}

    # --- Actual populated BakeRun for both sites ---------------------------
    for site in SITES:
        out = args.run_root / f"baked-{site['key']}"
        _copy_static_assets(out)
        run = BakeRun(site["client"], out)
        ids = bake_json_tree(run)
        bake_html_tree(run, "full", ids)
        _verify_clean(out)
        unexpected = [i for i in run.skipped if not i.endswith("(HTTP 404 optional)")]
        assert not unexpected, (site["key"], unexpected)
        assert ids["articles"] and ids["classics"]
        texts = []
        for path in out.rglob("*"):
            if path.is_file() and path.suffix in {".html", ".json"}:
                try:
                    texts.append((path.name, path.read_text(encoding="utf-8", errors="ignore")))
                except Exception:
                    pass
        baked = "\n".join(text for _, text in texts)
        other = other_of[site["key"]]
        assert site["sentinel"] in baked and site["branding"] in baked
        assert other["sentinel"] not in baked and other["origin"] not in baked
        content_trees = [text for name, text in texts if "articles" in name or "classics" in name]
        assert content_trees and all("私卷" not in text for text in content_trees)
        receipts["bake"][site["key"]] = {"json_count": run.json_count, "html_count": run.html_count,
            "skipped": run.skipped, "article_count": len(ids["articles"]),
            "classic_count": len(ids["classics"]), "verified_clean": True,
            "own_sentinel_present": True, "other_sentinel_absent": True,
            "branding_present": site["branding"] in baked}
        receipts["private"][site["key"]]["baked_content_trees_exclude_private"] = True

    receipt = {
        "llmwiki_version": importlib.metadata.version("llmwiki"),
        "siwen_version": importlib.metadata.version("siwen"),
        "llmwiki_module": llmwiki.__file__, "siwen_module": sys.modules["siwen"].__file__,
        "extension": _core.__file__, "extension_sha256": digest(Path(_core.__file__)),
        "wheel_sha256": digest(args.wheel), "external_network": "blocked",
        "model_transport": "explicit offline Python callback",
        "libraries": [{"key": s["key"], "library_id": s["library_id"], "base": str(s["base"]),
                       "branding": s["branding"], "canonical_origin": s["origin"],
                       "shared_slug": s["slug"]} for s in SITES],
        "receipts": receipts,
        "boundary": "Global Python monkey-patches (query.chat etc.) are process-wide and were "
                    "serialized per operation; no per-library patch isolation is claimed. Siwen "
                    "custom compiler, taxonomy and Flask extension code remain Python. The private "
                    "fixture is a raw uncompiled internal-type document, excluded from article and "
                    "classic content surfaces; raw source-inventory endpoints list raw documents by "
                    "design and are recorded as evidence, not asserted empty. Explicit non-default "
                    "native library ids are denied by the "
                    "installed extension ('library scope denied'); per-library atomic identity is "
                    "shown by root with the default id. No real-provider semantic quality is claimed.",
    }
    (args.run_root / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"result": "passed", "receipt": str(args.run_root / "receipt.json"),
                      "bake": receipts["bake"]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
