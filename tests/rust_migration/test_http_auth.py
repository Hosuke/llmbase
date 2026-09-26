"""Flask security contracts, including the deliberate visitor-cookie correction."""
import json

import frontmatter
import pytest

from llmwiki import operations, web
from llmwiki.agent_api import create_agent_server


@pytest.fixture(autouse=True)
def explicit_environment(monkeypatch):
    for name in ("LLMBASE_API_SECRET", "LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "PORT"):
        monkeypatch.delenv(name, raising=False)


def test_secretless_default_and_spa_cookie_cannot_run_models_or_writes(tmp_kb, monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("unauthorized request reached operation/model")
    monkeypatch.setattr(operations, "dispatch", forbidden)
    monkeypatch.setattr(web, "query", forbidden)
    app = web.create_web_app(tmp_kb)
    app.testing = True
    client = app.test_client()
    for path, body in [("/api/ask", {"question": "q", "file_back": False}),
                       ("/api/compile", {}), ("/api/ingest", {"source": "x"})]:
        assert client.post(path, json=body).status_code == 401
    monkeypatch.setenv("LLMBASE_API_SECRET", "operator-fixture")
    static = tmp_kb / "static/dist"
    static.mkdir(parents=True)
    (static / "index.html").write_text("fixture")
    app = web.create_web_app(tmp_kb)
    client = app.test_client()
    assert client.get("/").status_code == 200
    assert client.get_cookie("llmbase_auth") is not None
    for body in [{"question": "q", "deep": False, "file_back": False},
                 {"question": "q", "promote": True}, {"question": "q", "model": "expensive"}]:
        assert client.post("/api/ask", json=body).status_code == 401
    assert client.delete("/api/articles/kong").status_code == 401
    assert client.get("/api/articles").status_code == 200
    assert client.get("/api/trails").json == {"trails": []}
    trails = tmp_kb / "wiki/_meta/trails.json"
    trails.write_text('{"trails":[{"name":"PRIVATE_TRAIL"}]}')
    denied = client.get("/api/trails")
    assert denied.status_code == 401 and "PRIVATE_TRAIL" not in denied.get_data(as_text=True)
    trails.write_text('{"trails":[],"private_extra":"HIDDEN"}')
    assert client.get("/api/trails").json == {"trails": []}


def test_explicit_scoped_tokens_and_local_opt_in_have_separate_authority(tmp_kb, monkeypatch):
    calls = []
    monkeypatch.setattr(web, "query", lambda *a, **kw: calls.append(kw) or {"answer": "answer"})
    app = web.create_web_app(tmp_kb)
    app.config["llmbase"]["token_scopes"] = {"model-fixture": ("read", "model")}
    client = app.test_client()
    headers = {"Authorization": "Bearer model-fixture"}
    body = {"question": "q", "deep": False, "file_back": False}
    assert client.post("/api/ask", json=body, headers=headers).status_code == 200
    assert client.post("/api/ask", json={**body, "file_back": True}, headers=headers).status_code == 403
    assert client.delete("/api/articles/kong", headers=headers).status_code == 403
    assert len(calls) == 1
    app.config["llmbase"]["allow_unauthenticated_local"] = True
    assert client.post("/api/ask", json=body).status_code == 200
    assert client.post("/api/ask", json=body, environ_base={"REMOTE_ADDR": "203.0.113.5"}).status_code == 401
    assert client.post("/api/ask", json=body, headers={"X-Forwarded-For": "8.8.8.8"}).status_code == 401
    agent = create_agent_server(tmp_kb)
    agent.config["llmbase"]["token_scopes"] = {"model-fixture": ("read", "model")}
    # The compatibility handler treats non-empty fix values as true. Scope and
    # locking must follow that actual behavior even before strict schema validation.
    response = agent.test_client().post("/api/op/kb_lint", json={"fix": "yes"}, headers=headers)
    assert response.status_code == 403
    assert "write" in response.json["required_scopes"]


def test_agent_generic_dispatch_requires_explicit_dynamic_capabilities(tmp_kb, monkeypatch):
    calls = []
    custom = operations.Operation("custom_auth_fixture", "fixture", lambda base: calls.append(base) or {"value": 7})
    monkeypatch.setitem(operations._REGISTRY, custom.name, custom)
    app = create_agent_server(tmp_kb)
    client = app.test_client()
    assert client.post("/api/op/custom_auth_fixture", json={}).status_code == 401
    assert client.post("/api/ask", json={"question": "q"}).status_code == 401
    assert client.post("/api/index/rebuild", json={}).status_code == 401
    assert calls == []
    custom.required_scopes = ("read",)
    assert client.post("/api/op/custom_auth_fixture", json={}).json["result"] == {"value": 7}
    custom.required_scopes = None
    app.config["llmbase"]["api_secret"] = "operator-fixture"
    response = client.post("/api/op/custom_auth_fixture", json={}, headers={"Authorization": "Bearer operator-fixture"})
    assert response.status_code == 200
    assert response.headers["Cache-Control"] == "private, no-store"
    assert len(calls) == 2


def seed_private(tmp_kb):
    path = tmp_kb / "wiki/concepts/hidden.md"
    path.write_text("---\ntitle: PRIVATE_TITLE_SENTINEL\nprivate: true\nsummary: PRIVATE_SUMMARY_SENTINEL\n---\nPRIVATE_BODY_SENTINEL")
    raw = tmp_kb / "raw/hidden"
    raw.mkdir(parents=True)
    (raw / "index.md").write_text("---\nvisibility: internal\ntitle: PRIVATE_RAW_TITLE\n---\nPRIVATE_RAW_BODY")
    meta = tmp_kb / "wiki/_meta"
    (meta / "aliases.json").write_text(json.dumps({"SECRET_ALIAS": "hidden", "空": "kong"}))
    (meta / "backlinks.json").write_text(json.dumps({"kong": ["hidden"]}))
    (meta / "index.json").write_text(json.dumps([{"slug": "hidden", "title": "PRIVATE_TITLE_SENTINEL"}, {"slug": "kong", "title": "Public"}]))
    return path


def test_public_web_views_exclude_private_sources_and_withhold_opaque_artifacts(tmp_kb):
    seed_private(tmp_kb)
    app = web.create_web_app(tmp_kb)
    client = app.test_client()
    for url in ["/api/articles", "/api/articles?limit=10", "/api/articles/lite", "/api/collections",
                "/api/aliases", "/api/articles/kong", "/api/wiki/export", "/api/sources", "/api/search?q=PRIVATE"]:
        response = client.get(url)
        assert response.status_code == 200, (url, response.json)
        assert "PRIVATE_" not in response.get_data(as_text=True), url
        assert "SECRET_ALIAS" not in response.get_data(as_text=True), url
    for url in ["/api/articles/hidden", "/api/articles/hidden/sections", "/api/sources/hidden"]:
        assert client.get(url).status_code == 404
    for url in ["/api/taxonomy", "/api/entities", "/api/xici", "/api/export/article/kong", "/api/stats", "/api/health"]:
        response = client.get(url)
        assert response.status_code == 401
        assert response.json["required_scopes"] == ["private"]
    app.config["llmbase"]["token_scopes"] = {"private-fixture": ("read", "private")}
    private = client.get("/api/articles/hidden", headers={"Authorization": "Bearer private-fixture"})
    assert private.status_code == 200 and "PRIVATE_BODY_SENTINEL" in private.get_data(as_text=True)
    assert private.headers["Cache-Control"] == "private, no-store"


def test_agent_public_reads_and_generic_alias_get_do_not_leak_private(tmp_kb):
    seed_private(tmp_kb)
    client = create_agent_server(tmp_kb).test_client()
    for url in ["/api/articles", "/api/sources", "/api/search?q=PRIVATE"]:
        response = client.get(url)
        assert response.status_code == 200
        assert "PRIVATE_" not in response.get_data(as_text=True)
    assert client.get("/api/articles/hidden").status_code == 404
    result = client.post("/api/op/kb_get", json={"slug": "SECRET_ALIAS"})
    assert result.status_code == 200 and result.json["result"]["found"] is False
    assert "PRIVATE_" not in result.get_data(as_text=True)


def test_visibility_edit_invalidates_public_lite_etag_without_index_rebuild(tmp_kb):
    meta = tmp_kb / "wiki/_meta/index.json"
    meta.write_text('[{"slug":"kong","title":"cached title"}]')
    client = web.create_web_app(tmp_kb).test_client()
    before = client.get("/api/articles/lite")
    assert before.json["total"] == 1
    path = tmp_kb / "wiki/concepts/kong.md"
    post = frontmatter.load(path)
    post["visibility"] = "private"
    path.write_text(frontmatter.dumps(post))
    after = client.get("/api/articles/lite", headers={"If-None-Match": before.headers["ETag"]})
    assert after.status_code == 200
    assert after.json == {"articles": [], "total": 0}


def test_all_search_factories_filter_before_tokens_and_idf(tmp_kb, monkeypatch):
    import importlib
    search_module = importlib.import_module("llmwiki.search")
    from llmwiki import _native
    factories = [web.create_web_app, create_agent_server, search_module.create_search_app]
    baseline = []
    for factory in factories:
        baseline.append(factory(tmp_kb).test_client().get("/api/search?q=philosophy").json["results"])
    hidden = tmp_kb / "wiki/concepts/hidden.md"
    hidden.write_text("---\nprivate: true\ntitle: HIDDEN_TOKENIZER_SENTINEL\n---\nphilosophy")
    original = tmp_kb / "raw/book/.original"
    original.mkdir(parents=True)
    (original / "index.md").write_text("ORIGINAL_TOKENIZER_SENTINEL")
    seen = []
    def tokenizer(text):
        seen.append(text)
        return [part.lower() for part in text.split() if len(part) > 1]
    for index, factory in enumerate(factories):
        client = factory(tmp_kb).test_client()
        response = client.get("/api/search?q=philosophy")
        assert response.status_code == 200
        assert response.json["results"] == baseline[index]
        with monkeypatch.context() as scoped:
            scoped.setattr(search_module, "SEARCH_TOKENIZER", tokenizer)
            assert client.get("/api/search?q=philosophy").status_code == 200
        assert not any("HIDDEN_TOKENIZER_SENTINEL" in text for text in seen)
    client = web.create_web_app(tmp_kb).test_client()
    assert client.get("/api/sources/book/.original").status_code == 404
    with search_module.search_visibility(public_only=True):
        assert search_module.search_raw("ORIGINAL_TOKENIZER_SENTINEL", base_dir=tmp_kb) == []
    # The in-process API still defaults to its established full-library view.
    assert any(row["slug"] == "hidden" for row in search_module.search("philosophy", base_dir=tmp_kb))
    if _native.available():
        assert _native.last_trace()["visibility"] == "all"
