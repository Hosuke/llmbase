# Exact-byte atomic journal — 2026-09-26

Explicit-library atomic writes previously installed bytes without a recoverable
intent or retained before-image. They now commit a scoped exact-byte journal
before replacement, hold the shared writer and per-file locks, fsync/rename,
then acknowledge the intent. Normal library reopen completes pending writes
idempotently or preserves a conflicting external edit and its pending evidence.
Opaque metadata and invalid UTF-8 payloads are never implicitly indexed.

The additive `write_journal` table retains both byte images and hashes. Existing
schema-v1 source/revision data survives opening the new core. Journaled writes
refuse final-component symlinks before reading their referents. Standalone
writes keep their existing exact-byte/no-database behavior and `journal:false`.

## Scope and evidence

Parent checkpoint: `d50f7e3`. Only four implementation files changed:

- `crates/llmbase-core/src/compat_io.rs`
- `crates/llmbase-core/src/recovery.rs`
- `crates/llmbase-core/src/store.rs` (additive schema and open wiring)
- `crates/llmbase-core/tests/compat_io.rs`

The archive, patch and manifest reconstruct those exact four files. The broader
migration source remains largely untracked; this is a recovery checkpoint, not
a complete buildable release.

| Verification | Result |
| --- | --- |
| Pre-task implementation with changed compatibility regressions | exit 101; 3 passed, 4 failed |
| Independent final full compat_io target | exit 0; 7 passed |
| Independent final core library unit target | exit 0; 24 passed |
| Independent scoped rustfmt | exit 0 |
| Local frontend typecheck and Python lint import | each exit 0 |
| Current remote/local source provenance | all 108 hashes match |
| Fresh archive/patch replay | passed; four reconstructed hashes match |

Primary review requested three missing acceptance proofs before accepting the
first passing implementation: a nonempty overwrite with both byte/hash images,
opening a populated pre-journal schema-v1 database, and recovery under a held
per-file lock. The delegated revision added these tests without changing the
reviewed production implementation. The final tests also cover absent versus
empty files, simulated crash windows, repeated reopen, external-edit conflict,
cross-library records, escaping/reserved paths and retained standalone behavior.

Independent runner: `evidence/verify.sh`, executed in
`baypier-prod:/root/llmbase-rust-migration-20260920/b4-atomic-journal-source`.
See `evidence/b4-accept-source-sha256.json` and individual raw logs/exits. The
initial shared-target build reused stale baseline artifacts and is explicitly
invalid; its failed log is preserved. Later delegated runs and independent
acceptance force a fresh core build. This is not an unexplained waived failure.

Direct primary-agent review was authorized by the user; see `evidence/review.txt`.
Implementation, revision and task-specific remote test sessions have exited.
No downstream worktree, production data, version or publication was changed.

## Limits and next task

Crash windows use deterministic durable pending-intent fixtures. Actual process
kill/power-loss testing and full workspace/release gates remain outstanding.
Conflicted intents fail open until resolved; both images stay available. The
explicit-library journal rejects non-UTF-8 filenames; standalone filenames are
unchanged. No final wheel or whole-migration acceptance is implied here.

Next comes a fresh installed candidate and isolated Siwen/Taixu adoption proof,
ahead of remaining paragraph/config/facade migration. The user authorizes bounded
redesign of demonstrably unreasonable internals while preserving original data
and necessary downstream contracts. Production cutover remains unapproved.

## Replay

Extract `baseline-files.tar.gz` into a fresh scratch directory, run
`git apply --check /absolute/path/to/change.patch`, then apply the same patch.
Compare reconstructed files with the `after` entries in `files-sha256.json`.
Building still requires the full migration checkout.
