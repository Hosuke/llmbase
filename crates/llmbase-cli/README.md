# Native transports

This crate provides the standalone `llmbase` executable, the two HTTP API
surfaces, and an MCP stdio server using the official Rust MCP SDK. All implemented
operations execute through `llmbase-core::application::Application`.

This is an incremental migration. The frozen contract inventory is in
[`docs/rust-migration/parity.md`](../../docs/rust-migration/parity.md). Discovery
lists available native MCP tools; known unimplemented legacy operations return
an explicit capability error. It does not report simulated successful work.

## CLI

```sh
llmbase --base-dir /path/to/library --library-id research --json op index --args '{}'
llmbase --base-dir /path/to/library --library-id research --json op search --args '{"query":"learning","kind":"raw","top_k":10}'
llmbase --base-dir /path/to/library --library-id research query 'What does the source say?' --stream
llmbase --base-dir /path/to/library --library-id research mcp
```

`op` prints the core JSON payload. Errors go to stderr with nonzero exit status:
invalid arguments use 2, contention uses 3, and other operation failures use 1.
`query --stream` emits structured JSONL events as generation proceeds and returns
nonzero on terminal error, including failure after a partial answer. Native query
is read-only, defaults to raw retrieval and English, and accepts
`--answer-language`, `--short-answer`, and `--max-sentences`.

The MCP command writes protocol messages only to stdout. `--library ID=PATH`
can configure additional libraries for HTTP; local CLI and MCP authority remains
limited to the explicitly selected `--library-id`.

## HTTP

`serve` uses the Agent envelope and port 5556; `web` uses the Web envelope and
port 5555. Both bind to `127.0.0.1` by default. `--public-read` permits anonymous
reads of only the default library. A cookie grants no write or model authority.

`LLMBASE_API_SECRET` grants owner authority for the default library. More specific
grants can be configured with `LLMBASE_AUTH_GRANTS`, a JSON array such as:

```json
[{"token_env":"RESEARCH_TOKEN","libraries":["research"],"capabilities":["read","model"]}]
```

The token itself is supplied through the named process environment variable;
request authentication uses `Authorization: Bearer …`. No credential file is
read or written. Optional `X-Library-Id` selects a configured library for routes
without a library in their request body. Every request still checks the grant.

Native endpoints:

| Method and path | Request/result |
| --- | --- |
| `POST /api/v1/op` | `{library_id, operation, args}` → versioned result envelope |
| `POST /api/v1/evidence/resolve` | `{evidence}` → current/stale source validation |
| `POST /api/v1/ask` | `AskRequest` → collected `AskResponse`; read + model authority |
| `POST /api/v1/ask/stream` | Same request → actual provider-backed SSE |
| `POST /api/v1/requests/{request_id}/cancel` | Only the initiating principal in the same library may cancel |

SSE events are `progress`, `evidence`, `delta`, `usage`, and one terminal `done`
or `error`. Each carries version 1, request ID, sequence, and optional session ID.
A request may specify an ASCII request ID, or the server generates one. A dropped
HTTP body cancels the upstream provider and releases its concurrency slot. The
server admits at most 16 concurrent operations/asks. SSE heartbeats are comments,
not generated answer text. No answer is written into a knowledge library.

Legacy routes currently include operation discovery/call, compiled search,
article list/get, sources, stats, and index rebuild. Agent and Web response
envelopes remain distinct. Full factory/SPA/customization parity is pending in
the contract matrix; native ask routes do not silently replace legacy `/api/ask`.

## Model configuration

Model endpoints and credentials are operator process configuration, never ask
body fields. The loader accepts:

| Variable | Meaning/default |
| --- | --- |
| `LLMBASE_BASE_URL`, `LLMBASE_MODEL` | Both required to enable generation |
| `LLMBASE_API_KEY` | Optional bearer key; `OPENAI_API_KEY` is a compatibility fallback |
| `LLMBASE_FALLBACK_MODELS` | Comma-separated model names on the same endpoint |
| `LLMBASE_PRIMARY_RETRIES`, `LLMBASE_FALLBACK_RETRIES` | Maximum attempts: 3 / 1 |
| `LLMBASE_CONNECT_TIMEOUT_SECONDS` | 30 |
| `LLMBASE_REQUEST_TIMEOUT_SECONDS` | 300 |
| `LLMBASE_READ_TIMEOUT_SECONDS` | 60 |
| `LLMBASE_STREAMING` | `false` or `0` explicitly disables provider streaming |
| `LLMBASE_EMBEDDING_MODEL` | Optional; absence uses lexical retrieval with reported degradation |
| `LLMBASE_EMBEDDING_BASE_URL` | Defaults to the generation endpoint |
| `LLMBASE_EMBEDDING_API_KEY` | Defaults to the generation key |

The adapter never turns provider reasoning into answer text. Providers without
streaming support are reported by the core adapter; completed text is not split
into fabricated deltas.

## Copy-first administration

```sh
llmbase --base-dir /original --library-id research migration dry-run
llmbase --base-dir /original --library-id research migration copy /separate/candidate
llmbase migration verify /separate/candidate
llmbase --base-dir /original --library-id research migration backup /separate/snapshot
llmbase migration restore /separate/snapshot /separate/restored
llmbase migration rollback /separate/snapshot
```

Dry-run inspects without creating an original database, directory, or lock.
Copy/backup/restore delegate to the verified core migration receipt and database
backup APIs. `rollback` verifies that the original remains preserved; it does not
switch production traffic or mutate an original. These commands are local-only;
arbitrary administrative filesystem targets are not accepted as HTTP operations.

## Verification

`cargo test -p llmbase-cli` covers a standalone subprocess with no inherited
Python environment, real MCP stdio initialization/tool execution/EOF, both HTTP
envelopes, library/private/write/model boundaries, and the migration lifecycle.
The ask suite runs two ephemeral real HTTP listeners and verifies early deltas,
citations, scoped cancellation, disconnect cleanup, malformed provider data,
timeouts, and read-only completion. Tests use deterministic local provider
fixtures and no production credentials.
