# Native model adapter

`crates/llmbase-model` owns outbound generation and embedding HTTP. It has no
Python dependency, subprocess path, mock production backend, filesystem access,
environment loader, or hardcoded provider/model selection. `base_url` and model
names come from trusted application configuration. The application must authorize
paid operations before calling this crate; provider configuration must not be
accepted directly from an untrusted remote request.

## Interface

```rust
use llmbase_model::{CancellationToken, ChatRequest, Client, ModelConfig};

async fn example() -> Result<(), llmbase_model::ModelError> {
    let client = Client::new(ModelConfig::new(
        "https://models.example/v1", "operator-selected-model",
    ))?;
    let cancel = CancellationToken::new();
    let response = client.complete(ChatRequest::new("Question"), cancel.clone()).await?;
    assert_eq!(response.truncated, response.finish_reason.as_deref() == Some("length"));
    let mut stream = client.stream(ChatRequest::new("Question"), cancel)?;
    while let Some(event) = stream.recv().await {
        let event = event?;
        // Application adds request_id, version, sequence, progress, and evidence.
        // Forward Delta immediately; Done contains the accepted final metadata.
        std::hint::black_box(event);
    }
    Ok(())
}
```

The example has a deliberately non-routable example endpoint and is not a smoke
test. Credentials are passed as `config.api_key = Some(SecretKey::new(key))` from
an existing in-memory credential source. The crate does not save or print them.

| Item | Contract |
| --- | --- |
| `ModelConfig::new(base_url, model)` | Explicit operator endpoint/model; memory-only optional credential; timeouts, limits, retry policy, explicit fallback models, and streaming capability |
| `ChatRequest::new(prompt).with_system(system)` | Typed role/content messages, optional model, max_tokens, optional temperature |
| `Client::complete(request, CancellationToken)` | Async `Result<ChatResponse, ModelError>` |
| `Client::stream(request, CancellationToken)` | Immediate `Result<ChatStream, ModelError>`; requires caller's Tokio runtime |
| `ChatStream::recv()` | Async `Option<Result<StreamEvent, ModelError>>`; receiver is bounded |
| `ChatStream::cancel()` / Drop | Cancel child operation / abort supervised task and drop provider response |
| `Client::embed(EmbeddingRequest, CancellationToken)` | Async ordered embeddings, dimension, model, usage, attempts |
| `ChatResponse` | content, model, optional finish_reason, truncated, Usage, actual HTTP attempts |
| `StreamEvent` | Delta, separate ReasoningDelta, Usage, Done; terminal errors use Result::Err |

`Usage` preserves optional `prompt_tokens`, `completion_tokens`,
`reasoning_tokens`, and `total_tokens`. Nested provider reasoning token usage is
flattened. Unknown/missing token counts remain `None`, never fabricated zero.
Provider model identity is used when returned; requested identity is the fallback.
`EmbeddingResponse.embeddings` restores input order by provider index and rejects
duplicate/missing/out-of-range indices, inconsistent dimensions, and nonfinite
values. Generation fallbacks do not apply to embeddings: switching embedding
models silently would corrupt index-generation compatibility.

## Completion, retries, and cancellation

The only HTTP endpoints are `chat/completions` and `embeddings`, appended to the
configured API prefix. Requests use JSON and normal certificate verification.
Redirects are refused. Reqwest's built-in retries are disabled so the adapter's
attempt count covers every send it initiates. The default primary budget is three
attempts; explicit fallback models each get one attempt. No implicit fallback
model exists. Transient transport errors and HTTP 408/409/429/5xx retry with
exponential backoff; `Retry-After` seconds and HTTP dates are honored up to the
configured maximum delay. Other failures may move to an explicitly configured
generation fallback, but are not retried within the same model.

One wall-clock deadline covers the entire operation including fallback, backoff,
response consumption, and stream-channel backpressure. Connect and per-read
timeouts are separate. Cancellation wins when simultaneously ready and drops the
operation future, including any network request or backoff. Stream child-token
cancellation does not cancel the caller's parent token. A stream held without
being consumed can retain its bounded buffered events and terminal error; dropping
the handle always aborts its task.

Empty complete responses retry; exhaustion returns the last empty content and
metadata to preserve the legacy primitive's behavior. Compile/apply callers must
reject an empty answer and decide truncation policy. A provider `length` finish
reason returns prefix text with `truncated: true`, never a false complete answer.
`reasoning_content_fallback` defaults to the existing Python behavior for complete
responses. Streaming reasoning is emitted separately as `ReasoningDelta`; only
the final assembled response may use reasoning as its legacy fallback when no
answer content exists. Do not treat reasoning events as answer first-token time.

## Real streaming and bounds

Generation uses provider `stream: true` and `stream_options.include_usage: true`
(the latter is configurable for compatible servers). The response must declare
`text/event-stream`. Each provider content delta is emitted while reading its
response. No completed answer is sliced into simulated deltas. Providers without
streaming return `StreamingUnsupported`.

The parser accepts arbitrary network splits, including inside UTF-8 scalars,
CRLF, JSON, and `[DONE]`. SSE comments are ignored, data lines are joined, and LF,
CRLF, or CR boundaries work. Total response bytes and each SSE frame have separate
limits. Body JSON parsing also has a total byte limit; JSON nesting uses
serde_json's normal recursion limit. Request input bytes and serialized bytes are
bounded. Defaults: 8 MiB request, 16 MiB response, 1 MiB frame, 16 buffered events.

The stream must contain choice zero and `[DONE]` before success is emitted. EOF
without completion, malformed events, provider errors, timeouts, or cancellation
terminate with an error after any already-delivered deltas. Missing final usage
remains unknown. No retry occurs after successful streaming headers: replaying
partial text could duplicate output and a second paid request. The gated fixture
holds the provider's second chunk until the consumer has received its first delta,
demonstrating genuine incremental delivery.

## Compatibility and security boundaries

`llmwiki.llm.chat` / `chat_with_meta` facade integration is an application concern;
this crate supplies the real primitive and metadata. The facade must read dynamic
overrides and per-call credentials each time and retain public Python return
types. It may not rely on the Python model client to execute these native calls.
The adapter does not cache caller credentials globally. A `Client` may be shared
only within the identity/configuration scope which constructed it.

`SecretKey` has a redacted Debug implementation and no Display/Serialize. Client
and configuration Debug omit endpoint and model fields, even before validation.
URLs containing credentials/query/fragment are rejected. Authorization header
values are marked sensitive. Error types contain only fixed diagnostics, HTTP
status, and attempt count; neither provider error bodies nor reqwest error text
enter logs, returned errors, or source chains. Successful generated text remains
untrusted application data, not commands or paths. Provider endpoint routing and
system proxy settings are operator-level trust, not a source-fetch SSRF API.

Retries cannot guarantee exactly-once billing: a transport failure can occur after
provider acceptance. Cache/receipt persistence and the external-success/local-write
crash window belong to the application/indexing transaction contract.

## Verification

The workspace pins Rust 1.98.0; this crate declares Rust 1.98 (edition 2024).
Reqwest 0.12.28's own manifest declares Rust 1.64.0, but that alone does not prove
the MSRV of the full resolved dependency graph. The earlier 1.85 candidate is not
advertised as verified. Workspace `Cargo.lock` is the authoritative dependency
resolution.

```sh
cargo fmt --all -- --check
cargo test --locked -p llmbase-model
cargo clippy --locked -p llmbase-model --all-targets -- -D warnings
```

Tests use ephemeral loopback TCP servers, synthetic prompts and responses, and
nonsecret fixture credentials. They execute the production HTTP adapter and cover
non-2xx, 429, retry exhaustion, explicit fallback, empty metadata, malformed JSON,
wall timeout, backoff timeout, cancellation, embeddings, truncation, Unicode and
bytewise SSE, first-delta gating, partial failure, unsupported streaming, byte
limits, redirects, and credential redaction. `model-first` completed with exit 0:
2 SSE unit tests and 19 HTTP fixture tests passed; the HTTP suite took 0.07 s.
Raw output: `.rust-migration-runs/model-first.log`. Final review verification also
exited 0 after the retry policy, endpoint/model identity accessors, and lint fixes:
2 SSE unit tests plus 19 HTTP integration tests passed (HTTP suite 0.08 s), with
raw output at `/tmp/llmbase-model-final.log` and exit code at
`/tmp/llmbase-model-final.exit`. Real paid-provider smoke and semantic quality:
**not run / unverified**.

The later scoped-override gate passed 20/20 HTTP tests in 0.07 s at
`/tmp/llmbase-workflow-privacy.log`. `Client::scoped_overrides` clones the existing
operator configuration and changes only an optional model and in-memory key.
It cannot change the provider endpoint. An explicit model override clears the
operator fallback list. Actual HTTP fixtures verify the scoped Authorization
header/model, unchanged original client, fallback isolation, and redacted Debug.

Protocol references consulted on 2026-09-20:

- [Official Chat Completions streaming schema](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events/): choice deltas, finish reason, and final optional usage.
- [Official embeddings API](https://developers.openai.com/api/reference/resources/embeddings): indexed embedding vectors and usage.
- [Reqwest ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html): connect/read timeout, redirects, sensitive headers, and built-in retry policy.
- [Tokio CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html): child token and cancellation behavior.
