# C3 — explicit Rust worker outcomes, 2026-10-10

First implementation slice of [#13](https://github.com/Hosuke/llmbase/issues/13).
The job ledger previously allowed `Succeeded` without a result, and the finite
worker interpreted missing/unknown report statuses as success. A faulty or
incomplete handler report could therefore produce an `ok` event.

`JobOutcome` now represents Completed, Partial, Failed and Cancelled outcomes.
The worker sends these through `Library::finish_job_outcome` into the existing
transaction and lease fence. Both the typed entry and retained `finish_job`
entry reject successful jobs with absent/null results or an error before any
SQL update/event. An explicit `{}` result remains a valid no-op.

The worker validates the terminal status of the existing report-producing
operations: compile, legacy_compile, plugin_ingest, maintenance_repair,
taxonomy_generate, xici_generate, entities_generate, lint_fix and
web_write_wiki_clean. Missing, non-string or unrecognized report statuses become
visible failures. Existing operation-specific no-op values are preserved:
legacy_compile/entities_generate `no_changes`, entities_generate `disabled`,
and cleanup `ok`. Ordinary domain outputs keep their shape, including a legacy
query string and a draft's `status: draft`; this is not generic JSON-schema
validation of every operation result.

Partial/failed/cancelled reports retain their result and a safe explanatory
error. The stored result/JSON schema and manual retry policy are unchanged.
After the work/cancellation select, the worker also checks the cancellation
token: cancellation observed before finalization converts a late Completed
result into Partial and preserves accepted effects. Persisted cancellation,
expired leases and replaced lease tokens still reject finalization through
the original SQL fence.

## Acceptance coverage

- Absent/null completion and contradictory error: rejected without state or
  event mutation. Explicit empty-object completion: accepted.
- Invalid compile handler reports: durably Failed, no `ok` event, including
  null, missing status, numeric status, `error`, `queued` and scalar output.
- Deterministic lint/cleanup partial reports remain Partial; successful no-ops
  and domain-specific status fields keep their existing meaning.
- Partial artifact references survive database reopen, manual retry and
  reclaim under the same library/job ID. Old attempt results remain in events.
- A real finite worker runs legacy_compile against a deterministic loopback
  fake provider: one accepted article and one invalid generation yield Partial;
  a reopened application retries the same job, requests only the failed source,
  and succeeds without rewriting the accepted article or its source metadata.
- A wholly invalid provider generation yields Failed, preserves the raw source
  bytes and creates no compiled document or `ok` event.
- Cancelled/expired/replaced leases cannot finish with a valid success result
  or append a terminal event. Existing worker contention/cancellation contracts
  run alongside these tests.

The final run passes **117 core tests plus 5 CLI workflow tests** (122 total;
the recovery test's nested subprocess is not double-counted). Core clippy and
formatting also pass. See `validation.json` and saved logs. Remote source and
fixture hashes are in `source-inputs.json`; all 128 inputs are checked before
and after the final run. This tests the changed native source, **not the older
B16 installed wheel**. No new package version or production deployment is made.

## Execution and review

Isolated runner:
`baypier-prod:/root/llmbase-rust-migration-20260920/c3-worker-outcomes-20261010`.
Final finite tmux session: `llmbase-c3-final`, command
`bash primary/remote-run.sh`; each step is bounded to 300
seconds. The script checks formatting, core unit/related integration tests,
clippy on core all-targets and CLI workflow contracts. Logs and exit files live
in that task directory. The fake provider listens only on remote loopback and
ends with its test; no paid provider or production library is used.

The initial run passed the selected tests but clippy could not compile two
other test targets because their checked-in fixtures were missing from the
copied runner. The fixtures were then included in the source manifest. Review
also found that lint_fix and cleanup can report Partial; their explicit
contracts and regression tests were added before the final run. Initial logs
are preserved remotely under `first-run/`; they do not constitute final lint
acceptance.

An implementation agent owned jobs.rs. Primary owned worker integration and
the actual compile/retry tests. A separate read-only reviewer checked the
staged Rust diff, identified the missed deterministic partial contracts, then
re-reviewed the correction as LGTM. No local Rust build or long test was run.
The final runner exited 0 and both C3 task sessions have ended. Local TypeScript
noEmit, Python lint import and staged whitespace checks pass. The normal native
GitHub CI is left enabled for this runtime change; its separate result is not
substituted for the recorded remote checks.

## Remaining scope

#13 stays open: versioned structured error codes/retryability, normalized
artifact IDs, external registered-handler integration and generalized partial
quota/checkpoint resumption remain to implement. This slice preserves current
reports and proves one existing compile-resume path; it does not claim arbitrary
custom jobs are resumable or changes are exactly once. #11's application-scoped
Rust extension registry is still absent; #12 lifecycle events and #14 durable
scheduling remain separate blocks. The C2 downstream classic privacy blocker
also remains unresolved.

Commit/push this accepted block, update #13 and stop. The broader Goal remains
paused; no next block or site cutover starts automatically.
