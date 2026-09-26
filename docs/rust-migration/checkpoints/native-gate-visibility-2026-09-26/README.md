# Native module wiring and public-only maintenance reads

Accepted bounded source checkpoint after `6f2b5d9`, not a complete source
release. Six source files are preserved as exact baseline bytes plus a
replayable patch; the same repair remains applied in the migration worktree.

The maintenance compatibility module was missing its crate export. Wiring it
into the native build exposed a duplicated visibility check: quoted or numeric
`private` flags could enter public-only article loads. The reader now uses the
existing shared fail-closed policy. Privileged reads retain all articles.
Two files receive formatting-only changes, and a recovery test uses a named
row structure without changing its exact-byte, hash or journal assertions.

Primary review found the visibility defect in the initial implementation and
required the separate repair before acceptance. The frozen new regression
fails against the initial module: quoted and numeric private records appear
in its public result. It passes against the repaired module, including Unicode
content, unchanged source bytes and privileged access to all eight fixtures.

Validation ran on baypier-prod in the isolated migration directory:

- Expanded maintenance target: **5 passed**. Core library: **24 passed**.
- Workspace formatting and all-target Clippy with `-D warnings`: **exit 0**.
- Primary independently reran both targets: **5 and 24 passed**, exit 0.
  All **119** Rust/build/fixture inputs match the current local source before
  and after that run (238 successful hash checks).
- Full workspace `cargo test --workspace --locked --no-fail-fast`: **exit 101**,
  three failed tests in two targets. Two HTTP background maintenance tests fail
  on post-claim writer contention; one web read test fails its body truncation
  assertion. These remain release blockers and are not waived as historical.
- TypeScript check and the configured project Python lint import: **exit 0**.
- Exact six-file baseline/patch replay and current-source hash comparison pass.

The implementation runner records per-step exits in `runner.log`. Its final
`revision.exit=0` means orchestration ended, not that the workspace passed.
The implementation agent reached its 15-minute wrapper timeout after the
remote runner completed and before finishing its report. Primary confirmed
the agent exited, collected the raw records and completed independent review;
no unfinished feature implementation was taken over.

Additional diagnostics use the **earlier first-pass** release binary SHA256
`5c043109012341241abfa1a2256b9f22195b4efc2ffffd95de6711ddb701ecd0`, which does
not include the later visibility repair. They are progress evidence only:

- Interpreter-free Linux chroot CLI lifecycle: 31 steps passed, including
  ingest, drafts, jobs, retrieval, streaming, backup, restore and rollback.
  Deterministic fixture servers run outside the runtime; no paid provider used.
- Frozen lexical evaluation: raw/compiled each 30 held-out queries have
  Recall@5 and MRR@10 of 1.0, with 60 unique citations resolved and no baseline
  comparison failures. Real embedding quality remains unverified.
- Literal YAML `max_content_chars: 3` returns exactly `𠮷原典` and leaves the
  source unchanged. The failing serialized fixture needs separate diagnosis;
  this probe does not waive its failing test.

The associated diagnostic receipts remain under
`.rust-migration-runs/b6-primary/artifacts/`. Native HTTP/MCP inside the same
interpreter-free runtime, final-artifact reconciliation, remaining parity,
migration and downstream adoption gates are still open. No production switch,
version bump, push or publication occurred. All B6 runners have ended.

Replay in a scratch tree: extract `baseline-files.tar.gz`, verify the before
hashes in `files-sha256.json`, run `git apply --check change.patch` followed by
`git apply change.patch`, then verify after hashes. Do not apply blindly to a
different dirty tree.
