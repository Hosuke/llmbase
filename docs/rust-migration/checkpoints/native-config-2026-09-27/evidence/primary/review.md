# B16 review record

Independent read-only review covered compat_config, native config wiring, the
PyO3 host, Python facade/bridge and new tests. One concrete defect was identified:
an unconditional current_dir call incorrectly rejected absolute paths when cwd
had been removed. Primary independently confirmed the same finding, reproduced
actual old success/candidate failure, repaired it, and added two regressions.
No other concrete callback-order, identity, trace, mkdir-effect or OS-string
defect was reported by that bounded review.

Primary review of the final source and exact recovery delta verifies:
- IO/branch/path/mkdir orchestration is genuinely native; the format plugin and
  live profile policy are explicitly documented Python boundaries.
- No weakening of the existing scope/recovery helper or unrelated source edits.
- No JSON flattening of Python document values or lossy path string conversion.
- Read-only calls do not register/create DBs; load retains explicit registration.
- Fresh candidate-v2 is used after the native repair;651 tests and all phase
  exits are checked against the229-input snapshot and48 installed Python files.
- The ten-file patch replays byte-exactly outside Git against its saved baseline.

The user authorizes primary-agent staged review. Only the scoped checkpoint
bundle is eligible for this commit; unrelated modified/untracked migration files
remain outside the staged set. See staged-review.json for the reviewed tree hash.
