"""Real native HTTP adapter tests, intended for an installed candidate wheel."""
from contextlib import contextmanager
from dataclasses import asdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib
import json
import threading
import time
from types import SimpleNamespace

import httpx
import pytest

from llmwiki import _native, llm


@pytest.fixture(autouse=True)
def native_candidate(monkeypatch):
    if not _native.available():
        pytest.skip("native candidate extension is not built")
    monkeypatch.setattr(llm, "_client", None)
    monkeypatch.setattr(llm, "_default_client", None)
    monkeypatch.setattr(llm, "time", SimpleNamespace(sleep=lambda *_: None))
    monkeypatch.setenv("LLMBASE_PRIMARY_RETRIES", "1")
    monkeypatch.setenv("LLMBASE_FALLBACK_RETRIES", "1")
    monkeypatch.setenv("LLMBASE_FALLBACK_MODELS", "")
    monkeypatch.setenv("LLMBASE_HTTP_TIMEOUT", "2")
    monkeypatch.setenv("LLMBASE_HTTP_CONNECT_TIMEOUT", "1")
    monkeypatch.setenv("LLMBASE_API_KEY", "fixture-operator-token")
    _native.clear_trace()


def completion(content="Native response 原典𠮷", *, finish="stop", reasoning=None, usage=None):
    return {"model": "provider-reported-alias", "choices": [{"message": {
        "content": content, "reasoning_content": reasoning}, "finish_reason": finish}], "usage": usage}


@contextmanager
def provider(monkeypatch, responses):
    calls = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = self.rfile.read(int(self.headers["Content-Length"]))
            index = len(calls)
            calls.append({"path": self.path, "body": json.loads(body), "authorization": self.headers.get("Authorization")})
            status, response, delay = responses[index] if index < len(responses) else (500, b"unexpected fixture request", 0)
            if delay:
                time.sleep(delay)
            raw = response if isinstance(response, bytes) else json.dumps(response, ensure_ascii=False).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            try:
                self.wfile.write(raw)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
    thread.start()
    monkeypatch.setenv("LLMBASE_BASE_URL", f"http://127.0.0.1:{server.server_port}/v1")
    try:
        yield calls
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def reject_sdk(*_args, **_kwargs):
    raise AssertionError("default production HTTP entered Python httpx")


def test_native_http_preserves_prompt_defaults_metadata_and_records_real_trace(monkeypatch):
    monkeypatch.setattr(httpx.Client, "send", reject_sdk)
    usage = {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8,
             "completion_tokens_details": {"reasoning_tokens": 2}}
    with provider(monkeypatch, [(200, completion(usage=usage), 0)]) as calls:
        text, metadata = llm.chat_with_meta("原典𠮷 café", system="English; preserve quotations.", model="requested-model")
    assert text == "Native response 原典𠮷"
    assert asdict(metadata) == {"model": "requested-model", "finish_reason": "stop", "truncated": False,
        "usage": {"prompt_tokens": 5, "completion_tokens": 3, "reasoning_tokens": 2, "total_tokens": 8}, "attempts": 1}
    assert calls[0]["path"] == "/v1/chat/completions"
    assert calls[0]["body"]["messages"] == [{"role": "system", "content": "English; preserve quotations."}, {"role": "user", "content": "原典𠮷 café"}]
    assert calls[0]["body"]["max_tokens"] == 16384
    assert calls[0]["body"]["stream"] is False
    assert _native.last_trace()["adapter"] == "llmbase_model::Client::complete"
    assert llm.last_call_trace() == _native.last_trace()
    assert _native.last_trace()["http_attempts"] == 1
    assert "fixture-operator-token" not in repr(_native.last_trace()) + repr(metadata)


def test_reasoning_fallback_length_cut_and_unknown_usage_remain_visible(monkeypatch):
    with provider(monkeypatch, [(200, completion("  ", finish="length", reasoning="Reasoning answer"), 0)]):
        text, metadata = llm.chat_with_meta("question", model="reasoner", max_tokens=37)
    assert text == "Reasoning answer"
    assert metadata.truncated is True
    assert metadata.finish_reason == "length"
    assert metadata.usage == {"prompt_tokens": None, "completion_tokens": None, "reasoning_tokens": None, "total_tokens": None}


def test_legacy_retry_fallback_accounting_calls_native_once_per_attempt(monkeypatch):
    monkeypatch.setenv("LLMBASE_PRIMARY_RETRIES", "2")
    monkeypatch.setenv("LLMBASE_FALLBACK_MODELS", "fallback-model")
    replies = [(429, {"error": "fixture rate limit"}, 0), (200, completion(""), 0), (200, completion("partial", finish="length"), 0)]
    with provider(monkeypatch, replies) as calls:
        text, metadata = llm.chat_with_meta("question", model="primary-model")
    assert text == "partial"
    assert metadata.model == "fallback-model"
    assert metadata.attempts == 3
    assert metadata.truncated
    assert [call["body"]["model"] for call in calls] == ["primary-model", "primary-model", "fallback-model"]


def test_empty_exhaustion_preserves_last_response_and_bare_chat_contract(monkeypatch):
    monkeypatch.setenv("LLMBASE_PRIMARY_RETRIES", "2")
    with provider(monkeypatch, [(200, completion(""), 0), (200, completion("", finish="length"), 0)]) as calls:
        text, metadata = llm.chat_with_meta("question", model="empty-model")
    assert text == ""
    assert metadata.model == "empty-model"
    assert metadata.finish_reason == "length" and metadata.truncated
    assert metadata.attempts == len(calls) == 2
    with provider(monkeypatch, [(200, completion("bare"), 0)]):
        assert llm.chat("question") == "bare"


def test_live_configuration_model_helpers_and_per_request_key_never_cache(monkeypatch):
    monkeypatch.setattr(llm, "get_default_model", lambda: "runtime-model-one")
    with provider(monkeypatch, [(200, completion("first"), 0)]) as first:
        assert llm.chat("question", api_key="fixture-request-token") == "first"
    monkeypatch.setattr(llm, "get_default_model", lambda: "runtime-model-two")
    monkeypatch.setenv("LLMBASE_API_KEY", "fixture-new-operator-token")
    with provider(monkeypatch, [(200, completion("second"), 0)]) as second:
        assert llm.chat("question") == "second"
    assert first[0]["authorization"] == "Bearer fixture-request-token"
    assert second[0]["authorization"] == "Bearer fixture-new-operator-token"
    assert first[0]["body"]["model"] == "runtime-model-one"
    assert second[0]["body"]["model"] == "runtime-model-two"
    assert llm._client is None


def test_live_python_callable_and_client_overrides_execute_and_clear_native_receipts(monkeypatch):
    with provider(monkeypatch, [(200, completion("native"), 0)]):
        assert llm.chat("question") == "native"
    original = llm._call_llm
    monkeypatch.setattr(llm, "_call_llm", lambda *a, **kw: ("override-one", "length", {"total_tokens": 7}))
    assert llm.chat_with_meta("question")[1].truncated
    assert _native.last_trace() is None
    assert llm.last_call_trace()["boundary"] == "callable_override"
    monkeypatch.setattr(llm, "_call_llm", lambda *a, **kw: ("override-two", "stop", {}))
    assert llm.chat("question") == "override-two"
    monkeypatch.setattr(llm, "_call_llm", original)
    captured = []
    def create(**kwargs):
        captured.append(kwargs)
        return SimpleNamespace(choices=[SimpleNamespace(message=SimpleNamespace(content="client override"), finish_reason="stop")], usage=None)
    fake_client = SimpleNamespace(chat=SimpleNamespace(completions=SimpleNamespace(create=create)))
    monkeypatch.setattr(llm, "get_client", lambda api_key=None: fake_client)
    assert llm.chat("live prompt", model="override-model") == "client override"
    assert captured[0]["messages"][-1]["content"] == "live prompt"
    assert llm.last_call_trace()["boundary"] == "client_override"
    assert _native.last_trace() is None
    monkeypatch.setattr(llm, "get_client", llm._DEFAULT_GET_CLIENT)
    monkeypatch.setattr(llm, "_client", fake_client)
    assert llm.chat("direct singleton override") == "client override"
    assert captured[-1]["messages"][-1]["content"] == "direct singleton override"
    assert llm.last_call_trace()["boundary"] == "client_override"


@pytest.mark.parametrize("per_request", [False, True])
def test_native_provider_errors_redact_environment_and_per_request_credentials(monkeypatch, caplog, per_request):
    key = "fixture-request-token" if per_request else "fixture-operator-token"
    with provider(monkeypatch, [(401, {"error": f"Rejected credential {key}"}, 0)]) as calls:
        with pytest.raises(RuntimeError) as error:
            llm.chat("question", api_key=key if per_request else None)
    assert len(calls) == 1
    assert key not in str(error.value) + caplog.text + repr(llm.last_call_trace())
    assert "401" in str(error.value)
    assert _native.last_trace() is None
    assert llm.last_call_trace()["status"] == "failed"


@pytest.mark.parametrize("reply,timeout", [(b"malformed JSON", "2"), (completion(), "0.02")])
def test_malformed_json_and_timeout_fail_without_sdk_fallback(monkeypatch, reply, timeout):
    monkeypatch.setattr(httpx.Client, "send", reject_sdk)
    monkeypatch.setenv("LLMBASE_HTTP_TIMEOUT", timeout)
    with provider(monkeypatch, [(200, reply, 0.08 if timeout == "0.02" else 0)]):
        with pytest.raises(RuntimeError):
            llm.chat("question")
    assert _native.last_trace() is None


def test_missing_required_extension_cannot_activate_python_http(monkeypatch):
    monkeypatch.setattr(httpx.Client, "send", reject_sdk)
    original_import = importlib.import_module
    def missing(name, *args, **kwargs):
        if name == "llmwiki._core":
            raise ModuleNotFoundError("fixture missing extension", name=name)
        return original_import(name, *args, **kwargs)
    monkeypatch.setattr(importlib, "import_module", missing)
    monkeypatch.setenv("LLMBASE_REQUIRE_NATIVE", "1")
    with pytest.raises(_native.NativeCapabilityError):
        llm.chat("question")


def test_prompt_context_and_chat_override_remain_live(monkeypatch):
    calls = []
    monkeypatch.setattr(llm, "chat", lambda prompt, **kwargs: calls.append((prompt, kwargs)) or "extension answer")
    assert llm.chat_with_context("問𠮷\ud800", [{"path": "原典", "content": "context\ud800"}], system="runtime system") == "extension answer"
    assert "context?" in calls[0][0] and "問𠮷?" in calls[0][0]
    assert calls[0][1]["system"] == "runtime system"


def test_nonfinite_timeout_values_use_finite_defaults(monkeypatch):
    for value in ("nan", "inf", "-inf", "0", "not a number"):
        monkeypatch.setenv("LLMBASE_HTTP_TIMEOUT", value)
        assert llm._get_float_env("LLMBASE_HTTP_TIMEOUT", 300.0) == 300.0
