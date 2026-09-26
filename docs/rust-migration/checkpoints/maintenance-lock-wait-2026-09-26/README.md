# Bounded maintenance writer acquisition

Accepted two-file checkpoint after `cdc8f7d`; the full migration remains
unreleased. Primary implemented this repair after independent runtime backtraces
showed that the earlier startup-only diagnosis was incomplete: a status reader
could collide with repair_slugs after the internal job was created, and with
final receipt publication. Existing operation-open retries did not cover these
sections. Retrying the entire operation could repeat models or accepted writes.

Maintenance now waits asynchronously for the writer session before each covered
synchronous section, up to 100 acquisition attempts separated by 20 ms. The wait
is cancellable. Nested journal/read helpers borrow the same session; it is
released before model awaits. Only acquisition is retried; each section executes
once. Finalization gets a bounded cleanup wait even after cancellation. If that
wait is exhausted, accepted changes remain journaled and the job records Partial
where effects already occurred, without publishing a completed receipt.

Remote validation on baypier-prod, isolated b7-primary-fix and b7-finalization:

- Frozen integration regressions on original implementation: **14 passed,
  3 failed**, showing premature startup failure, cancellation failure and
  post-model contention. Fixed maintenance target: **17 passed**.
- Worker target: **10 passed**. Complete HTTP model target: **7 passed in each
  of three consecutive runs**, preserving selected job, attempt and barrier
  assertions. The full workspace run also passes that target.
- Two additional deterministic final-publication tests both fail against the
  original implementation with only the identical cfg(test) seam/tests added.
  Fixed core library: **26 passed**, including successful publication after
  lock release and Partial/no completed receipt after exhaustion. Both prove
  one accepted journal change and one internal job. The test seam is absent
  from production builds.
- Model-return contention verifies the writer is free during the provider
  request, then forces contention across the response. Exactly one model request,
  one journaled metadata write, original body and unknown metadata are preserved.
- Workspace fmt and all-target Clippy `-D warnings` pass on both revisions.
  All **119 final source/build/fixture inputs** match the current worktree.
- Full workspace `--no-fail-fast` now fails only the separate web_read raw-preview
  configuration test. It is not waived. The later change adds only cfg(test)
  finalization proof; current core units, fmt and all-target Clippy were rerun.
- Frontend TypeScript and project Python lint import pass; exact two-file replay
  and current-source hash comparison pass.

Focused independent review found no source defect. It requested explicit final
publication coverage; the two added before/fixed tests close that finding.
Review of the additions also passed. Wall-clock exhaustion tests use a 1–5 second
window around the approximately two-second policy.

Logs retain exact commands and per-step exits. The original delegated diagnosis
and its remote instrumentation remain private diagnostic evidence; none of its
instrumentation entered this patch. A later implementation delegation was stopped
with exit 143 before it changed any owned source, following the user's correction
that primary owns diagnosis/design and may handle difficult fixes directly.

All B7 runners have ended. No current release binary or wheel is claimed from
this checkpoint, and no version, push, publication or production switch occurred.
Final compatibility, migration and downstream gates remain open.

Replay into a scratch directory: extract baseline-files.tar.gz, check before
hashes, run `git apply --check change.patch` then `git apply change.patch`, and
verify after hashes. The working implementation retains the same applied repair.
