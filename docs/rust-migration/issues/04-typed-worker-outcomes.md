# Rust: make worker outcomes explicit and stop treating swallowed errors as success

## Bounded progress — C3, 2026-10-10

[C3](../results/c3-worker-outcomes-2026-10-10/README.md) adds native `JobOutcome`,
rejects absent/null successful results, validates existing report statuses and
preserves Partial results through the fenced ledger. It tests a real finite
compile worker retrying the same job without rewriting accepted work. This is
the shared native boundary slice, independent of the future custom registry.
This issue remains open for structured error codes/retryability, normalized
artifact references, registered handlers and general quota/checkpoint resumption.

## Starting point
`crates/llmbase-core/src/worker.rs` already runs durable jobs. Legacy `llmwiki/worker.py` compile/learn helpers can catch errors and return None, which is not a reliable success signal.

## Deliverable
Introduce a typed handler outcome for completed, partial, failed and cancelled work, with artifact IDs, a stable error code and retryability where relevant. Connect built-in and registered handlers to one job-state transition path. Completed requires an explicit result; an absent/malformed handler result becomes a visible failure. Partial work keeps completed artifact/cache references for resumption.

## Acceptance
Faulting compile/learn/custom handlers cannot end in successful jobs. Partial quota exhaustion resumes from accepted work without duplicate writes. Cancellation and lease loss cannot be overwritten by a late success. Restart/retry retains the same library and job identity. Use deterministic fake providers; no paid model calls.

## Boundary and dependencies
Depends on 02 for custom handler registration. Own worker outcome types and focused tests; scheduling changes are issue 05. Preserve accepted contention/recovery behavior and existing receipts.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
