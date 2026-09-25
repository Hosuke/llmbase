# Worker contention checkpoint — 2026-09-26

This checkpoint saves one bounded repair to background maintenance jobs. The
job has already been claimed when a concurrent reader can briefly hold the
recovery writer lock. Previously the operation's `Library::open` returned Busy
and the worker persisted a failed job. The patch retries only that open, at
most 100 attempts separated by 20 ms, checking cancellation each time. The
repair body, model requests and accepted mutations are outside the retry loop.
The claimed job ID and lease are retained.

## Saved scope

The `rust-core-rebuild` worktree was based on
`a7c4d9da764462e09c0769ecd8ba548a46d27e83`; the Rust migration files were still
untracked. This commit saves the three affected original files in
`baseline-files.tar.gz`, the change in `change.patch`, and before/after hashes in
`files-sha256.json`. The working tree also contains the applied patch. The
archive is a recovery package for this task, not a complete buildable migration.

Changed files:

- `crates/llmbase-core/src/application.rs`: bounded pre-operation open retry and
  an opt-in, hidden synchronization hook for deterministic integration tests.
- `crates/llmbase-core/tests/worker.rs`: transient contention, exhaustion,
  cancellation, exact selected job and lease/event assertions.
- `crates/llmbase-cli/tests/http_model_contracts.rs`: deterministic background
  web repair contention, unrelated job remains queued, no unexpected provider
  calls, and the expected persisted repair result.

Implementation was delegated in a single bounded run. The run stopped at its
22-minute supervisor limit before producing a final report. The reviewer made
only formatter changes to the implementation, performed independent Linux
verification, and prepared this checkpoint. No second development task started.

## Verification and limits

The current uncommitted migration cannot compile because `document.rs:34`
references `crate::compat_yaml`, which is not exported in `lib.rs`. The initial
before/after commands all exited 101 at compilation; those are **not** evidence
that a regression test ran. The initial format check exited 1. Their raw logs
remain in `evidence/`.

For independent behavioral verification, the patch was applied cleanly to the
previous coherent `native-check-v2` source snapshot in a separate remote
directory, `/root/llmbase-rust-migration-20260920/k1-accept` on `baypier-prod`.
That historical snapshot and production libraries were not modified. The
historical source's Cargo target cache was reused. The exact final validation
source hashes are in `evidence/k1-final-source-sha256.json`; this test source is
distinct from the current migration worktree.

The reviewer first disabled only the retry, retaining the test synchronization
hook. The new transient-contention test then failed at the intended assertion:
`Failed` instead of `Succeeded` (exit 101, one test actually executed). Restoring
the patch passed all 10 worker tests and all 7 HTTP model contract tests,
including the original intermittent failure. The final formatted patch was
then verified with the same two suites and the three-file format check. Each
command has its own raw `.log` and `.exit` file; a wrapper's zero exit alone does
not establish success.

Commands (isolated Rust 1.98.0 environment, `CARGO_BUILD_JOBS=2`):

```sh
cargo test -p llmbase-core --test worker
cargo test -p llmbase-cli --test http_model_contracts
rustfmt --check --edition 2024 crates/llmbase-core/src/application.rs crates/llmbase-core/tests/worker.rs crates/llmbase-cli/tests/http_model_contracts.rs
```

The test scripts are included alongside the evidence. TypeScript
`npx --no-install tsc --noEmit` and Python
`python -c "from llmwiki.lint import lint; print('OK')"` passed locally.
Archive extraction, patch application and all reconstructed after-hashes were
independently checked in a fresh temporary directory.

The task has scoped behavioral evidence on the historical snapshot. Integration
acceptance of the current migration remains blocked by the unrelated compile
error. The new fixtures use repair paths with no model calls; nonzero-model
side-effect replay scenarios were not separately exercised. The hidden test
hook is compiled into the library and is unset by default. No complete workspace
suite, release build, production switch or publication was performed.

## Recover the exact changed files

Use a fresh scratch directory; the archive contains only the three affected
baseline files and does not contain the rest of the Rust workspace:

```sh
bundle=/absolute/path/to/worker-contention-2026-09-26
scratch=$(mktemp -d)
tar -xzf "$bundle/baseline-files.tar.gz" -C "$scratch"
git -C "$scratch" apply --check "$bundle/change.patch"
git -C "$scratch" apply "$bundle/change.patch"
```

The reconstructed files must match the `after` SHA-256 entries. To apply this
change to a full migration checkout, check the patch against that checkout and
run its integration checks; the scratch reconstruction alone is not buildable.
