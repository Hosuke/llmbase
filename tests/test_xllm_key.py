"""Tests for v0.7.4 X-LLM-Key per-request API key override.

Security contract (non-negotiable — see `project_v07_plan.md`):
- Key arrives via ``X-LLM-Key`` HTTP header **only**; body fields named
  ``api_key`` / ``apiKey`` / ``api-key`` / ``API_KEY`` / ``x-llm-key``
  etc. are rejected with 400 before any further processing.
- When ``LLMBASE_API_SECRET`` is set, the header requires
  ``Authorization: Bearer <secret>`` (same gate as ``model`` override).
- When ``LLMBASE_API_SECRET`` is unset, the header is honoured without
  auth (local-dev parity with ``/api/ingest``).
- The key is never logged, never written to ``outputs/``, never echoed
  in the response body.
- ``get_client(api_key=<str>)`` returns a fresh un-cached client —
  swapping keys across requests must not leak.
"""

import json
import logging
from pathlib import Path
from unittest.mock import patch

import pytest


# ─── get_client caching semantics ──────────────────────────────────────


def test_get_client_default_is_singleton(monkeypatch):
    # Reset module-level singleton so this test is hermetic.
    import llmwiki.llm as llm_mod
    monkeypatch.setattr(llm_mod, "_client", None)
    monkeypatch.setenv("LLMBASE_API_KEY", "sk-singleton")
    a = llm_mod.get_client()
    b = llm_mod.get_client()
    assert a is b, "get_client(None) must return the cached singleton"


def test_get_client_with_api_key_is_not_cached(monkeypatch):
    import llmwiki.llm as llm_mod
    monkeypatch.setattr(llm_mod, "_client", None)
    monkeypatch.setenv("LLMBASE_API_KEY", "sk-singleton")
    # A per-request key must never become the singleton; two calls with
    # different keys must return two distinct clients, neither of which
    # is the singleton.
    singleton = llm_mod.get_client()
    c1 = llm_mod.get_client(api_key="sk-user-1")
    c2 = llm_mod.get_client(api_key="sk-user-2")
    assert c1 is not c2
    assert c1 is not singleton
    assert c2 is not singleton
    # And the module-level cache is still the singleton afterwards.
    assert llm_mod.get_client() is singleton


# ─── chat() / chat_with_context() plumbing ─────────────────────────────


def test_chat_forwards_api_key_to_call_llm():
    captured = {}

    def fake_call(messages, model, max_tokens, api_key=None):
        captured["api_key"] = api_key
        return ("ok", "stop", {"prompt_tokens": 1, "completion_tokens": 1,
                                "reasoning_tokens": None, "total_tokens": 2})

    with patch("llmwiki.llm._call_llm", side_effect=fake_call):
        from llmwiki.llm import chat
        chat("hi", model="m", api_key="sk-user")
    assert captured["api_key"] == "sk-user"


def test_chat_default_api_key_is_none():
    captured = {}

    def fake_call(messages, model, max_tokens, api_key=None):
        captured["api_key"] = api_key
        return ("ok", "stop", {"prompt_tokens": 1, "completion_tokens": 1,
                                "reasoning_tokens": None, "total_tokens": 2})

    with patch("llmwiki.llm._call_llm", side_effect=fake_call):
        from llmwiki.llm import chat
        chat("hi", model="m")
    assert captured["api_key"] is None


def test_chat_with_context_forwards_api_key():
    captured = {}

    def fake_chat(prompt, system="", model=None, max_tokens=16384, api_key=None):
        captured["api_key"] = api_key
        return "ok"

    with patch("llmwiki.llm.chat", side_effect=fake_chat):
        from llmwiki.llm import chat_with_context
        chat_with_context(
            "q?", [{"path": "a.md", "content": "body"}], api_key="sk-user"
        )
    assert captured["api_key"] == "sk-user"


# ─── query.py forwards api_key ─────────────────────────────────────────


def test_query_passes_api_key_through(tmp_kb):
    captured = {}

    def fake_cwc(question, context_files, system="", model=None, max_tokens=16384, api_key=None):
        captured["api_key"] = api_key
        return "ans"

    fake_ctx = [{"path": "stub.md", "content": "stub"}]
    with patch("llmwiki.query._gather_context", return_value=fake_ctx), \
         patch("llmwiki.query.chat_with_context", side_effect=fake_cwc):
        from llmwiki.query import query
        query("q?", base_dir=tmp_kb, api_key="sk-user-xyz")
    assert captured["api_key"] == "sk-user-xyz"


def test_query_with_search_passes_api_key_to_selector_and_answer(tmp_kb):
    meta_dir = tmp_kb / "wiki" / "_meta"
    meta_dir.mkdir(parents=True, exist_ok=True)
    (meta_dir / "index.json").write_text(json.dumps([
        {"slug": "x", "title": "X", "summary": "x", "tags": []},
    ]))

    selector_keys = []
    answer_keys = []

    def fake_chat(prompt, system="", model=None, max_tokens=16384, api_key=None):
        selector_keys.append(api_key)
        return "X"

    def fake_cwc(question, context_files, system="", model=None, max_tokens=16384, api_key=None):
        answer_keys.append(api_key)
        return "ans"

    fake_ctx = [{"path": "stub.md", "content": "stub"}] * 3
    with patch("llmwiki.query.chat", side_effect=fake_chat), \
         patch("llmwiki.query.chat_with_context", side_effect=fake_cwc), \
         patch("llmwiki.query._gather_context", return_value=fake_ctx):
        from llmwiki.query import query_with_search
        query_with_search("q?", base_dir=tmp_kb, api_key="sk-per-req")

    assert "sk-per-req" in selector_keys
    assert "sk-per-req" in answer_keys


# ─── operations.py / kb_ask schema ─────────────────────────────────────


def test_kb_ask_schema_declares_api_key_writeonly():
    """The Operation schema must mark ``api_key`` ``writeOnly`` so it
    never echoes in op listings or MCP tool descriptions."""
    # Ensure the default op pack is registered. Importing the module
    # triggers the module-level ``Operation(...)`` block at the bottom.
    import llmwiki.operations as _ops
    kb_ask = _ops.get("kb_ask")
    assert kb_ask is not None, "kb_ask must be registered"
    props = kb_ask.params["properties"]
    assert "api_key" in props
    assert props["api_key"].get("writeOnly") is True


def test_op_ask_forwards_api_key(tmp_kb):
    captured = {}

    def fake_qws(question, **kwargs):
        captured.update(kwargs)
        return {"answer": "x", "consulted": []}

    with patch("llmwiki.query.query_with_search", side_effect=fake_qws):
        from llmwiki.operations import _op_ask
        _op_ask(tmp_kb, question="q", deep=True, api_key="sk-user")
    assert captured.get("api_key") == "sk-user"


# ─── /api/ask HTTP surface ─────────────────────────────────────────────


def _client(tmp_kb):
    from llmwiki.web import create_web_app
    app = create_web_app(tmp_kb)
    app.config["TESTING"] = True
    app.config["llmbase"]["allow_unauthenticated_local"] = True
    return app.test_client()


def _stub_shallow(captured):
    def fake_query(q, file_back=False, base_dir=None, tone="default",
                   return_path=False, model=None, api_key=None, **kwargs):
        captured["api_key"] = api_key
        return {"answer": "stub", "output_path": None}
    return fake_query


def _stub_deep(captured):
    def fake_dispatch(name, base, args):
        captured.update(args)
        return {"answer": "stub", "consulted": []}
    return fake_dispatch


@pytest.mark.parametrize("body_field", [
    "api_key", "apiKey", "api-key", "API_KEY",
    "x-llm-key", "x_llm_key", "X-LLM-Key",
    "openai_api_key", "llm_key", "llmKey",
])
def test_api_ask_rejects_api_key_in_body_toplevel(tmp_kb, body_field):
    """Any spelling of a key-bearing body field → 400, before the key
    could reach any downstream code / log."""
    c = _client(tmp_kb)
    r = c.post("/api/ask", json={
        "question": "hi",
        "deep": False,
        body_field: "sk-should-not-land-here",
    })
    assert r.status_code == 400, r.data
    # And the rejection message itself must not echo the key.
    assert b"sk-should-not-land-here" not in r.data


def test_api_ask_rejects_api_key_in_nested_body(tmp_kb):
    """Codex HIGH (2026-04-20): top-level-only scan was bypassable by
    burying the key in a nested object. Rejection must recurse."""
    c = _client(tmp_kb)
    r = c.post("/api/ask", json={
        "question": "hi",
        "deep": False,
        "meta": {"api_key": "sk-nested-leak"},
    })
    assert r.status_code == 400, r.data
    assert b"sk-nested-leak" not in r.data


def test_api_ask_rejects_api_key_in_list_body(tmp_kb):
    """List of objects also recursed."""
    c = _client(tmp_kb)
    r = c.post("/api/ask", json={
        "question": "hi",
        "deep": False,
        "attachments": [{"name": "x"}, {"apiKey": "sk-list-leak"}],
    })
    assert r.status_code == 400, r.data


def test_api_ask_rejects_api_key_at_arbitrary_depth(tmp_kb):
    """Codex HIGH (2026-04-20 round 2): a hard depth limit was itself a
    bypass vector — attacker nests beyond the cap. Scan is now
    iterative / unbounded; verify an obnoxiously-deep payload still
    gets rejected."""
    deep: dict = {"question": "hi", "deep": False}
    node = deep
    # Deeper than any plausible legitimate API body (pathological).
    for _ in range(50):
        node["nest"] = {}
        node = node["nest"]
    node["api_key"] = "sk-deep-leak"
    c = _client(tmp_kb)
    r = c.post("/api/ask", json=deep)
    assert r.status_code == 400
    assert b"sk-deep-leak" not in r.data


def test_api_ask_non_bearer_auth_rejected_for_x_llm_key(tmp_kb, monkeypatch):
    """Codex HIGH (2026-04-20): ``.replace("Bearer ", "")`` accepted
    ``Authorization: <secret>`` without the Bearer scheme. Tightened to
    require the literal Bearer prefix."""
    monkeypatch.setenv("LLMBASE_API_SECRET", "shh")
    c = _client(tmp_kb)
    r = c.post(
        "/api/ask",
        json={"question": "hi", "deep": False},
        headers={
            # Raw secret without Bearer scheme — must NOT unlock X-LLM-Key.
            "Authorization": "shh",
            "X-LLM-Key": "sk-user",
        },
    )
    assert r.status_code == 401


def test_api_ask_non_bearer_auth_rejected_for_model_override(tmp_kb, monkeypatch):
    """Same fix covers the model-override strong-auth gate."""
    monkeypatch.setenv("LLMBASE_API_SECRET", "shh")
    c = _client(tmp_kb)
    r = c.post(
        "/api/ask",
        json={"question": "hi", "deep": False, "model": "alpha"},
        headers={"Authorization": "shh"},  # no Bearer prefix
    )
    assert r.status_code == 401


def test_api_ask_x_llm_key_honored_in_local_dev(tmp_kb, monkeypatch):
    """Without LLMBASE_API_SECRET, X-LLM-Key header is accepted without auth."""
    monkeypatch.delenv("LLMBASE_API_SECRET", raising=False)
    captured = {}
    with patch("llmwiki.web.query", side_effect=_stub_shallow(captured)):
        c = _client(tmp_kb)
        r = c.post(
            "/api/ask",
            json={"question": "hi", "deep": False},
            headers={"X-LLM-Key": "sk-dev-key"},
        )
    assert r.status_code == 200
    assert captured["api_key"] == "sk-dev-key"


def test_api_ask_x_llm_key_requires_auth_when_secret_set(tmp_kb, monkeypatch):
    monkeypatch.setenv("LLMBASE_API_SECRET", "shh")
    c = _client(tmp_kb)
    r = c.post(
        "/api/ask",
        json={"question": "hi", "deep": False},
        headers={"X-LLM-Key": "sk-user"},
    )
    assert r.status_code == 401
    assert b"X-LLM-Key" in r.data


def test_api_ask_x_llm_key_accepted_with_strong_auth(tmp_kb, monkeypatch):
    monkeypatch.setenv("LLMBASE_API_SECRET", "shh")
    captured = {}
    with patch("llmwiki.web.query", side_effect=_stub_shallow(captured)):
        c = _client(tmp_kb)
        r = c.post(
            "/api/ask",
            json={"question": "hi", "deep": False},
            headers={
                "Authorization": "Bearer shh",
                "X-LLM-Key": "sk-user",
            },
        )
    assert r.status_code == 200
    assert captured["api_key"] == "sk-user"


def test_api_ask_cookie_auth_insufficient_for_x_llm_key(tmp_kb, monkeypatch):
    """Cookie (promote-level) auth must NOT unlock X-LLM-Key — that
    would let drive-by browser visitors burn the operator's key."""
    monkeypatch.setenv("LLMBASE_API_SECRET", "shh")
    from llmwiki.web import derive_session_token
    cookie = derive_session_token("shh")
    c = _client(tmp_kb)
    c.set_cookie("llmbase_auth", cookie)
    r = c.post(
        "/api/ask",
        json={"question": "hi", "deep": False},
        headers={"X-LLM-Key": "sk-user"},
    )
    assert r.status_code == 401


def test_api_ask_deep_path_forwards_api_key(tmp_kb, monkeypatch):
    monkeypatch.delenv("LLMBASE_API_SECRET", raising=False)
    captured = {}
    from llmwiki import operations as _ops
    with patch.object(_ops, "dispatch", side_effect=_stub_deep(captured)):
        c = _client(tmp_kb)
        r = c.post(
            "/api/ask",
            json={"question": "hi", "deep": True},
            headers={"X-LLM-Key": "sk-deep"},
        )
    assert r.status_code == 200
    assert captured["api_key"] == "sk-deep"


def test_api_ask_empty_x_llm_key_header_treated_as_none(tmp_kb, monkeypatch):
    monkeypatch.delenv("LLMBASE_API_SECRET", raising=False)
    captured = {}
    with patch("llmwiki.web.query", side_effect=_stub_shallow(captured)):
        c = _client(tmp_kb)
        r = c.post(
            "/api/ask",
            json={"question": "hi", "deep": False},
            headers={"X-LLM-Key": "   "},
        )
    assert r.status_code == 200
    assert captured["api_key"] is None


def test_api_ask_oversized_x_llm_key_rejected(tmp_kb):
    c = _client(tmp_kb)
    r = c.post(
        "/api/ask",
        json={"question": "hi", "deep": False},
        headers={"X-LLM-Key": "x" * 501},
    )
    assert r.status_code == 400


# ─── No-leak invariants ────────────────────────────────────────────────


def test_api_ask_response_does_not_echo_key(tmp_kb, monkeypatch):
    monkeypatch.delenv("LLMBASE_API_SECRET", raising=False)
    KEY = "sk-this-must-not-leak-abc123"

    def fake_query(q, file_back=False, base_dir=None, tone="default",
                   return_path=False, model=None, api_key=None, **kwargs):
        # Even if an inner layer tried to surface the key, the response
        # body would catch it. Answer intentionally benign.
        return {"answer": "safe answer", "output_path": None}

    with patch("llmwiki.web.query", side_effect=fake_query):
        c = _client(tmp_kb)
        r = c.post(
            "/api/ask",
            json={"question": "hi", "deep": False},
            headers={"X-LLM-Key": KEY},
        )
    assert r.status_code == 200
    assert KEY.encode() not in r.data


def test_api_ask_logs_do_not_contain_key(tmp_kb, monkeypatch, caplog):
    monkeypatch.delenv("LLMBASE_API_SECRET", raising=False)
    KEY = "sk-this-must-not-leak-log-xyz"

    def fake_query(q, **kwargs):
        return {"answer": "ok", "output_path": None}

    with caplog.at_level(logging.DEBUG, logger="llmbase"), \
         patch("llmwiki.web.query", side_effect=fake_query):
        c = _client(tmp_kb)
        c.post(
            "/api/ask",
            json={"question": "hi", "deep": False},
            headers={"X-LLM-Key": KEY},
        )

    for rec in caplog.records:
        assert KEY not in rec.getMessage(), rec


def test_chat_redacts_key_in_error(monkeypatch):
    """If the OpenAI client raises with the key echoed back, the retry
    warning and re-raised exception must not carry the key."""
    import llmwiki.llm as llm_mod

    KEY = "sk-echo-key-in-error"
    # Zero fallback retries: go straight from one primary attempt to raise.
    monkeypatch.setenv("LLMBASE_PRIMARY_RETRIES", "1")
    monkeypatch.setenv("LLMBASE_FALLBACK_MODELS", "")

    def boom(messages, model, max_tokens, api_key=None):
        # Simulate upstream echoing a key prefix into the error text.
        raise RuntimeError(f"Incorrect API key provided: {KEY}")

    monkeypatch.setattr(llm_mod, "_call_llm", boom)

    with pytest.raises(RuntimeError) as excinfo:
        llm_mod.chat("hi", model="m", api_key=KEY)
    assert KEY not in str(excinfo.value)
    assert "[redacted]" in str(excinfo.value)
