# Native evidence-first ask

`llmbase_core::ask` implements read-only retrieval followed by genuine provider
streaming. It does not save an answer, create a concept, promote a note, or edit a
source. The shared application must authorize library scope and paid-model
capability before starting it. Saving/compile/promote are separate operations.

## Public interface

- `AskModels { generation: Client, embedding: Option<Client> }` is in-memory
  trusted configuration; it is not a serializable request and carries no keys
  into transport events.
- `AskAccess { include_private: bool }` is a trusted application grant. It does
  not implement Deserialize. `AskRequest` rejects unknown fields, so a remote
  caller cannot add `include_private` to grant itself access.
- `start_ask(config, models, access, request, CancellationToken)` returns
  `Result<AskStream>`. Scope and input validation occur before task creation.
- `AskStream::recv()` yields `Option<AskEvent>`, `cancel()` cancels a child token,
  and Drop cancels/aborts the supervised task. HTTP response-body Drop should own
  and drop this handle to propagate disconnection.
- `collect_ask(...)` and `AskStream::collect()` use exactly the streaming pipeline
  and return the original typed terminal error or `AskResponse`.

Request defaults are raw sources, 8 candidates, 8192 evidence-context bytes
expressed as a conservative token estimate, and 2048 maximum generation tokens.
`AskKind` permits raw, compiled, or both; both excludes private notes unless a
future explicit note scope is introduced. Query and context bounds are validated;
`top_k` is at most 100. Session/request IDs are bounded ASCII identifiers and do
not confer library authorization or persist a conversation.

Native answer language defaults to English. Optional `answer_language` is a
generic language/profile code such as `en`, `zh`, or `wenyan`; no downstream
persona is hardcoded. `short_answer` and `max_sentences` are prompt preferences,
not destructive post-generation truncation. Original quotations retain their
bytes and language even when the answer language changes. The legacy Python
entry points retain their separately documented language defaults.

## Event schema and lifecycle

Every event has `version: 1`, request_id, session_id, and monotonically increasing
`sequence` starting at zero. The flattened `type` selects:

| type | Payload / meaning |
| --- | --- |
| progress | stage = retrieval or generation; never an answer-token event |
| evidence | retrieved candidates, labeled context evidence, retrieval mode/degradation, context budget |
| delta | exact provider answer-content delta, forwarded immediately |
| usage | provider token usage, with missing values remaining unknown |
| done | complete `AskResponse`, including truncation and citation metadata |
| error | safe code/message and `partial` indicating prior answer deltas |

The channel holds at most 16 events. Retrieval runs in `spawn_blocking`; the
thread owns its non-Sync SQLite Library and uses the caller's runtime handle for
optional real embedding I/O. Generation awaits `llmbase-model::ChatStream`
directly. No full-answer buffering precedes delta delivery, and no synthetic
splitting creates false streaming. Provider reasoning deltas are excluded from
answer output and first-token timing. A reasoning-only legacy fallback cannot
silently masquerade as a streamed native answer.

Cancellation drops active provider futures/streams. Drop aborts the supervisor
and child-cancels any retrieval worker, whose bounded local/HTTP work observes
that token. Provider timeouts and malformed/partial streams emit an error and
never a done event. A length finish returns the actual prefix with
`generation.truncated = true`. Network truncation remains an error.

## Evidence and context semantics

Retrieval can use the actual hybrid index when an embedding client is configured;
otherwise the lexical mode and degradation are explicit. Raw and compiled
filters, library ID, and trusted visibility grant apply before model context.
Candidates are resolved against retained original revisions before use.

Evidence is sent as escaped JSON, with a domain-neutral system instruction that
source text is untrusted data and cannot supply instructions. Only whole,
resolvable passage evidence fits into context; an oversized passage is skipped
rather than changing its span or fabricating a new citation ID. The context JSON
has a strict byte budget. One UTF-8 byte per estimated token is conservative for
common byte tokenizers but is not a provider-specific exact token count. System
instructions/question overhead and the provider's actual context limit are
separate from the evidence-only budget. If no passage fits, the request fails
with insufficient evidence before a generation call.

`AskResponse` keeps three separate lists:

1. `candidates`: passages returned by retrieval;
2. `context_evidence`: whole passages actually included in the bounded prompt,
   labeled E1, E2, etc.;
3. `used_evidence`: only labels explicitly present as `[E1]`-style markers in the
   answer, re-resolved against the original revision at completion.

Unknown labels are preserved as `unknown_citations`; they are not converted into
valid references. Repeated labels are deduplicated. Used evidence includes stale
and deleted status at completion. The citation semantics field explicitly says
`model_markers_only_not_entailment`: marker presence and exact quote resolution do
not prove a generated inference is supported. Independent answer-support review
is a separate evaluation gate.

## Verification

`tests/ask.rs` drives the real native adapter via ephemeral loopback HTTP fixtures:
evidence arrives before generation; the provider withholds finish until the
first delta is consumed; cancellation and handle Drop close the provider TCP
connection; timeout and malformed midstream data cannot emit done; original
evidence remains resolvable and becomes stale after a concurrent source edit;
actual citation markers differ from retrieved/context lists; length finish,
English default, wenyan override, visibility, input grants, and no-save semantics
are checked. Tests contain synthetic Unicode including a non-BMP character.

```sh
cargo test -p llmbase-core --test ask
```

The focused run exited 0: 6/6 HTTP fixture integration tests passed in 0.12 s
(`/tmp/llmbase-ask-tests.log`, `/tmp/llmbase-ask-tests.exit`). Real provider quality,
faithfulness/entailment assessment, end-user latency percentiles, and remote
transport deployment are not claimed by these fixture tests.
