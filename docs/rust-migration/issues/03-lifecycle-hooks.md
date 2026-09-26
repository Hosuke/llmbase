# Rust: define and emit the ten library-scoped lifecycle events

## Starting point
The legacy event table is in `CLAUDE.md` and `llmwiki/hooks.py`; native lifecycle coverage is incomplete. Extension callbacks must not depend on process-global Python lists.

## Deliverable
Own the native event types/dispatcher and calls at the relevant application-service boundaries. Cover ingested, before_compile, compiled, after_compile_batch, index_rebuilt, taxonomy_generated, after_lint_check, after_auto_fix, xici_generated and entity_extracted. Define schema version, library ID, operation ID and event-specific payloads, with no credentials or private document content in generic event records.

Specify a concrete failure policy: before_compile failure aborts before mutation; post-commit observer failure is recorded separately and cannot falsely roll back or relabel a committed write. Define ordering and retry delivery semantics; do not claim exactly-once delivery unless implemented.

## Acceptance
A collecting external extension verifies payloads and order for success, no-op, partial batch, cancellation and observer failure. Two-library tests prove isolation. Failure after a committed write preserves both committed data and a visible observer error. Map every old event to the v1 event or an explicitly documented replacement.

## Boundary and dependencies
Depends on 02. Reuse durable job/event infrastructure; do not add a message broker. Python adapters, when retained, translate these events rather than becoming a second authoritative emitter.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
