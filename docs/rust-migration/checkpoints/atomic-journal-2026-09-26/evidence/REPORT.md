# B4: durable exact-byte journal for explicit-library atomic_write

Base: B3 accepted as d50f7e3. Before snapshot: `.rust-migration-runs/b4-atomic-journal/before/`
(`src-*.rs`, `tests-compat_io.rs`; sha256 below).

## Modified files (owned only)

- `crates/llmbase-core/src/compat_io.rs` — journaled `Some(config)` path; `file_lock_path` /
  `reserved_destination` pub(crate) helpers shared with recovery; final-component symlink refusal
  for journaled writes; receipt gains `journal:true` + `intent_id`.
- `crates/llmbase-core/src/recovery.rs` — `JournalIntent`, `record_journal`,
  `mark_journal_applied`, `recover_journal_locked` (called from `Library::open`),
  `install_journaled` (stage mode 0600 preserved); 5 new unit regressions; B1–B3 unit tests
  untouched.
- `crates/llmbase-core/src/store.rs` — additive `write_journal` table in `SCHEMA`;
  `Library::open` wires `recover_journal_locked()` between `recover_locked()` and
  `recover_projections_locked()`. No other store changes.
- `crates/llmbase-core/tests/compat_io.rs` — draft `journal:false`/no-database expectations
  replaced with journaled-storage proof; B3 final-link test became a refusal test; standalone
  replace-the-link coverage added; two sentinel stubs removed after their refusal assertions
  (see contract change). All other B3 assertions preserved.

No other files touched. No staging/commit performed (files remain untracked for primary review).

## Schema / backward compatibility

New additive table `write_journal(id, library_id, root, relative_path, before_hash, before_bytes,
after_hash, after_bytes, status, created_at)` created via `CREATE TABLE IF NOT EXISTS` inside the
existing schema batch; `schema_version` stays 1, so existing version-1 databases open and gain the
table without losing data. A separate table (instead of reusing `write_intents`) avoids
reinterpreting old source intents: journal rows are opaque exact bytes, never parsed or indexed.
`root` stores the configured-root key (`raw|concepts|outputs|meta`) + exact relative path.
`before_bytes` NULL = destination absent; empty BLOB = existing empty file (distinct images).
Applied rows keep both images for audit/backup; nothing is deleted.

Write protocol (all under the shared kernel writer lease, re-entrant via the existing
thread-local session map, so `Library::open` inside `atomic_write` does not nest on the lock):
B3 configured-root + reserved-path checks (unchanged, before any mutation) → journaled
final-component symlink refusal → `WriterSession` → `Library::open` (recovers pending source and
journal intents first) → `create_dir_all` → same per-file OS lock as standalone → symlink
re-check under lock → capture exact before-image (absent vs empty distinguished) → durable
intent commit (autocommit, synchronous=FULL) → stage(0600)/write/fsync/rename/directory-fsync →
mark applied. The per-file lock is acquired before the intent commit, so a Busy refusal leaves
no new pending row and no byte change (unit test proves count unchanged).

Recovery (`Library::open`, idempotent): selects this library's pending rows only (foreign
`library_id` rows are never replayed); revalidates root key, `scoped_path` (traversal + symlink
components, stored paths are data), and the reserved-destination rule (with the spelled meta
parent also checked) before any install; takes the destination's per-file lock (Busy propagates
and fails open safely). Current bytes == after-image ⇒ crash after rename: file+directory fsync
(the durability step) before committing `applied`. Current == before-image (or both absent) ⇒
install with the 0600 stage helper. Anything else ⇒ `Error::Conflict`, external bytes untouched,
pending row kept as conflict evidence. Escaping/reserved pending records abort open with no
install. Repeated reopen changes nothing (no duplicate history).

## Intentional contract change (required by the migration goal)

Explicit-library `atomic_write` now initializes and uses the real core database and returns
`journal:true` + `intent_id`. Draft tests asserting "no database" and `journal:false` for
`Some(config)` were replaced with proof of journaled storage; these were draft expectations, not
a production guarantee. Standalone `config=None` behavior is unchanged and still mandatory:
standalone file lock, exact bytes, no guessed library ancestor, no SQLite, `journal:false`.
Consequence: two B3 tests kept a SENTINEL stub at `llmbase.sqlite3` while performing an ordinary
write later in the same test; the stub is now removed after all refusal/sentinel assertions pass
(a real database is required once writes are journaled). The final `SENTINEL` re-assertion in
the case-alias test became a bytes-unchanged assertion against the real database — same
protective intent, not weakened.

## Stricter library-only symlink boundary

Journaled writes refuse a final-component symlink (`Error::Scope`), checked before any mutation
and re-checked under the per-file lock before reading a before-image: journaling an external
referent's bytes would capture data the library does not own, and rename would silently swap the
link. The accepted B3 final-link replacement test is now a refusal test preserving BOTH link and
referent. Parent symlink aliases may still resolve into permitted roots (unchanged).
Standalone (`config=None`) final-link replacement is unchanged and now explicitly covered.

## Commands and exits (remote baypier-prod, tmux llmbase-b4-journal)

Env: CARGO_HOME/RUSTUP_HOME/PATH/PYO3_PYTHON per task, `CARGO_BUILD_JOBS=2`,
`CARGO_TARGET_DIR=/root/llmbase-rust-migration-20260920/source/target`, `nice -n10`.
Scoped rustfmt (1.98.0 aarch64-apple-darwin, `--edition 2024 --config skip_children=true`)
applied to the four owned files before rsync; exit 0. Local macOS pre-check:
`cargo test -p llmbase-core --lib` 22/22 ok, `--test compat_io` 7/7 ok.

Sources: NEW isolated `/root/llmbase-rust-migration-20260920/b4-atomic-journal-source`
(rsynced crates/, Cargo.toml, Cargo.lock, rust-toolchain.toml) and
`b4-atomic-journal-pretask` (same tree with the three pre-task src files from the before
snapshot + the NEW integration test file) to prove the regression bites the old implementation.
Prior task source directories untouched.

First pass (`b4-journal-run.sh`, logs `logs/b4-journal-*`):
- pretask `--test compat_io`: exit 101 — 4 failed/3 passed; journal:false receipts and
  final-link replacement observed, i.e. the new regressions FAIL the pre-task implementation.
- new `--test compat_io`: exit 101 — INVALID: cargo's mtime freshness in the shared target dir
  reused the pretask-built rlib (rsync preserved older local mtimes; test binary line numbers
  were new, linked core was old).
- new `--lib`: exit 0 — 22/22 (compiled fresh from the new source; all 5 journal tests ran).

Rerun with `cargo clean -p llmbase-core` before each leg (`b4-journal-run2.sh`,
logs `logs/b4-journal2-*`, exits in `logs/b4-journal-exits.txt`; each leg compiled
llmbase-core exactly once):
- pretask `--test compat_io`: exit 101 — 4 failed/3 passed at the new assertions
  (`left: Bool(false)` for `journal:true` at lines 45/141/325; final-link replacement not
  refused at line 229). The new regressions FAIL the pre-task implementation.
- new `--test compat_io`: exit 0 — 7 passed, 0 failed.
- new `--lib`: exit 0 — 22 passed, 0 failed (all 5 new journal unit tests included; B1/B2/B3
  recovery assertions intact).
Runner window: 05:26:10–05:26:48 remote (+08:00); tmux session exits on completion.

Review follow-up (`b4-journal-run3.sh`, logs `logs/b4-journal3-*`, same tmux session name):
three acceptance-gap contract tests added inside recovery.rs's test module only (no production
code change, no other file touched):
1. `journaled_atomic_write_stores_exact_images_without_indexing` extended: real nonempty
   overwrite of a binary/invalid-UTF-8 payload now asserts before/after BLOBs AND hashes for
   every row, 5 unique receipt intent IDs, final exact file bytes and zero source rows;
   empty-file coverage retained.
2. `journal_table_is_additive_for_existing_version_one_databases`: populated library with an
   accepted source/revision, `DROP TABLE write_journal` on the disposable DB (pre-task schema),
   normal reopen keeps schema_version 1 and identical source/revision/original bytes, and the
   re-created table serves an ordinary journaled atomic_write.
3. `journal_recovery_respects_the_per_file_lock`: pending intent + held destination lock makes
   `Library::open` return Busy with bytes and pending row unchanged; after release, two reopens
   yield one applied intent, expected bytes, no duplicate history.
Synced amended `src/recovery.rs` (sha256 be2c41bdca878e1c9cb9940210973ebf4a670906a4e7bbf12391ceee8d788d6a,
local == remote), forced freshness with `cargo clean -p llmbase-core` before each leg (each leg
compiled llmbase-core exactly once). Exits (in `logs/b4-journal-exits.txt`):
`run3-new-compat_io exit=0` (7 passed), `run3-new-lib exit=0` (24 passed, 0 failed; all 7 journal
unit tests ok). Pretask regression proof from run 1/2 stands and was not rerun. Old logs preserved.

## Hashes (sha256, local working tree == rsynced source)

- new `src/compat_io.rs` 2222ed34bff6fcc1a650297d91314287153695523a56d58967d697a3d5e66996
- new `src/recovery.rs` be2c41bdca878e1c9cb9940210973ebf4a670906a4e7bbf12391ceee8d788d6a
  (run 1/2 had 7bab4175a04bf8df202ac44cf4de382845c4b3f371a5f26bb6f5d194a7aac17e; amended for
  the three review-gap contract tests, tests-only change)
- new `src/store.rs` f21534d3e834a22f06fe37c77a83b53beb91687e4ecf6fcadb906040984ac2ab
- new `tests/compat_io.rs` b7de0ec714a49465b1f2c8b36cbe5480c4e934d962f49c8413fb8e892dbe248f
- before `src-compat_io.rs` 937d1d37df8fdbb5f5e3de8f398781b8fde74c9bd98118a189477b8b4dc8f0da
- before `src-recovery.rs` 8bf553c2955b993667bc6ea6a974d28abb5d87c5b400b213ddc5482623590689
- before `src-store.rs` ee0f9c6231f1f83048ea64e1df037cae94cc4aa36623b98e8196a3e30c4d71f5
- before `tests-compat_io.rs` 0a16b09e9e3cc17697f1e0f1f214097e466343f93d986f3231df451fbb0df78c

## Processes / evidence

- tmux `llmbase-b4-journal` on baypier-prod (finite runners `b4-journal-run.sh`,
  `b4-journal-run2.sh`; both exit at completion).
- Raw logs: `/root/llmbase-rust-migration-20260920/logs/b4-journal-{pretask-compat_io,
  new-compat_io,new-lib}.log`, `b4-journal2-{pretask-compat_io,new-compat_io,new-lib,clean}.log`,
  `b4-journal3-{new-compat_io,new-lib,clean}.log`,
  exits: `logs/b4-journal-exits.txt`. Local copies of runners:
  `.rust-migration-runs/b4-atomic-journal/b4-journal-run{,2,3}.sh`.

## Remaining limits

- No real process-kill/power-loss testing: crash windows are simulated by deterministic
  pending-intent fixtures matching the accepted schema (inserted via the production
  `record_journal` or schema-exact SQL) driving normal `Library::open`. Full fault-injection
  gate remains later work.
- Durability (fsync) steps are exercised but their effects are not directly observable in tests.
- Non-UTF-8 file names under an explicit library are now refused (`Invalid`) because the journal
  stores the exact relative path as TEXT; standalone writes are unaffected.
- A conflicted pending journal intent fails `Library::open` with `Conflict` until resolved,
  matching existing source-intent recovery semantics; evidence row and both images are retained.
- Pre-existing unrelated breakage: untracked `crates/llmbase-core/tests/compat_maintenance.rs`
  references a nonexistent module and fails `--tests` compilation; outside B4 ownership, not
  touched. Scoped targets (`--lib`, `--test compat_io`) are unaffected.
