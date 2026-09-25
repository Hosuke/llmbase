# Reserved atomic-write paths — 2026-09-26

Explicit-library `atomic_write` could replace the engine database or a held
writer-lock inode. It now rejects engine DB/sidecar/lock names within configured
meta, and per-file lock/stage artifact names within every configured root,
before acquiring a writer session or creating files. The policy reserves ASCII
case variants on every platform. Parent aliases are resolved before validation.
Ordinary file writes and standalone `config=None` retain their existing behavior.

Primary review rejected the first implementation despite its six passing Linux
tests: on the local case-insensitive filesystem, `LLMBASE.SQLITE3` aliased the
same inode as `llmbase.sqlite3`, bypassed case-sensitive path comparison and
replaced the database in a disposable filesystem probe. The delegated revision
adds portable case-alias rejection and a regression that fails that first pass.
Its source is preserved as `evidence/pre-revision-compat_io.rs`.

## Scope and evidence

Parent checkpoint: `939e885`. Only these files changed:

- `crates/llmbase-core/src/compat_io.rs`
- `crates/llmbase-core/tests/compat_io.rs`

The before archive, patch and hash manifest reconstruct exactly these two files;
fresh replay passed. The full migration source remains largely untracked, so
this is a recovery checkpoint, not a complete buildable release.

| Verification | Result |
| --- | --- |
| Original implementation with new B3 regressions | exit 101; 3 passed, 3 failed |
| First B3 implementation with new case-alias regression | exit 101; 6 passed, 1 failed |
| Independent final `cargo test -p llmbase-core --test compat_io` | exit 0; 7 passed |
| Independent scoped rustfmt | exit 0 |
| Local frontend typecheck and Python lint import | each exit 0 |
| Archive/patch replay and source hashes | passed |

Independent runner: `evidence/verify.sh`, executed in
`baypier-prod:/root/llmbase-rust-migration-20260920/b3-reserved-write-source`.
All 108 Rust/Cargo/toolchain file hashes matched the current local worktree;
see `evidence/b3-accept-source-sha256.json`. Individual raw logs and exits are
retained, including initial test-layout failures. The tests exercise actual
atomic writes, unchanged protected bytes/inodes, OS lock fencing, missing paths,
parent aliases, case aliases and ordinary/standalone controls.

The user authorized direct primary-agent review; disposition is recorded in
`evidence/review.txt`. No feature fix was implemented by the reviewer.
No full workspace/wheel/release or compiled macOS integration run is claimed.
Known-library journaling remains incomplete (`journal:false`) and is the next
task. No downstream worktree, production data, version or publication changed.
The implementation, revision and task-specific remote test sessions have exited.

## Replay

Extract `baseline-files.tar.gz` into a fresh scratch directory, run
`git apply --check /absolute/path/to/change.patch` there, then `git apply` with
the same patch. Compare reconstructed files with the `after` hashes in
`files-sha256.json`. A full migration checkout is needed to build these files.
