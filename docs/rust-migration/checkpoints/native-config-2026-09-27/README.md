# Accepted native configuration IO and directory orchestration

Predecessor: `af16b2d`. Primary implemented and accepted B16 on2026-09-27.
Rust now owns bounded configuration reads, missing/present branching, ordered
path resolution, default root construction and sequential directory creation.
The Python document remains opaque: PyYAML is an explicit format plugin and live
profile/default policy remains Python. Native CLI configuration shares the bytes
reader and retains its native parser. The existing scope/recovery resolver is
unchanged; this task uses a separate configuration resolver.

This ten-file recovery bundle contains the source delta, regression tests and
contract. Extract `baseline-files.tar.gz`, apply `change.patch`, and verify
`files-sha256.json`. Exact replay outside Git passes. The complete migration
still resides in its separate worktree; this bundle is not a full distribution.

## Acceptance

- Remote fmt and Clippy pass. Native core26 unit tests and4 new config tests pass,
  including the actual byte limit, native parser, ordered host steps, partial
  mkdir failure and symlink/missing-component resolution.
- Fresh candidate-v2 passes63 focused and651 full Python tests. New tests prove
  native IO despite poisoned Python read/mkdir/resolve fallbacks, malformed
  document errors, opaque mapping and alias identity, live parser/default/path
  callbacks, original exception identity, nested trace cleanup, partial mkdir
  effects, OS error fields, POSIX non-UTF8 paths, limits and explicit registration.
- An independent verifier compares the actual accepted B15 config module with
  the installed candidate: rich YAML types/alias graphs, seven pathlib edge cases,
  ordered directory errors and absence of read-only registration all match.
- Primary and independent review identified an absolute-path/deleted-cwd defect
  after the first candidate passed649 tests. The retained reproducer proves old
  success/native FileNotFoundError. The repair fetches cwd only for relative
  paths. Two new regressions and the actual-old comparison pass on a fresh venv
  and wheel; the extension hash changes, ruling out same-version pip reuse.
- All229 source inputs and48 installed Python files match.220 predecessor
  inputs remain unchanged. Both attempt receipts and the first manifest are
  retained; final acceptance uses the repaired snapshot and fresh native checks.
- TypeScript noEmit, Python lint import, exact ten-file replay and primary staged
  review pass. The user expressly permits primary review; independent review's
  concrete finding was repaired and reproduced before acceptance.

Final wheel SHA256:
`9206384f0b203440146e2ebae3885724eefa6a72a8dd8f5b0b2b91aed79066bc`.
Final extension SHA256:
`e65792c0ea141b6e18111dfcb4c079c83aefd514a24a114921afb35d98cf5b6b`.

Remote root: `baypier-prod:/root/llmbase-rust-migration-20260920/b16-native-config`.
Candidate: `candidate-v2/bin/python`. Finite tmux `llmbase-b16-native-config`,
command `bash primary/launch.sh`, uses a1500-second timeout. Final runner exits0
and its session is absent. First-run actual pane PID609979 is absent; the final
run completed before its pane PID was observed. No task process remains.

`evidence/` contains manifests, original module, scripts, phase logs/exits,
provenance, review and replay receipts. Re-running the verifier requires the full
matching snapshot, isolated toolchain and recorded directory layout; the bundle
alone does not contain all build inputs. Tests used temporary fixtures, not model
requests, production libraries or the original downstream worktrees.

## Remaining scope and requested pause

B16 is complete; the full Rust migration/release is not. Maintenance/generation
facades, worker/hooks, seven connector wrappers, remaining transport parity,
latest-candidate Siwen/Taixu integration and final full-source/release gates remain.
The user explicitly requested that the goal pause after this task is accepted
and saved, to conserve quota. Do not start another implementation task until
the user resumes.
