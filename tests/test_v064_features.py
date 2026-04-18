"""Tests for v0.6.6 additions: model override (A) + surrogate sanitize (C)."""

from unittest.mock import patch

import pytest

from tools.llm import strip_surrogates


# ─── C: surrogate sanitize ────────────────────────────────────────


def test_strip_surrogates_passes_clean_text():
    s = "玄之又玄，眾妙之門。Hello — 日本語"
    assert strip_surrogates(s) == s


def test_strip_surrogates_replaces_lone_low_surrogate():
    """The exact byte that crashes OpenAI client (\\udce7) must not survive."""
    bad = "head\udce7tail"
    out = strip_surrogates(bad)
    # Round-trip must succeed under strict UTF-8 — the bug we are fixing.
    out.encode("utf-8")
    assert "\udce7" not in out
    assert out.startswith("head") and out.endswith("tail")


def test_strip_surrogates_replaces_high_surrogate():
    bad = "\ud800abc"
    out = strip_surrogates(bad)
    out.encode("utf-8")
    assert "\ud800" not in out


def test_strip_surrogates_handles_non_string():
    assert strip_surrogates(None) is None
    assert strip_surrogates(42) == 42


def test_chat_with_context_sanitizes_payload():
    """End-to-end: surrogate-laden context must not break the LLM call."""
    captured = {}

    def fake_chat(prompt, system="", model=None, max_tokens=16384):
        captured["prompt"] = prompt
        captured["model"] = model
        # If sanitize failed, this encode raises and the test fails clearly.
        prompt.encode("utf-8")
        return "ok"

    with patch("tools.llm.chat", side_effect=fake_chat):
        from tools.llm import chat_with_context
        result = chat_with_context(
            question="what is \udce7?",
            context_files=[{"path": "art\udce7", "content": "body\udce7here"}],
        )
    assert result == "ok"
    assert "\udce7" not in captured["prompt"]


# ─── A: model override plumbing ───────────────────────────────────


def test_query_passes_model_through(tmp_kb):
    """query() must pass `model` into chat_with_context."""
    captured = {}

    def fake_cwc(question, context_files, system="", model=None, max_tokens=16384):
        captured["model"] = model
        return "stub answer"

    fake_ctx = [{"path": "stub.md", "content": "stub"}]
    with patch("tools.query._gather_context", return_value=fake_ctx), \
         patch("tools.query.chat_with_context", side_effect=fake_cwc):
        from tools.query import query
        out = query("test?", base_dir=tmp_kb, model="my-special-model")
    assert out == "stub answer"
    assert captured["model"] == "my-special-model"


def test_query_with_search_passes_model_through(tmp_kb):
    """query_with_search() must pass `model` into both LLM calls (selector + answer)."""
    # Seed minimal index.json so query_with_search doesn't bail early.
    import json
    meta_dir = tmp_kb / "wiki" / "_meta"
    meta_dir.mkdir(parents=True, exist_ok=True)
    (meta_dir / "index.json").write_text(json.dumps([
        {"slug": "kong", "title": "Emptiness / 空", "summary": "x", "tags": []},
    ]))

    selector_calls = []
    answer_calls = []

    def fake_chat(prompt, system="", model=None, max_tokens=16384):
        selector_calls.append(model)
        return "Emptiness / 空"

    def fake_cwc(question, context_files, system="", model=None, max_tokens=16384):
        answer_calls.append(model)
        return "stub answer"

    fake_ctx = [{"path": "stub.md", "content": "stub"}] * 3  # >=3 to skip fallback
    with patch("tools.query.chat", side_effect=fake_chat), \
         patch("tools.query.chat_with_context", side_effect=fake_cwc), \
         patch("tools.query._gather_context", return_value=fake_ctx):
        from tools.query import query_with_search
        query_with_search("what?", base_dir=tmp_kb, model="custom-model")

    assert "custom-model" in selector_calls
    assert "custom-model" in answer_calls


def test_query_default_model_is_none_when_omitted(tmp_kb):
    """Omitting `model` must result in None (defers to LLMBASE_MODEL downstream)."""
    captured = {}

    def fake_cwc(question, context_files, system="", model=None, max_tokens=16384):
        captured["model"] = model
        return "x"

    fake_ctx = [{"path": "stub.md", "content": "stub"}]
    with patch("tools.query._gather_context", return_value=fake_ctx), \
         patch("tools.query.chat_with_context", side_effect=fake_cwc):
        from tools.query import query
        query("q?", base_dir=tmp_kb)
    assert captured["model"] is None


# ─── A: /api/ask honors model in body ─────────────────────────────


def _client(tmp_kb):
    from tools.web import create_web_app
    app = create_web_app(tmp_kb)
    app.config["TESTING"] = True
    return app.test_client()


def test_api_ask_shallow_passes_model(tmp_kb):
    captured = {}

    def fake_query(q, file_back=False, base_dir=None, tone="default", return_path=False, model=None):
        captured["model"] = model
        return {"answer": "x", "output_path": None}

    with patch("tools.web.query", side_effect=fake_query):
        c = _client(tmp_kb)
        r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": "alpha-v2"})
    assert r.status_code == 200
    assert captured["model"] == "alpha-v2"


def test_api_ask_deep_passes_model(tmp_kb):
    captured = {}

    def fake_dispatch(name, base, args):
        captured.update(args)
        return {"answer": "x", "consulted": []}

    from tools import operations as _ops
    with patch.object(_ops, "dispatch", side_effect=fake_dispatch):
        c = _client(tmp_kb)
        r = c.post("/api/ask", json={"question": "hi", "deep": True, "model": "beta-v3"})
    assert r.status_code == 200
    assert captured["model"] == "beta-v3"


def test_api_ask_omitted_model_not_in_dispatch_args(tmp_kb):
    """When client omits `model`, ops dispatch must not receive a stray key."""
    captured = {}

    def fake_dispatch(name, base, args):
        captured.update(args)
        return {"answer": "x", "consulted": []}

    from tools import operations as _ops
    with patch.object(_ops, "dispatch", side_effect=fake_dispatch):
        c = _client(tmp_kb)
        r = c.post("/api/ask", json={"question": "hi", "deep": True})
    assert r.status_code == 200
    assert "model" not in captured


def test_api_ask_rejects_non_string_model(tmp_kb):
    c = _client(tmp_kb)
    r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": 123})
    assert r.status_code == 400


def test_api_ask_rejects_oversized_model(tmp_kb):
    c = _client(tmp_kb)
    r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": "x" * 201})
    assert r.status_code == 400


def test_api_ask_empty_string_model_treated_as_none(tmp_kb):
    """Empty / whitespace `model` must be treated as 'use default', not error."""
    captured = {}

    def fake_query(q, file_back=False, base_dir=None, tone="default", return_path=False, model=None):
        captured["model"] = model
        return {"answer": "x", "output_path": None}

    with patch("tools.web.query", side_effect=fake_query):
        c = _client(tmp_kb)
        r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": "  "})
    assert r.status_code == 200
    assert captured["model"] is None


# ─── A: kb_ask Operation accepts model ────────────────────────────


def test_kb_ask_operation_declares_model_param():
    from tools.operations import _REGISTRY
    op = _REGISTRY["kb_ask"]
    assert "model" in op.params["properties"]


# ─── C: ingest_url sanitizes before write ─────────────────────────


def test_ingest_url_sanitizes_content(tmp_kb, monkeypatch):
    """ingest_url must not write lone surrogates into raw/."""
    from tools import ingest as _ing

    class FakeResp:
        status_code = 200
        content = b"<html><head><title>T</title></head><body><p>fake</p></body></html>"
        text = content.decode()
        encoding = "utf-8"
        headers = {"content-type": "text/html; charset=utf-8"}
        def raise_for_status(self): pass

    # Bypass SSRF check + network — we only want to assert sanitize.
    monkeypatch.setattr(_ing, "_validate_url", lambda u: None)
    monkeypatch.setattr(_ing, "requests", type("R", (), {"get": staticmethod(lambda *a, **k: FakeResp())}))
    # Force a surrogate into the markdown conversion path.
    monkeypatch.setattr(_ing, "md", lambda *a, **k: "body\udce7tail")

    path = _ing.ingest_url("https://example.com/x", base_dir=tmp_kb)
    text = path.read_text(encoding="utf-8")
    assert "\udce7" not in text  # sanitized


def test_ingest_file_sanitizes_content(tmp_kb, monkeypatch):
    """ingest_file must strip surrogates from content + title before persisting.

    Real-world entry path: upstream mojibake leaves a lone surrogate in
    the Post returned by frontmatter.load. The file is readable (no
    surrogate bytes on disk yet) but the in-memory content carries it,
    and re-serialising via frontmatter.dumps + write_text would crash
    or propagate the surrogate downstream.
    """
    from tools import ingest as _ing
    import frontmatter

    src = tmp_kb / "src.md"
    src.write_text("---\ntitle: Clean\n---\n\nclean body\n", encoding="utf-8")

    # Inject a surrogate into the Post after frontmatter.load returns, as
    # if the upstream decoder had produced one.
    real_load = frontmatter.load

    def poisoned_load(path, *a, **kw):
        post = real_load(path, *a, **kw)
        post.content = "body\udce7poison"
        post.metadata["title"] = "dirty\udce7title"
        return post

    monkeypatch.setattr(_ing.frontmatter, "load", poisoned_load)

    path = _ing.ingest_file(str(src), base_dir=tmp_kb)
    text = path.read_text(encoding="utf-8")
    assert "\udce7" not in text
    text.encode("utf-8")  # must not raise


# ─── v0.6.7: model override auth + allowlist ──────────────────────


def test_api_ask_model_override_blocked_when_api_secret_set(tmp_kb, monkeypatch):
    """With API_SECRET set, unauthenticated `model` override returns 401."""
    monkeypatch.setenv("LLMBASE_API_SECRET", "secret-abc")
    c = _client(tmp_kb)
    r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": "alpha"})
    assert r.status_code == 401


def test_api_ask_model_override_allowed_when_authed(tmp_kb, monkeypatch):
    monkeypatch.setenv("LLMBASE_API_SECRET", "secret-abc")
    captured = {}

    def fake_query(q, file_back=False, base_dir=None, tone="default",
                   return_path=False, model=None):
        captured["model"] = model
        return {"answer": "x", "output_path": None}

    with patch("tools.web.query", side_effect=fake_query):
        c = _client(tmp_kb)
        r = c.post(
            "/api/ask",
            json={"question": "hi", "deep": False, "model": "alpha"},
            headers={"Authorization": "Bearer secret-abc"},
        )
    assert r.status_code == 200
    assert captured["model"] == "alpha"


def test_api_ask_model_override_rejects_spa_cookie(tmp_kb, monkeypatch):
    """SPA-minted session cookie must NOT unlock model override (codex finding).

    /api/ask promote=True still accepts the cookie for SPA convenience, but
    the model-override path — which has cost implications — requires the
    raw API secret in an Authorization header so a drive-by browser visitor
    cannot pin an expensive model just by loading `/`.
    """
    monkeypatch.setenv("LLMBASE_API_SECRET", "secret-abc")
    from tools.web import derive_session_token
    session = derive_session_token("secret-abc")
    c = _client(tmp_kb)
    c.set_cookie(domain="localhost", key="llmbase_auth", value=session)
    r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": "alpha"})
    assert r.status_code == 401
    assert "Authorization" in r.get_json()["message"]


def test_api_ask_promote_still_accepts_cookie(tmp_kb, monkeypatch):
    """Regression guard: promote=True must still accept SPA cookie auth."""
    monkeypatch.setenv("LLMBASE_API_SECRET", "secret-abc")
    from tools.web import derive_session_token
    session = derive_session_token("secret-abc")

    def fake_dispatch(name, base, args):
        return {"answer": "x", "consulted": []}

    from tools import operations as _ops
    with patch.object(_ops, "dispatch", side_effect=fake_dispatch):
        c = _client(tmp_kb)
        c.set_cookie(domain="localhost", key="llmbase_auth", value=session)
        r = c.post("/api/ask", json={"question": "hi", "deep": True, "promote": True})
    assert r.status_code == 200


def test_api_ask_model_allowlist_rejects_unlisted(tmp_kb, monkeypatch):
    monkeypatch.setenv("LLMBASE_MODEL_ALLOWLIST", "gpt-4o-mini,gpt-4o")
    c = _client(tmp_kb)
    r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": "expensive-o5"})
    assert r.status_code == 400


def test_api_ask_model_allowlist_accepts_listed(tmp_kb, monkeypatch):
    monkeypatch.setenv("LLMBASE_MODEL_ALLOWLIST", "gpt-4o-mini,gpt-4o")
    captured = {}

    def fake_query(q, file_back=False, base_dir=None, tone="default",
                   return_path=False, model=None):
        captured["model"] = model
        return {"answer": "x", "output_path": None}

    with patch("tools.web.query", side_effect=fake_query):
        c = _client(tmp_kb)
        r = c.post("/api/ask", json={"question": "hi", "deep": False, "model": "gpt-4o-mini"})
    assert r.status_code == 200
    assert captured["model"] == "gpt-4o-mini"


# ─── v0.6.7: slug sanitize + heal ─────────────────────────────────


def test_sanitize_slug_strips_url_punctuation():
    from tools.compile import sanitize_slug
    assert sanitize_slug("reasons-just-vs-expl/?ref=josephnoelwalker.com") == \
        "reasons-just-vs-expl-ref=josephnoelwalker.com"
    assert sanitize_slug("foo bar baz") == "foo-bar-baz"
    assert sanitize_slug("  ..  ") == ""
    assert sanitize_slug("a:b#c&d") == "a-b-c-d"


def test_heal_urly_slugs_renames_dirty_files(tmp_kb):
    """heal_urly_slugs must relocate concepts whose stem carries URL chars."""
    import frontmatter
    concepts_dir = tmp_kb / "wiki" / "concepts"
    # Simulate the pre-0.6.7 bug: a slug containing '/' produced a subdir.
    subdir = concepts_dir / "reasons-just-vs-expl"
    subdir.mkdir()
    dirty = subdir / "?ref=josephnoelwalker.com.md"
    dirty.write_text(frontmatter.dumps(frontmatter.Post(
        "## English\n\nStub.",
        title="Weird", summary="s", tags=["stub"],
        created="2026-04-01T00:00:00+00:00", updated="2026-04-01T00:00:00+00:00",
    )), encoding="utf-8")

    # Another article links to it with the dirty target.
    hume = concepts_dir / "david-hume.md"
    hume.write_text(frontmatter.dumps(frontmatter.Post(
        "See [[reasons-just-vs-expl/?ref=josephnoelwalker.com]].",
        title="Hume", summary="s", tags=[],
        created="2026-04-01T00:00:00+00:00", updated="2026-04-01T00:00:00+00:00",
    )), encoding="utf-8")

    from tools.lint.fixes import heal_urly_slugs
    # Stub rebuild_index to skip taxonomy/backlinks regen — we only care
    # about file renames and wikilink rewrites in this test.
    with patch("tools.compile.rebuild_index", lambda base_dir=None: []):
        fixes = heal_urly_slugs(tmp_kb)

    assert any("Renamed" in f for f in fixes)
    # Dirty path gone, clean path exists
    assert not dirty.exists()
    clean_slugs = {p.stem for p in concepts_dir.glob("*.md")}
    assert any("reasons-just-vs-expl" in s for s in clean_slugs)
    # Wikilink rewritten
    assert "reasons-just-vs-expl/?ref=" not in hume.read_text()


# ─── v0.6.7: HTTP timeout env override ────────────────────────────


def test_http_timeout_env_overrides_default(monkeypatch):
    """LLMBASE_HTTP_TIMEOUT must flow into the OpenAI client's httpx timeout."""
    import tools.llm as _llm
    monkeypatch.setattr(_llm, "_client", None)
    monkeypatch.setenv("LLMBASE_HTTP_TIMEOUT", "900")
    monkeypatch.setenv("LLMBASE_HTTP_CONNECT_TIMEOUT", "15")
    monkeypatch.setenv("LLMBASE_API_KEY", "sk-test")

    client = _llm.get_client()
    # httpx.Timeout stores read + connect as attributes
    assert client.timeout.read == 900.0
    assert client.timeout.connect == 15.0
    # Reset cached client so subsequent tests pick up env changes freely
    _llm._client = None


def test_worker_status_requires_auth_when_secret_set(tmp_kb, monkeypatch):
    """/api/worker/status is auth-gated when API_SECRET is set (codex finding).

    It leaks whether a write job is in flight, which tracks the same
    privilege as the write endpoints themselves.
    """
    monkeypatch.setenv("LLMBASE_API_SECRET", "secret-abc")
    c = _client(tmp_kb)
    r = c.get("/api/worker/status")
    assert r.status_code == 401

    r = c.get("/api/worker/status", headers={"Authorization": "Bearer secret-abc"})
    assert r.status_code == 200


def test_worker_status_reflects_lock_state(tmp_kb):
    """GET /api/worker/status must report the shared job_lock's liveness.

    Issue #7: the Ingest page uses this to recover in-flight compile state
    after a route change. Correctness guarantee: busy=True iff the lock is
    held by some other caller.
    """
    from tools.worker import job_lock
    c = _client(tmp_kb)

    assert not job_lock.locked()
    r = c.get("/api/worker/status")
    assert r.status_code == 200
    assert r.get_json() == {"busy": False}

    acquired = job_lock.acquire(blocking=False)
    assert acquired
    try:
        r = c.get("/api/worker/status")
        assert r.get_json() == {"busy": True}
    finally:
        job_lock.release()

    r = c.get("/api/worker/status")
    assert r.get_json() == {"busy": False}


def test_http_timeout_env_invalid_falls_back(monkeypatch):
    import tools.llm as _llm
    monkeypatch.setattr(_llm, "_client", None)
    monkeypatch.setenv("LLMBASE_HTTP_TIMEOUT", "not-a-number")
    monkeypatch.setenv("LLMBASE_API_KEY", "sk-test")
    client = _llm.get_client()
    assert client.timeout.read == 300.0
    _llm._client = None
