# Native legacy query facade

`llmwiki.query` retains its public signatures and imported private helper names.
The default basic/deep query runs `llmbase_core::compat_query::run` through the
local PyO3 callback host. Rust owns index loading, context gathering, selector
sequencing, fallback selection, answer sequencing, conditional filing, promotion
decision validation, accepted article writing, and metadata rebuilding.

The native `ask` API remains the versioned streaming/evidence API. Compatibility
queries retain the established complete-answer return shapes, overlap retrieval,
title selector, `consulted` bookkeeping, and accepted promotion behavior. A title
in `consulted` records supplied context, not a claim of semantic support.

## Executable customization boundary

The host invokes the current Python `chat`, `chat_with_context`, system prompt,
tone map, format helper, promotion examples, `extract_json`, and language headers
at the appropriate stage. A selector callback can change the answer system prompt
before the answer stage. Per-query model/key values reach selector and answer;
the promotion judge uses default model configuration, as before. Secrets remain
in the host closure and the model adapter's memory; the core request, source
metadata, provenance, and receipt contain no model credential.

The default Python model functions perform actual Rust HTTP requests. Explicit
Python model overrides remain Python callbacks. Final receipts identify the Rust
orchestrator, host model boundary, stage sequence, and non-null retrieval/write
helper overrides. They do not relabel customized Python retrieval as native work.

`_bm25_prefilter` retains its historical name and TF-IDF formula. The default
tokenizer and ranking run in Rust. Live stopword sets are request data, and an
explicit Python tokenizer supplies tokens to Rust scoring. The pure prefilter
does not load configuration or create library files.

## Writes and deliberate corrections

Basic/deep defaults still have no filing side effect. Requested file-back writes
a scoped note through the shared source journal. Returned note paths are relative
to the configured base, or a filename for external configured roots. Same-second
filename collisions receive a random suffix instead of overwriting an answer.
The note records a validated public source snapshot; this is not a provider
receipt, citation proof, or entailment claim.

A writer session serializes each requested write workflow. Sources are captured
before model callbacks and checked against current indexed revisions and live
file bytes at persistence. Retrieval snapshot changes are rejected before answer
generation. A source becoming private during generation rejects the write.
Accepted promotion reuses the native compatibility compiler's merge, alias/CJK
deduplication, source metadata, guarded journal write, and metadata rebuild.
It creates or merges the actual article; it does not substitute a review draft
for the legacy accepted-write result. Private merge targets are rejected.

Default unauthenticated compatibility retrieval exposes only current public
sources. Private/internal/unknown visibility is excluded. Stale static index
text cannot bypass that rule: visible source metadata supplies summaries and the
context index is reconstructed from visible source files. This intentional
privacy correction can change selector context relative to old static files.
Local, explicit Python retrieval overrides remain caller-owned boundaries.

Malformed/non-object/declined promotion decisions do not write. A truncated
default judge response is rejected. Query DTOs and callback results are bounded
to 16 MiB; sources use the shared bounded document parser. Exceptions from model
callbacks propagate with their Python type unless legacy deep-query promotion
semantics turn a promotion failure into a declined result.

## Validation

The focused native suite covers sequence order, consulted titles, no implicit
save, journal filing, accepted promotion, bare language profiles, original source
bytes, private/current/stale filtering, mid-generation source changes, malformed
and truncated judges, callback failure, and Unicode/tokenizer ranking. Installed
wheel tests additionally use a loopback HTTP provider to prove actual native model
requests and verify live Python overrides and existing return contracts.
