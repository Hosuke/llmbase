# Rust: finish taxonomy, introduction and entity generation through shared services

## Starting point
Native catalog/entities/model services exist; B13 taxonomy read helpers are accepted. Legacy taxonomy generation/assignment, Xici and entities still retain orchestration or mutable paths.

## Deliverable
Define/reuse typed application commands for taxonomy generation and assignment, guided introduction generation, and entity extraction/CRUD. Keep model orchestration, validation, cache visibility, dedup and storage in Rust. Route supported Python entry points to these commands or give an explicit migration replacement. Prompt templates and language style are profile data; custom formatting/generation uses issue 02 interfaces.

## Acceptance
Cover English and bare-body wenyan, no model configuration, invalid/truncated model output, cancellation, stale apply, revoked private access and cache invalidation. Validate generated references against the authorized library. Failed generation cannot replace valid existing output. Compare representative supported legacy results without flattening arbitrary Python callback objects into JSON.

## Boundary and dependencies
Depends on 01/02 and uses 03 events. Own catalog/entities orchestration and focused fixtures. Do not redo B13 reads or add a separate taxonomy engine.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
