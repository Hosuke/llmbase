# Query/compile visibility checkpoint — 2026-09-26

Compatibility query and compile paths used weaker visibility checks than the
shared core policy. Malformed private flags could reach public query context,
permit public promotion into a non-public target, or produce a public article
from a private raw source. They now use the shared fail-closed classifier and
retain the strongest source/target classification when deriving content.

## Scope and recovery package

Parent checkpoint: `62f5a70`. Exactly four files changed:

- `crates/llmbase-core/src/compat_query.rs`
- `crates/llmbase-core/src/compat_compile.rs`
- `crates/llmbase-core/tests/compat_query.rs`
- `crates/llmbase-core/tests/compat_compile.rs`

The tests use real gather/load-index/query callbacks, promotion and compile
entrypoints. They cover malformed flags, stale public index rows, private text
in callback payloads, unchanged bytes after denied promotion, inheritance and
ordinary public controls. Promotion calls the compile public-only guard; the
test therefore exercises that actual write boundary.

The full migration source remains largely untracked. This checkpoint archives
the four original files in `baseline-files.tar.gz`, a replayable `change.patch`,
before/after hashes and acceptance evidence. It is not a complete buildable
source release. Applied source remains in the implementation worktree.

## Acceptance

Implementation was delegated; the primary agent reviewed the scoped source and
test diff and independently ran `evidence/verify.sh` on the current snapshot at
`baypier-prod:/root/llmbase-rust-migration-20260920/b2-privacy-source`.
All 108 Rust/Cargo/toolchain files matched the local hashes exactly, with no
additional remote files; see `evidence/b2-accept-source-sha256.json`.

| Check | Result |
| --- | --- |
| Baseline compile with new regressions | exit 101; 7 passed, 2 expected failures |
| Baseline query with new regressions | exit 101; 10 passed, 2 expected failures |
| Independent `cargo test -p llmbase-core --test compat_query` | exit 0; 12 passed |
| Independent `cargo test -p llmbase-core --test compat_compile` | exit 0; 9 passed |
| Scoped rustfmt on all four files | exit 0 |
| Local frontend `npx --no-install tsc --noEmit` | exit 0 |
| Local candidate Python `from llmwiki.lint import lint` | exit 0 |
| Fresh archive/patch replay and all before/after hashes | passed |

The baseline combined command stopped after compile failed; its missing query
proof was supplied by a separate baseline query run. The first format check
failed on one test assertion's layout; the final check passed. Original logs and
individual exit files are retained, including both initial failures.

The implementation wrapper reached its 17-minute limit (exit 124) after writing
the changes and report. Acceptance is based on the reviewed artifacts and
independent successful verification, not a claimed successful agent exit.
Task implementation and test processes have stopped.

The user explicitly authorized direct primary-agent review. See
`evidence/review.txt` for its disposition. These results accept this bounded
task only: complete privacy coverage, workspace/release builds, installed wheel
and Siwen/Taixu acceptance remain separate gates. No production or downstream
worktree was changed, and no version/tag/publication was performed.

## Replay

Extract `baseline-files.tar.gz` into a fresh scratch directory, run
`git apply --check /absolute/path/to/change.patch` there, then `git apply` with
the same patch. Compare each reconstructed file against `files-sha256.json`.
All four files were replayed and matched; see `evidence/replay.txt`.
Building them requires the complete migration snapshot.
