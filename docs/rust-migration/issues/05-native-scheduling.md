# Rust: move supported worker schedules into durable per-library state

## Starting point
Native finite job execution exists, while legacy run_worker/start_worker_thread still implement scheduling and global custom registrations.

## Deliverable
Represent supported periodic schedules by library ID, schedule ID, handler ID, interval and next due time in durable state. Enqueue due jobs transactionally with a unique occurrence identity. Define missed-tick coalescing, backoff, disable/update behavior and clock handling. Provide a finite scheduler tick plus run-one workflow suitable for a supervised remote process. Translate supported legacy registrations at the compatibility boundary.

## Acceptance
Two concurrent ticks enqueue one occurrence, restart does not duplicate it, and two libraries with the same schedule name remain isolated. Failed/partial jobs follow issue 04 outcomes. Test disabled schedules, changed intervals, clock movement and cancellation using a controlled clock. Document exact replacement for CUSTOM_JOBS/LEARN_SOURCES and reject unsupported registrations clearly.

## Boundary and dependencies
Depends on 02 and 04. Own scheduling state, CLI entry and job tests. No always-running local watcher, second queue backend or deployment automation. Long-lived deployment remains an operator choice.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
