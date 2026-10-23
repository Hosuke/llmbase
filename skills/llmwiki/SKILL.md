---
name: llmwiki
version: "0.10.0"
description: "Use LLMBase to ingest sources, compile a linked knowledge library, retrieve evidence, and answer questions through CLI or MCP. Supports the native Rust runtime and the Python compatibility package with configurable language profiles."
author: Hosuke
homepage: https://github.com/Hosuke/llmbase
source: https://github.com/Hosuke/llmbase
license: MIT
keywords:
  - knowledge-base
  - wiki
  - research
  - mcp
  - multilingual
install: "pip install llmwiki"
requires:
  credentials:
    - name: LLMBASE_API_KEY
      description: "Credential for the configured generation endpoint; unnecessary for offline lexical search"
      required: false
    - name: LLMBASE_BASE_URL
      description: "HTTP-compatible generation endpoint"
      required: false
    - name: LLMBASE_MODEL
      description: "Generation model"
      required: false
  permissions:
    - network: "Requested URL/corpus ingestion and configured model calls"
    - filesystem: "Library files, SQLite revision and job records, and configured migration destinations"
    - server: "Optional HTTP services; MCP uses stdio"
  notes: |
    Model requests transmit selected library context to the configured endpoint.
    URL and corpus ingestion contact their source sites. Servers and scheduled
    workers are opt-in. Public reads do not authorize writes or model calls.
---

# LLMBase

> Version 0.10.0 packages the Rust-backed Python compatibility engine. Flask
> factories, Python callbacks and scheduling remain supported boundaries.
> Consult the release guide and compatibility matrix for the separate native
> executable and remaining all-native migration work.

LLMBase keeps original sources, accepted knowledge articles, revisions, aliases,
and backlinks. The native executable runs without Python. The Python package
retains `llmwiki.*`, live overrides, callbacks, Flask extensions, and corpus
connectors. Both installations can provide `llmbase`; use an explicit executable
path when the distinction matters.

## Choose the interface

- Use native `ask` for evidence-first answers from raw sources, compiled articles,
  or both. It is read-only by default; `--stream` emits provider-backed events.
- Use `query` for the established compiled-wiki workflow. Its `--deep`,
  `--tone`, `--format`, and `--file-back` flags retain their distinct meanings.
- Use Python/Flask when a downstream project needs runtime callable overrides or
  custom routes. Native profiles do not execute arbitrary Python callbacks.
- MCP clients should discover tools and their current schemas. Missing model or
  plugin capabilities are explicit errors; do not interpret them as empty results.

Native MCP configuration, using the installed binary's actual path:

~~~json
{
  "mcpServers": {
    "llmbase": {
      "command": "/path/to/native/llmbase",
      "args": ["--base-dir", "/path/to/library", "--library-id", "example", "mcp"]
    }
  }
}
~~~

For the Python compatibility host, use `python -m llmwiki --base-dir /path/to/library`
with the intended virtual environment's interpreter.

## Search, answer, and compile

~~~bash
llmbase --base-dir /path/to/library index
llmbase --base-dir /path/to/library search raw "exact source phrase"
llmbase --base-dir /path/to/library search query "compiled concept"
llmbase --base-dir /path/to/library ask "What supports this claim?" --kind both
llmbase --base-dir /path/to/library query "Explain this concept" --tone wenyan
~~~

When answering, distinguish retrieved candidates, consulted articles, exact
quoted evidence, and the generated inference. A resolvable quote does not alone
prove the inference. Preserve the library and source revision in citations;
report stale evidence rather than silently substituting current text.

Native `compile source SOURCE_ID` creates drafts. Inspect with
`compile draft DRAFT_ID`; accept with `compile apply DRAFT_ID`, supplying
`--expected-hash` for an existing target. Do not discard a hash conflict and
overwrite a newer edit. `compile new` is the accepted batch workflow.

Use `query --file-back` only when saving is intended. MCP `kb_ask` has its own
schema defaults; do not assume the HTTP, CLI, and MCP legacy defaults match.
Native `compile all` re-extracts all sources; the Python compatibility path
retains its documented legacy source-receipt behavior.

## Language and extensions

Native interfaces default to English. The Python factory default remains
English / 中文 / 日本語. Respect configured profiles and downstream overrides.
Classical Chinese bare-body content is supported through a single empty heading:

~~~python
import llmwiki.compile as compile_module
compile_module.SECTION_HEADERS = [("文言", "")]
~~~

Never reassign `DEFAULT_SECTION_HEADERS`; it is the identity anchor used to
detect overrides. Supported callable patches run at their documented Python
boundary. Process adapters are trusted operator-configured programs whose
returned documents pass through native persistence.

## Data and operation boundaries

- Source bytes and accepted knowledge are authoritative. SQLite also contains
  revision history, evidence, and job records; a Markdown export is not a backup.
- Use copy-first migration, then verification and a restore rehearsal. Keep the
  original until rollback is verified. Never infer a production switch from
  authorization to build or test a candidate.
- Supply model credentials through the environment or a secret manager, not a
  file, frontend value, committed configuration, or persisted job payload.
- HTTP callers need explicit authorization for writes and model calls. A visitor
  cookie does not confer those capabilities.
- Run long jobs on a server or supervised runner. Native `jobs run-one` performs
  one durable claim and exits; inspect job status/events before retrying a
  partial operation. Cancellation does not undo already accepted writes.
- Genuine embedding quality and external provider checks are unverified unless
  a report records actual runs. Synthetic fixtures establish engineering
  behavior only.

## References

Read the relevant document when selecting commands or managing a migration:

- [README and installation](https://github.com/Hosuke/llmbase#readme)
- [Native migration runbook](https://github.com/Hosuke/llmbase/blob/main/docs/rust-migration/runbook.md)
- [Compatibility matrix](https://github.com/Hosuke/llmbase/blob/main/docs/rust-migration/parity.md)
- [Customization contract](https://github.com/Hosuke/llmbase/blob/main/docs/customization.md)
- [Jobs and recovery](https://github.com/Hosuke/llmbase/blob/main/docs/rust-migration/jobs.md)
