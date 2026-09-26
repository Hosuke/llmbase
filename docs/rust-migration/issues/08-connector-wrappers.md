# Rust: expose the seven connector workflows through the existing process adapter

## Starting point
The bounded process-adapter protocol and native ingest/journal already exist; specialized legacy CLI wrappers remain missing: wikisource-learn, wikisource-work, cbeta-learn, cbeta-status, cbeta-work, ctext-book and ctext-catalog.

## Deliverable
Map each command's arguments and result/progress semantics to a configured adapter and shared native ingest/jobs. Preserve source text, companion metadata, provenance and resumable work identity. Parser/network format code may remain an explicit Python adapter; the core owns scope, progress, storage and indexing. Inspect actual legacy help to record exact flags before implementation.

## Acceptance
Offline recorded fixtures cover each command, an unavailable adapter, invalid output, timeout, partial output and retry. Missing adapters return a named capability/configuration error, never empty success. Out-of-root paths and wrong-library writes are denied. Replaying a completed batch avoids duplicate sources and preserves source bytes.

## Boundary and dependencies
Depends on 04 for outcomes; use existing plugins.md protocol rather than inventing a second one. No live crawl, credentials, real corpus import or new shell-based command execution.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
