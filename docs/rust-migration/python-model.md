# Python model compatibility boundary

In a native candidate installation, the default `llmwiki.llm._call_llm` sends HTTP
through `llmbase_model::Client::complete` using the local PyO3 `model_complete`
bridge. Python's OpenAI/httpx implementation is not the production HTTP path.
`chat`, `chat_with_meta`, and `chat_with_context` retain their existing signatures,
return shapes, and live callable override behavior.

## Preserved contracts

The Python facade keeps the outer retry/fallback loop because that loop is a
public extension boundary: downstream can replace `_call_llm`, `get_client`,
`get_default_model`, `get_fallback_models`, `chat`, or a custom `_client` at runtime.
Each default `_call_llm` invocation performs exactly one native adapter attempt.
The facade still retries exceptions/empty responses with the current primary and
fallback budgets. A replacement callable actually executes; it is never silently
serialized, ignored, or described as Rust execution.

`LLMMeta.attempts` counts `_call_llm` invocations. `LLMMeta.model` remains the
configured attempted model, including a successful fallback, rather than a
provider-returned alias. `finish_reason`, `truncated == (finish_reason == 'length')`,
and the flat usage fields are preserved. Missing usage values remain `None`.
Nonempty length-cut output is returned with metadata, not raised. Exhausted empty
responses return an empty string and the final observed metadata. Reasoning text
falls back into content for providers that omit ordinary answer content.

Native Rust application paths independently retain their bounded native retry,
backoff, streaming, and cancellation implementation. This compatibility change
does not claim all Python retry or extension-host logic has disappeared. It also
does not preserve the old SDK's hidden additional transport retries: one facade
attempt now means one native HTTP attempt, making request counts explicit.

`get_client` remains an SDK accessor for callers that explicitly request its
singleton/per-request behavior. An unchanged default accessor and default
singleton do not force ordinary chat calls back through the SDK. Replacing the
accessor, OpenAI constructor, or singleton intentionally invokes the Python
client extension boundary. Only an unbuilt development checkout may use its
legacy default implementation. A candidate with a missing/unloadable extension
fails; a native HTTP/configuration error never triggers Python HTTP fallback.

## Live configuration and secret handling

Default native calls read endpoint, model helpers, timeouts, and credentials at
each invocation. Explicit per-request keys override environment keys without
entering the default SDK singleton. Finite positive timeout validation rejects
NaN/infinity as invalid environment values and uses the documented defaults.

The PyO3 bridge receives nonsecret configuration/request JSON plus a separate
optional in-memory credential argument. It bounds inputs, validates the model
configuration, releases the GIL for the native HTTP request, and returns response
metadata with a native receipt. It does not open a library, write a profile/job,
persist credentials, or expose an arbitrary-provider operation on HTTP/MCP.
Provider error bodies and raw transport errors are not returned by the native
adapter. The Python extension boundary redacts both per-request and environment
keys from error logs and propagated messages.

## Execution evidence and tests

`_native.last_trace()` is set only by a completed native bridge call. A model
receipt identifies `llmbase_model::Client::complete`, actual HTTP attempts, and a
process-local completion sequence. `llm.last_call_trace()` additionally reports
an explicit Python callable/client boundary or failed native call. Entering a
Python override clears an earlier native receipt, preventing stale native traces
from misrepresenting a later callback execution. Receipts contain no keys.

The installed-candidate fixture suite starts ephemeral loopback HTTP providers
and rejects Python `httpx.Client.send` on the default path. It verifies exact
Unicode/system prompts, default budgets, actual Rust receipts, usage/reasoning/
truncation, fallback/empty accounting, changing environment/model functions,
uncached per-request credentials, live callable/client replacements, credential
redaction, bad JSON, timeout, and missing-extension failure. Existing LLMMeta,
X-LLM-Key, timeout, and context-prompt contracts remain separate regression gates.

```sh
LLMBASE_REQUIRE_NATIVE=1 python -m pytest --import-mode=importlib \
  tests/rust_migration/test_native_model.py tests/test_llm_meta.py
```

Run from outside the source checkout against the installed candidate wheel; an
editable/source import alone cannot establish native packaging coverage. Tests
use synthetic local responses and establish no real-provider answer quality.

The v5 installed candidate matched the source `llm.py` SHA-256 and passed all 92
selected native-model, metadata, search/KB, and writer-boundary tests in 1.87 s.
The earlier v4 wheel exposed an outdated pending/failed trace marker; v5 includes
the source fix and passed those assertions.
