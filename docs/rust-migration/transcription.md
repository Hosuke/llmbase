# Device transcription boundary

The native transcription entry accepts supplied UTF-8 text and passes it unchanged
to the existing evidence-bearing ask flow. It does not perform OCR, operate a
device, interpret device display commands, or save an answer by default. The
fixed handwriting fixture exercises this actual native entry and the real model
HTTP/SSE adapter using scripted loopback responses.

`TranscriptionRequest` requires version `1`, a transcription ID, library ID,
session ID, and `input_kind`. Supplied kinds are `supplied_transcription` and
`synthetic_transcription`; both require nonblank `text` of at most 16,384 UTF-8
bytes. Text is not trimmed, normalized, translated, or substituted with the
transcription ID. Optional answer preferences carry `short_answer` (default
true), `max_sentences` (1–100), and an answer-language/profile code. English
remains the native default; explicit `wenyan` or another valid code passes to ask.
Sentence limits are generation preferences, not destructive output truncation.

The independent `image` and `handwriting` input variants return `Capability`
before any provider call because no recognition adapter is configured. An
optional `image_base64` field describes that unsupported boundary; it is bounded
at 8 MiB, is never decoded or sent to the generation provider, and cannot accompany
a supplied transcript. Request fields for filesystem paths, URLs, credentials,
private grants, or write policy are rejected. A future recognition host must be
implemented and declared explicitly; this entry never returns fabricated OCR.

The core interfaces are:

```rust
prepare_transcription(expected_library_id, &request) -> Result<PreparedTranscription>
start_transcription_ask(config, models, access, request, cancel) -> Result<TranscriptionStream>
collect_transcription_ask(config, models, access, request, cancel)
    -> Result<TranscriptionResponse>
```

Preparation is pure: it validates the scope/identifiers/preferences and returns
the mapped `AskRequest` with a receipt containing the input hash, byte/scalar
counts, caller-declared input kind, library/session/transcription IDs,
`ocr_performed=false`, and `write_policy="none"`. The input-kind label is not an
attestation about how the caller obtained the text. A transcription ID identifies
the submitted input, not a source, source revision, or evidence passage.

`TranscriptionStream` owns the real `AskStream`. Its events add `transcription_id`
to the existing version, request ID, session ID, sequence, progress, evidence,
provider delta, usage, done, and error fields. It does not collect and re-chunk a
completed answer. Dropping the wrapper drops/cancels the underlying stream;
explicit cancellation uses that same provider token. Collected output contains
the transcription receipt and the unmodified structured `AskResponse`. Source
indexing is a separate ingest/index operation; an unindexed library has no
retrievable evidence. Opening an ask can initialize the internal database;
`write_policy="none"` means that it creates no saved note or compiled content.

`Application::prepare_transcription(principal, request)` authorizes library Read
before preparation. `Application::start_transcription_ask(principal, models,
request, cancel)` requires Read and Model, then delegates to the existing native
ask entry. Its private access comes only from the trusted principal, and it needs
no Write grant for a read-only answer. Provider clients and credentials remain
separate from the serializable request. Transport adapters call this shared
application boundary.

The frozen input is
`tests/rust_migration/fixtures/downstream/transcription.json`, with the associated
custom-root library fixture. It maps `學而時習之，出於何篇？` to the exact question,
session, library, short-answer preference, and two-sentence preference. The test
validates the real source path, recorded quote bytes, original URL/chapter,
revision resolution, preserved non-BMP/combining characters, private-context
exclusion, and absence of saved notes. Additional cases check pure preparation,
strict input validation, cross-library/model/read denials before HTTP or indexing,
unsupported image/handwriting input, explicit cancellation, and stream-drop
disconnection. Scripted generation proves execution/contract behavior only; it
does not establish OCR accuracy, provider answer quality, or entailment.

The Riddle checkout was inspected read-only. Its current oracle accepts page PNGs
and emits its own `Ink`, `Show`, and `Transcript` events, with `⟦show:N⟧` and `⁂`
as private display/transcription syntax. The new knowledge-library API does not
interpret those directives or adopt its persona. No Riddle source, font, display
driver, hardware deployment, or live oracle was changed. A device client still
needs an explicit adapter to consume the new structured contract.

Validation passed: `cargo test -p llmbase-core --test transcription` ran five tests
in 0.07 seconds, and `cargo clippy -p llmbase-core --all-targets -- -D warnings`
completed successfully. Logs and exit-0 receipts are
`.rust-migration-runs/transcription-gate-v2.*` and
`.rust-migration-runs/transcription-clippy-v2.*`. The fixture is indexed explicitly
in test setup through the normal native index operation; its source text is not
changed to satisfy retrieval.

The [actual execution receipt](results/transcription-acceptance-v1.json) records
the frozen input, mapped question, real outgoing generation request, all seven
stream events, final answer/citations, and byte/privacy/no-save checks. Its SHA-256
is `aed039c1769a532152426e74004cf47a2656e4c7a52d998e363156199fc873b6`;
the unchanged input fixture hash is
`6949ac5dfde79852bb767a25faaab5ae052a41b8b6c77c54bccf3edbac4b7824`.
