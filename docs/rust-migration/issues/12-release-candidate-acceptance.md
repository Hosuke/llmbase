# Rust: accept one immutable release candidate across recovery, performance and both sites

## Starting point
B16's 651 Python tests and older standalone/recovery proofs are useful but belong to different snapshots. They do not certify the final assembled engine.

## Deliverable
Build one immutable source commit on the isolated remote runner. Record source, binary and wheel hashes; use that same candidate for locked workspace fmt/Clippy/tests/release build, installed Python subset/full suite as applicable, frontend checks, no-Python CLI/HTTP/MCP/SSE lifecycle, and both downstream integration proofs. Reuse scripts/rust-migration harnesses, adding only missing meaningful checks.

Complete copy-first migration/verify/backup/restore/old-reader rollback and deterministic crash-window injection around durable intent, file replacement, SQLite publication and recovery. Record cold/warm latency, p95, RSS, disk growth and incremental indexing on fixed comparable data. Report retrieval quality by language; synthetic embeddings are not semantic-quality evidence.

## Acceptance
Publish machine-readable receipts tied to that commit. Preserve original source bytes, evidence identity (including deleted revisions), isolation and atomic visible generations after interruption. No gate is silently skipped; limitations and optional real-provider/embedding evaluation are explicit. All retained generic lifecycle features work without Python. Dual-site proof uses actual current entry code or documents a blocking missing input.

## Boundary and dependencies
Depends on 03–11. This is a release gate, not permission for paid API calls or production cutover. Long jobs use finite remote tmux sessions with logs/exits. Do not repeatedly rerun broad suites without a new change or unresolved failure.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
