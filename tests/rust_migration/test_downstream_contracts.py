"""固定斯文所用之 facade 契約；此套本身不充 Rust 接管之證。"""

import hashlib
import shutil
from pathlib import Path

import frontmatter
import pytest


FIXTURE = Path(__file__).parent / "fixtures" / "downstream" / "library"


@pytest.fixture
def downstream_library(tmp_path, monkeypatch):
    base = tmp_path / "library"
    shutil.copytree(FIXTURE, base)
    monkeypatch.delenv("PORT", raising=False)
    monkeypatch.delenv("LLMBASE_API_SECRET", raising=False)
    from llmwiki.config import ensure_dirs, load_config

    ensure_dirs(load_config(base))
    return base


def test_section_assignment_is_read_each_call_and_anchor_keeps_identity(monkeypatch):
    import llmwiki.compile as compiler
    from llmwiki.config import get_language_profile, get_section_headers

    anchor = compiler.DEFAULT_SECTION_HEADERS
    cfg = {"languages": {"default": "single", "profiles": {
        "single": {"sections": [{"key": "配置", "header": "## 配置"}]}
    }}}
    monkeypatch.setattr(compiler, "SECTION_HEADERS", anchor)
    assert get_section_headers(cfg) == [("配置", "## 配置")]
    monkeypatch.setattr(compiler, "SECTION_HEADERS", [("文言", "")])
    first = get_language_profile(cfg)
    assert first["name"] == "_patched"
    assert first["bare_body"] is True
    assert first["codes"] == ["文言"]
    assert first["default_lang"] == first["api_default_lang"] == "zh"
    monkeypatch.setattr(compiler, "SECTION_HEADERS", [("後改", "## 後改")])
    assert get_section_headers(cfg) == [("後改", "## 後改")]
    assert compiler.DEFAULT_SECTION_HEADERS is anchor
    monkeypatch.setattr(compiler, "SECTION_HEADERS", anchor)
    assert get_section_headers(cfg) == [("配置", "## 配置")]


def test_language_override_reaches_existing_flask_app(downstream_library, monkeypatch):
    import llmwiki.compile as compiler
    from llmwiki.web import create_web_app

    monkeypatch.setattr(compiler, "SECTION_HEADERS", compiler.DEFAULT_SECTION_HEADERS)
    client = create_web_app(downstream_library).test_client()
    assert client.get("/api/languages").get_json()["name"] == "classical"
    monkeypatch.setattr(compiler, "SECTION_HEADERS", [("文言", "")])
    patched = client.get("/api/languages").get_json()
    assert patched["name"] == "_patched"
    assert patched["bare_body"] is True
    monkeypatch.setattr(compiler, "SECTION_HEADERS", compiler.DEFAULT_SECTION_HEADERS)
    assert client.get("/api/languages").get_json()["name"] == "classical"


def test_compile_private_helpers_keep_paths_and_list_shape(downstream_library):
    from llmwiki.compile import _find_uncompiled, _list_existing_concepts
    from llmwiki.config import load_config

    cfg = load_config(downstream_library)
    raw = Path(cfg["paths"]["raw"])
    concepts = Path(cfg["paths"]["concepts"])
    assert raw == downstream_library / "originals"
    assert concepts == downstream_library / "knowledge" / "entries"
    assert _find_uncompiled(raw) == [raw / "lunyu" / "index.md"]
    assert _list_existing_concepts(concepts) == ["xue (學)"]
    source = raw / "lunyu" / "index.md"
    post = frontmatter.load(source)
    post["compiled"] = True
    source.write_text(frontmatter.dumps(post), encoding="utf-8")
    assert _find_uncompiled(raw) == []
    assert _find_uncompiled(raw / "absent") == []


def test_query_runtime_prompt_and_mutable_tone_are_not_cached(downstream_library, monkeypatch):
    import llmwiki.query as query

    calls = []
    def respond(question, files, **kwargs):
        calls.append((question, files, kwargs))
        return "學而習之。"

    monkeypatch.setattr(query, "chat_with_context", respond)
    monkeypatch.setattr(query, "SYSTEM_PROMPT", "初訓")
    monkeypatch.setattr(query, "TONE_INSTRUCTIONS", {"wenyan": "初調"})
    answer = query.query("學", base_dir=downstream_library, tone="wenyan")
    assert answer == "學而習之。"
    assert "初訓" in calls[-1][2]["system"]
    assert "初調" in calls[-1][2]["system"]
    query.TONE_INSTRUCTIONS["wenyan"] = "後調"
    monkeypatch.setattr(query, "SYSTEM_PROMPT", "後訓")
    result = query.query("學", base_dir=downstream_library, tone="wenyan", return_path=True)
    assert result == {"answer": "學而習之。", "output_path": None}
    assert "後訓" in calls[-1][2]["system"]
    assert "後調" in calls[-1][2]["system"]
    assert not list((downstream_library / "knowledge" / "answers").iterdir())


def test_knowledgebase_legacy_ask_default_files_answer(downstream_library, monkeypatch):
    import llmwiki.query as query
    from llmwiki.agent_api import KnowledgeBase

    monkeypatch.setattr(query, "chat_with_context", lambda *a, **kw: "學而習之。")
    result = KnowledgeBase(downstream_library).ask("學")
    assert result == {"status": "ok", "answer": "學而習之。"}
    outputs = list((downstream_library / "knowledge" / "answers").glob("*.md"))
    assert len(outputs) == 1
    assert "學而習之。" in outputs[0].read_text(encoding="utf-8")


def test_taxonomy_callable_is_replaced_and_receives_real_articles(downstream_library, monkeypatch):
    import llmwiki.taxonomy as taxonomy

    calls = []
    def generator(label):
        def run(articles, cfg):
            calls.append((label, articles, cfg))
            return [{"id": "jing", "label": {"zh": label}, "children": [],
                     "article_slugs": ["xue"]}]
        return run

    monkeypatch.setattr(taxonomy, "chat", lambda *a, **k: pytest.fail("不可略 callback 而召模型"))
    monkeypatch.setattr(taxonomy, "TAXONOMY_GENERATOR", generator("經"))
    first = taxonomy.generate_taxonomy(downstream_library)
    monkeypatch.setattr(taxonomy, "TAXONOMY_GENERATOR", generator("後定經部"))
    second = taxonomy.generate_taxonomy(downstream_library)
    assert first["generated"] is second["generated"] is True
    assert first["categories"][0]["label"]["zh"] == "經"
    assert second["categories"][0]["label"]["zh"] == "後定經部"
    assert [c[0] for c in calls] == ["經", "後定經部"]
    assert calls[0][1][0]["slug"] == "xue"
    assert calls[0][2]["base_dir"] == str(downstream_library)
    article = frontmatter.load(downstream_library / "knowledge" / "entries" / "xue.md")
    assert article["editorial_extension"] == {"owner": "fixture", "revision_note": "保留未知欄"}


def test_flask_factory_retains_mutable_registry_hooks_and_runtime(downstream_library, monkeypatch):
    import llmwiki.web as web
    from flask import current_app, g, jsonify

    def before():
        g.contract_seen = True

    def after(response):
        response.headers["X-Contract"] = "middleware"
        return response

    @web.require_auth
    def custom():
        return jsonify({"base": str(current_app.config["llmbase"]["base_dir"]),
                        "before": g.contract_seen})

    monkeypatch.setattr(web, "EXTRA_ROUTES", [("/api/downstream-contract", custom, {"methods": ["POST"]})])
    monkeypatch.setattr(web, "BEFORE_REQUEST_HOOKS", [before])
    monkeypatch.setattr(web, "AFTER_REQUEST_HOOKS", [after])
    app = web.create_web_app(downstream_library)
    assert {"base_dir", "cfg", "api_secret", "session_token"} <= app.config["llmbase"].keys()
    by_url = {r.rule: r.endpoint for r in app.url_map.iter_rules()}
    assert by_url["/api/compile"] == "api_compile"
    assert by_url["/api/branding"] == "api_branding"
    app.view_functions["api_branding"] = lambda: jsonify({"name": "下游覆寫"})
    client = app.test_client()
    assert client.get("/api/branding").get_json() == {"name": "下游覆寫"}
    app.config["llmbase"].update(api_secret="fixture-only-token", session_token="fixture-session")
    assert client.post("/api/downstream-contract").status_code == 401
    app.config["llmbase"]["base_dir"] = downstream_library / "other"
    response = client.post("/api/downstream-contract", headers={"Authorization": "Bearer fixture-only-token"})
    assert response.status_code == 200
    assert response.headers["X-Contract"] == "middleware"
    assert response.get_json() == {"base": str(downstream_library / "other"), "before": True}


def test_bake_read_contract_preserves_raw_bytes_and_envelopes(downstream_library):
    from llmwiki.compile import rebuild_index
    from llmwiki.web import create_web_app

    raw_path = downstream_library / "originals" / "lunyu" / "index.md"
    original = raw_path.read_bytes()
    rebuild_index(downstream_library)
    client = create_web_app(downstream_library).test_client()
    for endpoint, collection_key in [("/api/articles", "articles"),
                                     ("/api/articles/lite", "articles"),
                                     ("/api/sources", "documents")]:
        response = client.get(endpoint)
        assert response.status_code == 200, endpoint
        assert len(response.get_json()[collection_key]) == 1, endpoint
    article = client.get("/api/articles/xue")
    assert article.status_code == 200
    body = article.get_json()
    assert body["title"] == "學"
    assert "學者" in body["content"]
    sections = client.get("/api/articles/xue/sections")
    assert sections.status_code == 200
    assert sections.get_json()["sections"][0]["title"] == "所出"
    assert raw_path.read_bytes() == original


def test_sections_keep_python_scalar_offsets_for_non_bmp_and_combining_text(downstream_library):
    from llmwiki.sections import extract_section_text, parse_sections

    post = frontmatter.load(downstream_library / "originals" / "lunyu" / "index.md")
    body = post.content
    sections = parse_sections(body)
    main = sections[0]
    assert main["start"] == body.index("## 學而第一")
    assert main["start"] < len(body[:main["start"]].encode("utf-8"))
    child = main["children"][0]
    assert child["start"] == body.index("### 字例")
    assert extract_section_text(body, sections, child["anchor"]) == body[child["start"]:]
    assert "𠮷" in body and "e\u0301" in body


def test_wenguan_cache_reads_existing_layout_and_hash_change_misses(tmp_path):
    from llmwiki.chunk_cache import ChunkCache

    cid, fingerprint, output = "taixu:序", "old-content-hash", "原字𠮷不易。"
    cid_hash = hashlib.sha256(cid.encode()).hexdigest()
    content_hash = hashlib.sha256(fingerprint.encode()).hexdigest()
    slot = tmp_path / "chunks" / cid_hash / f"{content_hash}.md"
    slot.parent.mkdir(parents=True)
    slot.write_text(output, encoding="utf-8")
    cache = ChunkCache(tmp_path, subdir="chunks")
    assert cache.get(cid, fingerprint) == output
    assert cache.get(cid, "new-content-hash") is None
    cache.put(cid, "new-content-hash", "後文。")
    assert cache.get(cid, fingerprint) == output
    assert cache.get(cid, "new-content-hash") == "後文。"
    assert ChunkCache(tmp_path, subdir="chunks").get(cid, "new-content-hash") == "後文。"


def test_llm_metadata_keeps_wenguan_truncation_contract(monkeypatch):
    import llmwiki.llm as llm

    monkeypatch.setattr(llm, "_call_llm", lambda *a, **kw: (
        "尚未竟", "length", {"reasoning_tokens": 17}))
    text, metadata = llm.chat_with_meta("原文", model="fixture-model")
    assert text == "尚未竟"
    assert metadata.truncated is True
    assert metadata.usage["reasoning_tokens"] == 17
    assert metadata.attempts == 1
    assert metadata.finish_reason == "length"
    assert metadata.model == "fixture-model"
    monkeypatch.setattr(llm, "_call_llm", lambda *a, **kw: ("已竟", "stop", {}))
    assert llm.chat_with_meta("原文", model="fixture-model")[1].truncated is False
