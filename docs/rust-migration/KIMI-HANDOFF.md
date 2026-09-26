# Bounded Kimi development tasks; Codex acceptance

Prepared2026-09-24. This file does not start an agent or authorize a production
switch. The full autonomous goal remains paused. Run one assigned task at a time;
return its patch and evidence before starting another task.

Worktree: `/Users/lanchair/Developer/LLM-infra/Github/llmbase-rust`.
Base commit: `a7c4d9da764462e09c0769ecd8ba548a46d27e83`.
This is a dirty, uncommitted implementation tree: a fresh clone of main will not
contain the Rust migration. Preserve all existing changes. Read `CLAUDE.md`,
`docs/rust-migration/PAUSED-2026-09-24.md`, and only the assigned task's files.

Do not commit, push, bump versions, publish, deploy, access original Siwen data,
call real model providers for tests, or expand the task. Do not replace failing
assertions with weaker expectations. Use synthetic/offline fixtures. No new
subagents or unattended retry loops. Long tests/builds run in an explicitly
allocated remote snapshot under `/root/llmbase-rust-migration-20260920/` on
`baypier-prod`, using tmux; never overwrite a snapshot another task is testing.
The existing `source/` and logs are historical evidence, not a fresh empty task
directory. Credentials must not enter task files, logs or model prompts.

## First assignment: K1 — Repair the background-worker contention failure

Own `crates/llmbase-core/src/worker.rs`, the required narrow operation-entry change
in `application.rs`, and focused worker/HTTP regression tests. Read
`crates/llmbase-cli/src/http.rs`, `tests/http_model_contracts.rs` within that crate,
and `docs/rust-migration/jobs.md`. Do not modify YAML, Python facades or unrelated
HTTP routes. If another file is required, explain the dependency before expanding.

Observed Linux failure: `deep_lint_both_envelopes_and_background_web_fix_are_real_and_job_specific`
in remote `logs/native-check-v2.log`. The persistent job is claimed successfully,
then fails with `another writer is active; explicit retry is available` when a
concurrent status reader briefly acquires the library recovery lock. HTTP already
retries a preclaim Busy error; that does not handle this postclaim/preoperation
window.

Find and prove the exact boundary before changing it. Retry only an operation
that is demonstrably before model calls or accepted writes, with bounded delay,
cancellation and the same job lease. Never retry the whole repair blindly after
partial writes, change Failed to Succeeded, steal another queued job, or slow the
test's reader to conceal contention.

Acceptance: deterministic contention reproducer; no duplicate model calls or
accepted writes; targeted job ID stays fixed; cancellation/exhaustion remains
truthful. Run the existing worker tests and HTTP model contracts on Linux and
report real commands, exit codes and raw logs. Stop after this task even if other
defects are found. Codex reviews the patch and independently reruns the reproducer.

## Subsequent assignments (not started)

| ID | Scope and ownership | Required evidence |
|---|---|---|
| K2 | `compat_yaml.rs`, `document.rs`, `visibility.rs`, affected privacy call sites | PyYAML1.1 differential cases for yes/on/TRUE/quoted strings, merges and aliases; bounded cyclic input; anonymous API/search/bake cannot expose private text, including stale indexes |
| K3 | `compat_io.rs`, `recovery.rs`, focused atomic/recovery tests | Exact old/new bytes retained for known-library writes; crash replay and shared writer lock; no implicit indexing; native DB/lock paths protected; arbitrary standalone atomic writes remain separate |
| K4 | Utility/config/normalize/worker Python facades and their isolated PyO3 modules | Actual native execution receipts; Python types, scalar offsets, live settings/callbacks and worker state preserved; no hidden old-core fallback |
| K5 | `compat_maintenance.rs`, `compat_resolve.rs`, Python lint/resolve facades | Existing direct-call behavior and live patches; genuine native orchestration/storage; truthful failures/partial state |
| K6 | `compat_catalog_model.rs`, taxonomy/xici/entities Python facades | Live callbacks actually invoked; empty/truncated output never overwrites accepted data; guarded public/private artifacts; native provenance for required paths |

Assign K2/K3 before changing the dependent facade groups. Shared `lib.rs`,
`application.rs`, PyO3 exports and Cargo manifests require integration ownership;
do not run agents concurrently against these files. An assignment ID K3 in this
table is a task label, not the Kimi model ID.

## Every task must return

1. Exact changed files and the bug/contract addressed.
2. The minimal reproducer and real before/after result.
3. Test commands, exit codes, log paths, and explicit unrun checks.
4. Remaining risks and any stopped/running tmux session.
5. A finite patch for review; no self-asserted release approval.

Codex handles scoped review and independent verification per task, then one full
integration run: native checks, no-Python release lifecycle, frozen retrieval
evaluation, installed candidate Python/Siwen tests, full bake comparison and
backup/restore/rollback. Do not rerun every full suite after each tiny edit.

## Siwen adoption gate

Keep the current production version until the privacy, recovery and required
Siwen facade contracts pass on the final exact candidate. First use a separate
sanitized library snapshot for a staging bake; compare public files, hashes,
routes, metadata and language behavior, including private-data exclusion. A
staging observation period of24–48hours is a proposed rollout precaution, not a
substitute for deterministic acceptance and not an already-started deployment.

For a static site, a first rollout can change only the offline build tool and
publish verified static output while preserving the previous build/output for
rollback. Native dynamic ask/compile/worker adoption is a later deployment gate
requiring the authorization and job/recovery tests. Neither change is authorized
merely by this handoff; production switching still needs the user's instruction.

Local preflight found Kimi Code CLI0.31.1 at `/Users/lanchair/.local/bin/kimi`.
Its current `--help` supports `--model` and `--prompt`. Verify the actual K3 model
alias and authentication before invocation; do not assume a different/default
model is K3 or copy old kimi-cli flags that this installation does not support.
