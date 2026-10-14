# Status — 2026-10-14

C7 fixes #26's large-body visibility outage: bounded frontmatter reads replace
full-document reads, preserving fail-closed privacy and current metadata ETags.
365 native tests, 727 installed Python tests / 18 subtests, and independent
review pass. On an isolated copy grown to 83,391,106 Markdown bytes, the old
wheel's three public 503s become 200s with unchanged public response bytes.
The available real Taixu copy is only 40.7 MB; this is not acceptance of the
reporter's latest corpus or a full bake. See
[C7 evidence and limits](results/c7-visibility-headers-2026-10-14/README.md).
Hand this candidate to downstream retesting; no release, production switch or
broader Goal resume is implied.

## Earlier checkpoint — C6

C6 resolves R2's explicit public-projection integration gap (#25): reviewed Flask
host pipelines can register anonymous public aggregate views and bounded detail
wrappers. View/rule/hook bindings, scopes and conditional freshness fail closed.
Installed engine 719 tests/18 subtests and 136 frozen Siwen GET/privacy tests pass.
The real copied Siwen wrapped article makes zero snapshots (previously one), with
identical bytes; observed median 640.771 → 21.948 ms. See
[C6 evidence and remaining suite differences](results/c6-public-projections-2026-10-12/README.md).

The complete committed Siwen suite is **not all green**: 610 passed, 14 failed,
8 skipped. Thirteen failures are R2's existing auth migration differences; one
fixture replaces a registered view after setup. The saved site adapter patch must
be integrated before R3; a wheel upgrade alone does not opt in. C5 full CI passed.
Real dual-site bakes and #19/#21 release gates remain pending. Save this block and
stop; no production switch or broader Goal resume is implied.

## Earlier checkpoint — C5

C5 fixes #24's per-request whole-library visibility scan for the original Flask
article, sections and raw-source detail views. Each checks its current target
metadata; aliases/backlinks retain privacy and path checks, with no stale cache.
Fresh installed wheels pass 59 focused and 693 full Python tests (18 subtests).
At 10,000 synthetic documents, article median latency falls from 130.837 ms to
0.843 ms; response bytes and source files match. This is not real-site bake
acceptance. See [C5 proof and limits](results/c5-detail-visibility-2026-10-12/README.md).

C4's full GitHub CI at `90394d0` passed (native, fresh wheel and frontend checks).
C5 changes no Rust source and remains an unreleased upstream repair. Siwen's
custom-route privacy fix and actual dual-site bake gates remain with #19/#21.
Save this block and stop; the broader Goal remains paused, with no production
switch or unattended next task.

## Earlier checkpoint — 2026-10-11

C4 completes the minimal application-scoped Rust extension API in #11: typed
registration/schemas, explicit library/capability policy, constrained shared
services, cancellation, HTTP/MCP dispatch and a separate public-only example CLI.
The final isolated run passes 357 native tests (18 new), fmt and clippy. Independent
review caught and verified the fix for MCP request cancellation. See the
[API guide](extensions.md) and [C4 acceptance](results/c4-rust-extensions-2026-10-11/README.md).

C3's full GitHub CI at `513105a` passed, including native checks, fresh installed
wheel tests and frontend build. C4 remains an unreleased native checkpoint; no
new wheel or site acceptance is claimed by its remote run. C2's downstream classic
privacy blocker (#19), hooks (#12), worker follow-up (#13), schedules (#14) and
final release gates remain open. Work stops after this accepted block; the broader
Goal stays paused, with no production change or unattended next task.

## Earlier checkpoints — 2026-10-10

C1, the library-identity slice of issue #10, is accepted. The verifier now
selects and checks real `siwen`/`taixu` IDs instead of certifying `default` rows.
Six regressions and independent installed-candidate identity/denial checks pass;
the frozen downstream copy passes 24 Flask routes and 159 JSON / 135 HTML per
site. All 166 engine inputs/48 installed Python files match B16, so no Rust
rebuild was needed. Original downstream files changed concurrently; no acceptance
of those later changes is claimed. Full issue #10 and release remain open.
See [C1 evidence and limits](results/c1-dual-identity-2026-10-10/README.md).
The user authorized one block only: save it and stop; no next task or long test
is running. Goal scheduling remains paused. Earlier stop/resume notes below
are historical and do not override this bounded acceptance.

## Historical status — 2026-09-27

Not complete. This is an implementation log, not a claim of parity.

**Latest instruction, 2026-09-27: archive and pause.** The user requested a
complete WIP source commit/push and concrete GitHub issues, then rest until
quota is available. The [handoff queue](issues/README.md) supersedes older
next-task/resume notes below. No new feature, long test or implementation agent
is authorized by this handoff; the Goal remains paused. B16 is the last accepted
implementation checkpoint, not final dual-site/release acceptance.

[ADR 0002](../adr/0002-clean-rust-major-and-extensions.md) records the approved
clean-major direction: Rust services and explicit extensions, per-library
profiles, optional documented Python transition support. Classify legacy
contracts with replacements; do not spend unlimited work reproducing private
helper/global-patch details. Generic lifecycle, source preservation, isolation,
recovery and publication gates remain required. See the handoff for GitHub
links and the saved source branch; historical receipts below are unchanged.

**Resumed2026-09-26; goal status was verified as active.** The user prioritizes
Siwen and Taixu adoption, delegates implementation in small sequential tasks,
and authorizes primary-agent review and a Git checkpoint after each accepted
task. The primary agent owns task definition, integration, verification and
saving; after the user's correction it also owns diagnosis/design and may
directly implement difficult or critical repairs. K3 receives specified,
bounded implementation tasks. The earlier
[pause handoff](PAUSED-2026-09-24.md) remains historical evidence.

Later state correction2026-09-26: get_goal now reports `blocked` although work
has made progress and no engineering permission/input blocker exists. The user
explicitly asked to continue; primary is continuing this turn. The goal tool
cannot set active, so the user was told that UI `goal resume` is required for
automatic continuation. Do not treat this scheduler state as permission to
stop ongoing authorized work, mark completion, or narrow the original goal.

Subsequent goal resume2026-09-26: `get_goal` now verifies `active`. Primary
notified the user. Previous turn is progress: B8 source/test evidence and
checkpoint d937550 changed authoritative state. B9 actual PID90265 was
confirmed live on resume; do not restart it because of old marker/observation
timeouts. Its preliminary harness is under primary review while it completes.

## Current bounded checkpoints

- Latest accepted checkpoint `0229032`: B16 native config IO/default paths/
  ordered mkdir, with explicit PyYAML format and live profile-policy boundaries.
  Final fresh candidate-v2 passes63 focused/651 full Python, native26+4,
  fmt/Clippy, independent actual-old graph/path/directory comparison, and the
  deleted-cwd red/green reproducer.229 inputs/48 installed Python files and
  exact10-file outside-Git replay match. Primary staged review and TypeScript/
  lint import pass; staged tree is empty after commit. Both runner attempts
  exit0, final tmux is absent; no B16 task remains running. See
  [evidence](checkpoints/native-config-2026-09-27/README.md).
  Per the user's explicit budget instruction, stop here and pause the goal;
  no next task is assigned. Full release remains incomplete. The earlier B16
  pending/running notes below are historical. Handoff: PAUSED-2026-09-27.md.

- User budget limit2026-09-27: finish B16 only, independently accept and save
  its scoped Git checkpoint, then explicitly pause the active goal. Do not
  start the next implementation task. First B16 candidate passed native26+4,
  focused61/full Python649 and the actual-old comparison. Primary's separate
  deleted-cwd reproducer then proves old success/native FileNotFoundError;
  repair now avoids cwd lookup for absolute paths and adds two regressions.
  A fresh candidate-v2/wheels-v2 will be built to avoid same-version pip reuse.
  This budget instruction supersedes the earlier instruction to immediately
  continue with the next task after B16; full scope is preserved for later resume.

- B16 implementation2026-09-27: native config bounded reader, ordered opaque
  format host, separate non-strict path resolver and sequential mkdir are wired
  through the FFI/facade. PyYAML remains an explicit format plugin; native CLI
  keeps its Rust parser and shares the bytes reader. New native/Python tests,
  actual-old independent verifier and229-input source snapshot are prepared.
  Finite baypier-prod tmux llmbase-b16-native-config runs bash primary/launch.sh
  with1500-second timeout; actual pane PID609979 observed live. Source/fmt/Clippy
  pass; remaining acceptance pending. Logs/exits are under the isolated
  b16-native-config root. Primary review identified an absolute-path/deleted-cwd
  edge requiring a regression and repair before acceptance; no B16 success or
  full release claim yet. The previous status-review note below is historical.

- Status review2026-09-27: HEAD remains af16b2d, accepted candidate628 Python
  tests. B16 has a specified contract and independent design review, but no
  runtime implementation or new acceptance yet. Its config resolver will be
  separate from the authorization/recovery path helper. Remaining required work
  includes maintenance/generation facades, worker/hooks, seven connector CLI
  wrappers, transport parity, latest-candidate Siwen/Taixu integration and final
  source/build/release. Planning estimate is4–7 focused workdays, with low
  confidence until worker and dual-site integration findings are closed; this
  supersedes the earlier3–5-day estimate. Isolated read/search integration may
  start now; production readiness is not established. Repeated K3 connection
  failures are not assumed to provide future parallel speedup. No new long
  build/test or agent dispatch was launched for this status review.

- Latest accepted checkpoint `af16b2d`: B15 canonical config default roots.
  New tests reproduce5 failures/5 passes on B14; repaired fresh candidate passes
 40 focused/628 full Python tests. Actual-old/candidate relative/absolute matrix
  has four exact-byte successes; regressions verify applied journal before/after
  bytes, no unintended directories and unchanged out-of-root/reserved refusal.
  All225 inputs/48 installed files match;222 earlier inputs and the actual native
  extension hash are unchanged, permitting documented native-check reuse. Primary
  fixed a new test's early optional-extension import, verified10 clean unbuilt
  skips, then reran the complete remote runner; both attempts are retained.
  Exact3-file replay, TypeScript/import checks and staged primary review pass.
  Final runner0/session absent; no task process remains. [Evidence](checkpoints/canonical-config-roots-2026-09-27/README.md).
  The relative-root defect is repaired; older pending B15/B14 notes below are
  historical. Next: define the shared native config read/ensure_dirs contract,
  retaining live defaults and non-registering reads. Remaining full migration
  and latest dual-site/release gates are unchanged; goal is not complete.

- B15 canonical default roots: primary specified and applied the narrow
  config._defaults base-resolution repair; native scope/writer code unchanged.
  New regressions on accepted B14 reproduce5 failures/5 passes. Fresh225-input
  candidate passes40 focused tests and the actual-old/candidate relative/absolute
  write matrix (four exact-byte successes).222 preceding inputs are unchanged;
  B14 native checks are reused only after hash/provenance validation. The newly
  built extension hash is identical to B14. Finite baypier-prod tmux
  llmbase-b15-config-roots, command bash primary/launch.sh, actual pane PID573683
  observed live during full Python tests. Task root b15-config-roots contains
  runner.log/runner.exit; full acceptance and checkpoint pending.

- Latest accepted checkpoint `a36d463`: B14 native heading normalization.
  Primary takeover passes fmt/Clippy, native5 plus unchanged paragraph2,
  fresh-candidate focused79/full Python618, independent547 original comparisons
  and9 error checks, and callback/override/exception-identity proof. All223
  source inputs/48 installed Python files match; frozen60 outputs unchanged,
  native52 shared-dialect cases pass, exact8-file replay and precommit review
  pass. Final runner0, tmux llmbase-b14-heads and actual PID557647 absent.
  No task process remains. [Evidence](checkpoints/native-headings-2026-09-27/README.md).
  Following B14 pending/running notes are historical. Next required repair:
  the independently reproduced relative config-root duplication described below.
  Broader config migration, maintenance/catalog/worker/connectors, latest dual-site
  integration and full source/release gates remain outstanding.

- B14 primary takeover2026-09-27: implemented the specified native scanner,
  bounded declarative regex action and live Python matcher ABI/facade. Added
  native and installed-candidate tests, retaining the frozen60-case oracle and
  all existing paragraph code/tests. Snapshot223 source inputs is frozen.
  Isolated baypier-prod task b14-heads runs finite tmux llmbase-b14-heads,
  command bash primary/launch.sh, with1500-second timeout. Actual pane PID557647
  was observed running; source-before/fmt/Clippy pass, core tests/build/fresh
  candidate acceptance pending. Logs and phase exits under the task root,
  runner.log/runner.exit. No accepted B14 claim or full release claim yet.

- B14 terminal correction2026-09-27: K3 made no runtime implementation changes
  and encountered three observed APIConnectionError failures. Primary explicitly
  sent SIGTERM to the verified PID/PGID74284 at02:43:04 after the bounded task
  exceeded wall time without implementation. agent.exit is143; both74283/74284
  are absent and local tmux has no remaining sessions. All220 source-before
  hashes were unchanged at termination. Evidence:
  .rust-migration-runs/b14-heads/primary/termination.json. The detailed task,
  frozen60-case fixture and independent heldout/callback verifiers are retained;
  no B14 implementation, acceptance or remote test run is claimed. Do not blindly
  retry K3. Goal remains active; the latest accepted checkpoint is24f34f5.

- Independent config diagnosis during B14: installed accepted B13 candidate-v2
  rejects a legitimate native atomic write after missing-config load with a
  relative base (PermissionError: library scope denied); original Python succeeds
  and the same candidate succeeds with an absolute base. Defaults already embed
  the relative base prefix, then the native ABI joins it to the absolute base a
  second time. Reproducer, original modules and exact result/provenance are in
  .rust-migration-runs/config-native-design/. Isolated remote temporary fixtures,
  no model or production IO. Fix canonical effective config roots; do not weaken
  the native scope guard. Not assigned; B14 has now terminated as recorded above.

- B14 heading normalization assigned after24f34f5. Primary specified a shared
  Rust scanner/rewrite primitive, native declarative Rust-regex action and explicit
  Python regex callback boundary preserving richer expressions/compiled flags.
  Eight live parser/helper identities preserve an explicit override path. Task:
  .rust-migration-runs/b14-heads/TASK.md;220-input snapshot, frozen60 old outputs,
  native shared-dialect52-case subset. Local finite orchestration
  llmbase-b14-heads-k3 ran run.py; it is now terminal as recorded above.
  Intended remote finite runner llmbase-b14-heads under isolated baypier-prod
  /root/llmbase-rust-migration-20260920/b14-heads. No acceptance claimed. Prior
  B13 runners are terminal and checkpoint24f34f5 is saved before this assignment.

- Latest accepted checkpoint `24f34f5`: B13 native taxonomy read/value helpers.
  Fresh candidate-v2 passes57 focused and604 full Python tests. Native fmt,
  Clippy and9 tests passed before the final facade-only repair; exact217 unchanged
  input hashes and unchanged extension hash justify reusing them. Independent357
  old/new cases,10 ABI bounds/failure checks, all six live helper comparisons and
  six exception identity checks pass. Primary repaired the independently
  reproduced malformed-tag panic, aggregate read bound, stale config-error trace
  and recursive localize_tree wrapper bypass. All219 source inputs/48 installed
  Python files match; exact11-file replay and precommit checks/review pass.
  Final remote runner0, all B13 sessions ended; K3 exit124/process absent.
  [Scoped checkpoint](checkpoints/native-taxonomy-read-2026-09-26/README.md).
  The pending/running B13 notes below are historical. No full release or latest
  dual-site acceptance is claimed. B14's heading rule/callback contract and60
  original outputs are frozen; implementation remains outstanding after the
  failed K3 dispatch recorded above.

- B13 is now under primary ownership. K3 reached its explicit35-minute deadline;
  agent.exit124 and actual PID25272 absence were verified. Its11 owned-file
  handoff, hashes and logs are retained. Primary independently reproduced two
  real defects on isolated baypier-prod: native7 passed/2 failed, including a
  panic from file-derived tags=[42] and missing aggregate read bounds. The repair
  validates at the common fallback boundary, bounds actual reads to64MiB, reuses
  already-read titles, and clears traces before config errors. All219 build
  inputs are frozen. First main runner stopped on one Clippy sorting warning;
  logs retained under primary/attempt1, stable sort_by_key repair applied.
  Remote finite session llmbase-b13-taxonomy-read runs bash primary/launch.sh;
  runner.log/runner.exit and per-phase receipts in the isolated b13 task root.
  Candidate/full acceptance is pending. Primary has357 heldout cases and8 ABI
  size/type checks plus config-failure checks ready. No accepted B13 claim yet.
  B14 heading diagnosis and60 actual-old outputs are frozen under
  .rust-migration-runs/b14-heads/primary/; it is not assigned or implemented.

- Independent primary audits during B13: actual old worker compile/learn helpers
  swallow synthetic failures and return None; reproducer at
  .rust-migration-runs/worker-native-design/. Native scheduling cannot treat that
  return as success. Actual42-path CLI help audit on accepted B8 binary finds7
  absent connector commands;35 other paths advertise legacy options. This is
  help availability only, not full CLI parity or current-final-binary acceptance.
  Evidence .rust-migration-runs/cli-help-audit/; zero provider requests/files.

- B13 taxonomy read/value facade is now assigned after06d2898. Primary froze76
  actual legacy value cases and3 custom-root cached/uncached/missing libraries,
  defined seven native actions and live helper override/error boundaries. Exact
  task .rust-migration-runs/b13-taxonomy-read/TASK.md; source-before215 files.
  Local finite orchestration llmbase-b13-taxonomy-k3 runs run.py; inspect actual
  agent.pid/process plus agent.log/agent.exit. Remote intended finite session
  llmbase-b13-taxonomy-read under isolated baypier-prod b13-taxonomy-read root.
  No acceptance claimed. Native taxonomy generation/writes remain separate.

- Latest checkpoint `06d2898`: B12 paragraph normalization is accepted and saved.
  K3 dispatch after355db6f exited1 before edits: OAuth TLS connection failure;
  actual PID19557 is absent. No retry or live K3 task. Primary froze229 actual
  legacy outputs, implemented Rust state machine and five facade tests, leaving
  existing tests/heading normalization unchanged. Full214-source snapshot;
  isolated baypier-prod root b12-paragraphs, finite tmux llmbase-b12-paragraphs,
  command bash primary/launch.sh; runner.log/runner.exit/per-phase receipts.
  fmt/Clippy/core5, focused65 and full Python595 pass; all214 build inputs/48
  installed Python files match. Frozen229 oracle and independent1200-case
  differential comparison pass. Remote runner0/session ended, exact5-file replay
  passes. See .rust-migration-runs/b12-paragraphs/REPORT.md and
  [evidence](checkpoints/native-paragraphs-2026-09-26/README.md). B8 standalone
  binary predates this utility change; final native release refresh remains.
  Next taxonomy diagnosis is prepared at
  .rust-migration-runs/b13-taxonomy-read/primary/diagnosis.md; not assigned yet.
  It records concrete fallback/label/assignment differences in the uncompiled
  catalog draft. Define exact read-facade ownership/contracts before dispatch;
  generation/writes/worker remain required follow-up work.

- Latest checkpoint `355db6f`: B11 accepted candidate, fresh wheel,
  all211 source hashes and48 installed
  Python files verified; fmt/Clippy/native maintenance5, focused35 and full
  Python590 tests pass. Frozen actual legacy lint outputs match exactly in both
  CJK modes (17/16 issues); actual extracted Siwen callback executes once.
  Final remote runner exit0 and session ended. No full dual-site acceptance is
  claimed. [Scoped recovery bundle](checkpoints/native-lint-facade-2026-09-26/README.md).
  The B11 pending/running paragraphs below are historical execution records.
- Estimate corrected after the B11 acceptance: allow3–5 workdays for full
  completion/release, subject to remaining parity and integration findings.
  Earlier1–2-day estimate was optimistic. Isolated Siwen/Taixu integration can
  begin now; switching production requires final-candidate dual-site acceptance.
  Remaining work includes direct maintenance/catalog/worker/config/normalize_heads
  native delegation, transport contracts, refreshed integration and release.

- B11 now under primary ownership. K3 wrote the FFI/facade draft but then hit
  five observed APIConnectionError retries without producing tests/build.
  Primary intentionally stopped verified PID/PGID8758; exit143 and PID absence
  confirmed, handoff bytes/log retained. Primary reproduced and fixed renamed
  positional cfg, positional-only+kwargs and optional-gap binding defects,
  added15 native regression cases, plus a frozen actual-old-lint eight-category
  comparison and the actual extracted Siwen callback proof. No core algorithm
  or old test changed. Final source snapshot211 hashes is frozen.
  Remote fmt/Clippy and compat_maintenance5 pass. Initial venv setup failed
  (missing ensurepip); isolated pip --python is verified and the finite runner
  resumed with --without-pip venvs, retaining those passing Rust checks.
  `baypier-prod` tmux `llmbase-b11-native-lint`, actual pane PID139108 observed
  live during wheel build; root `/root/llmbase-rust-migration-20260920/b11-native-lint`,
  command `bash primary/launch.sh`, runner.log/runner.exit plus per-step files.
  Candidate lint/full Python acceptance is pending. Local orchestrator ended;
  remote work persists independently. Details `.rust-migration-runs/b11-primary`.

- Latest accepted: `a27595c`, B10 copy-first/actual legacy reader. K3's first
  complete pass and primary's strengthened independent34-step run exit0;
  full legacy module/archive hashes, original10-file byte inventory, completed
  job/deleted-source/full Evidence identity and configuration preservation pass.
  Incomplete-receipt conflict recovery is explicitly simulated, not SIGKILL;
  tampered snapshots fail verify. Native119 sources remain B8-identical.
  Exact two-file replay and required precommit checks/review pass. K3 exit0,
  actual PID gone; primary remote runner exit0, both B10 sessions ended.
  [Evidence](checkpoints/copy-first-legacy-reader-2026-09-26/README.md).
- B11 assigned after a27595c: native deterministic lint facade and real
  cfg-only Siwen callback compatibility. Primary reproduced the actual callback
  TypeError before invocation and specified DTOs, callback dispatch, original
  exceptions, hook ordering and fresh installed-candidate verification.
  Ownership: Python extension lib.rs/maintenance.rs, _native.py, lint/checks.py
  and one new focused test file. No Rust core redesign or downstream mutation.
  Snapshot210 files and before bytes in `.rust-migration-runs/b11-native-lint`;
  local orchestration `llmbase-b11-lint-k3` runs its run.py. Inspect agent.log,
  agent.pid and agent.exit; verify actual PID, not the marker alone.
  Remote finite build/test session will be `llmbase-b11-native-lint` under the
  same named isolated baypier-prod task root. No B11 acceptance claimed yet.

- Adoption estimate2026-09-26 (not acceptance): isolated integration may begin;
  allow 1–2 workdays for a switch-ready candidate, or 2–3 if deeper facade
  regressions appear. Current release native tests and transports are accepted;
  older B5.1 single-site results are not latest-candidate dual-site proof.
  B10 K3 was independently observed live, not merely a running marker. Primary
  has completed the bounded B11 lint facade task definition at
  `.rust-migration-runs/B11-native-lint-task-draft.md`; it remains undispatched
  until B10 review, verification and Git checkpoint. Remaining broader facade,
  worker/catalog and transport contracts are still required, not waived.

- Latest accepted: `944acae`, B9 actual release CLI/HTTP/MCP in the no-Python
  Linux chroot. K3 implemented the specified harness; primary strengthened gate
  expiry/deadline/actual-JSON/cleanup checks and independently verified31 CLI
  steps plus both real transports. The delayed-delta-after-expiry negative proof
  is rejected, no success receipt, HTTP clean exit0. All119 native inputs remain
  B8-identical. Same-binary fixed lexical/performance eval passes: raw/compiled
  heldout30 each, Recall@5/MRR@10=1,60unique citations. Two-file replay passes.
  [Evidence](checkpoints/native-transports-2026-09-26/README.md). All B9 processes
  ended; K3 exit0, primary finite remote session ended after two passing checks.
- B10 assigned after that commit: actual copy-first/backup/restore and legacy
  reader rollback, synthetic custom-root library and pristine baseline archive.
  Primary specified exact engine operations, immutable snapshots and simulated
  incomplete-receipt recovery. K3 ownership: verify-migration.py and runbook only.
  Local orchestration llmbase-b10-migration-k3 runs
  .rust-migration-runs/b10-migration-old-reader/run.py; inspect agent.log/agent.exit.
  Remote session llmbase-b10-migration-old-reader below the isolated baypier-prod
  migration root. Native binary remains f1b2bdd...; no build/runtime edit needed.

- Latest accepted: `d937550`, B8 one-file test fixture repair. Primary diagnosed
  serde_yaml/serde_json arbitrary_precision input corruption; K3 implemented
  JSON fixture encoding plus an actual literal YAML/non-BMP/raw-byte regression.
  Original web_read3 pass/1 fail; fixed5 pass, document contracts8 pass.
  **Full Rust workspace tests, fmt, all-target Clippy and actual release build
  now pass**, each exit0. Fresh CLI SHA256
  f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8.
  Primary checked all119 final inputs and exact one-file replay. First remote
  overlay path was wrong; those results remain recorded, not accepted. All B8
  processes ended. [Evidence](checkpoints/web-read-config-2026-09-26/README.md).
- B9 assigned sequentially after d937550: extend existing interpreter-free
  lifecycle proof with actual release-binary HTTP/MCP and gated SSE forwarding,
  then frozen same-binary retrieval evaluation. Primary has specified exact
  executable commands, routes, schemas, scope checks and shutdown behavior;
  K3 implements the bounded harness, not diagnosis/design. Local orchestration
  llmbase-b9-transports-k3 runs .rust-migration-runs/b9-native-transports/run.py;
  agent.log/agent.exit there. Remote session llmbase-b9-native-transports under
  the isolated baypier-prod migration root. No engine rebuild/change required.
  B10 copy-first/legacy-reader verification is prepared only, not dispatched.

- Earlier accepted: `b42def8`, primary's two-file maintenance writer acquisition
  repair. Original integration regressions: 14 pass/3 fail; fixed17 pass;
  worker10 pass, HTTP7 pass in each of three runs; workspace now fails only
  web_read. Final-publication regressions both fail original+test seam, current
  core26 pass. fmt/clippy,119-source provenance, replay and independent review
  all pass. [Evidence](checkpoints/maintenance-lock-wait-2026-09-26/README.md).
  All B7 processes have ended. Earlier B7 assignments below are historical.
- B8 assigned after that commit. Primary actually reproduced the remaining
  failure and printed its input: serde_yaml serializes arbitrary_precision JSON
  integer3 as {$serde_json::private::Number:"3"}, and loader receives a mapping.
  Literal YAML cap3 works. Exact one-file task: JSON fixture encoding, retain
  original expectations, add literal YAML/native-source byte proof. Snapshot119
  and before file in .rust-migration-runs/b8-web-read-config; local orchestration
  llmbase-b8-web-read-k3, remote llmbase-b8-web-read-config. Full workspace and
  actual release build/hash follow if green. No production parsing change.

- Latest: `cdc8f7d` accepts B6 module wiring and public-only visibility repair.
  Shared fail-closed policy, frozen before/fixed regression, 5 maintenance and
  24 core unit tests independently pass; all 119 inputs match current source.
  Workspace fmt/clippy pass; full no-fail-fast matrix still has three failing
  tests in HTTP jobs and web_read. Six-file replay passes. See
  [the accepted record](checkpoints/native-gate-visibility-2026-09-26/README.md).
  All B6 runners ended; the revision agent timed out after remote completion,
  and primary collected evidence and completed acceptance. Earlier B6 notes
  below are historical and superseded by this record.
- B7 dispatched sequentially after that commit: current-tree post-claim writer
  contention. Five-file ownership, before bytes and coherent 119-file snapshot
  under `.rust-migration-runs/b7-worker-current/`. Local orchestration session
  `llmbase-b7-worker-k3` runs `python3 .../b7-worker-current/run.py`; inspect
  agent.log/agent.exit there. Finite remote tests belong to
  `baypier-prod:llmbase-b7-worker-current` and the matching isolated directory.
  Same-job bounded startup retry, cancellation/fencing and no repeated accepted
  effects are required. Primary retains review, acceptance and checkpointing.
  The first B7 pass ended normally (agent exit 0) with diagnosis and no local
  source changes: pristine full HTTP target failed; instrumented repeated target
  failed 4/5 times, while the single target case passed. WriterLock contention
  class is established; exact internal startup origin needs a deterministic
  regression. Its old remote source contains three diagnostic eprintln lines
  and must not be promoted. The old runner ended. Primary approved adding only
  maintenance_repair.rs and its tests to ownership and dispatched continuation
  `.rust-migration-runs/b7-worker-fix/TASK.md`, preserving seven before files.
  Current local orchestration: `llmbase-b7-fix-k3`, run.py with K3 --continue;
  remote finite session/root: llmbase-b7-worker-fix / b7-worker-fix. The repair
  must retry only bounded pre-acceptance preparation, never the entire operation.
  Superseding user correction: primary owns diagnosis/design and may implement
  difficult repairs. Primary stopped that continuation after checking PID/PGID
  81764; wrapper exit 143, all seven owned files unchanged at handover. Actual
  primary backtraces contradict the startup-only proposal: failures occur in
  repair_slugs snapshot after internal job creation and final receipt saving.
  Primary now implements cancellable bounded acquisition of WriterSession around
  synchronous maintenance sections, dropping it before model awaits. No operation
  or model replay. Only maintenance_repair.rs and its test file changed. Four
  deterministic regressions cover startup wait/cancel/exhaustion and post-model
  contention. Remote before/fixed tests and full gates run in isolated
  b7-primary-fix, tmux llmbase-b7-primary-fix, command bash .../verify.sh,
  logs/runner.log and per-step .exit files. Separate focused read-only review
  is assigned. No B7 acceptance or commit yet.

- `6f2b5d9`: B5.1 inferred compile-helper registration repair accepted. Fresh
  installed wheel: 575 full Python and 25 focused tests pass. Primary's six
  native utility tests and exact-byte journal/overlap verification pass; both
  new regressions fail against the predecessor for the expected registry
  pollution. All 48 installed Python files match source; three-file replay
  passes. Unchanged single-site proof now passes taxonomy write and bake
  (133 JSON, 112 HTML, no skips). See
  [the record](checkpoints/helper-registration-2026-09-26/README.md).
  All B5.1 local and remote runners ended. This is the sixth accepted bundle,
  not a full source release or dual-site/production acceptance.
- B6 native gate repair is now assigned, predecessor `6f2b5d9`: five-file
  ownership for compat_maintenance module wiring, two formatting fixes and a
  recovery-test type declaration. Existing test assertions/lint strictness
  remain. Local session `llmbase-b6-core-gate-k3`; task, command wrapper and logs
  are `.rust-migration-runs/b6-core-gate-fix/{TASK.md,run.py,agent.log,agent.exit}`.
  Finite remote runner `llmbase-b6-core-gate` uses the matching isolated
  baypier-prod migration directory. Full native tests/release follow these
  observed blockers. The primary owns review, independent verification and
  the next Git checkpoint.
  First remote pass: fmt and workspace all-target clippy pass, compat_maintenance
  4/4 and core library 24/24 pass. Full workspace gate fails two HTTP background
  maintenance job tests with "another writer is active" before release build.
  The s8 binary probe uses a trailing `true`; its exit 0 is not build evidence.
  Primary review also found the newly exposed compat_maintenance public-only
  reader duplicates a weaker visibility predicate; revision is prepared in
  REVIEW-REVISION.md, not yet dispatched. No B6 checkpoint is accepted yet.
  A local core-wide diagnostic was started contrary to the remote-only rule;
  it ended with web_read truncation failure before primary revalidation, so
  primary sent no signal. Remote tests remain authoritative. The implementer
  reused the local wrapper's tmux name for its driver, orphaning the main agent;
  agent.exit is stale. Track the real PID from agent.pid and final log instead.
  The finite remote runner and local collection driver have both ended.
  First-pass main agent is now confirmed absent, with final report/log; its
  stale agent.exit was not used as evidence. B6 review revision is assigned
  under .rust-migration-runs/b6-core-review/, local wrapper
  llmbase-b6-review-k3, remote runner llmbase-b6-core-revision. It owns only
  compat_maintenance.rs and its regression test, preserves prior changes, and
  must use the shared fail-closed policy and remote --no-fail-fast test matrix.
  A fresh first-pass release was subsequently built (separate diagnostic d3),
  SHA256 5c043109012341241abfa1a2256b9f22195b4efc2ffffd95de6711ddb701ecd0;
  it is available for a diagnostic no-Python proof but not full release acceptance.
  Primary pinned that exact binary and ran the existing Linux chroot CLI proof:
  31 lifecycle steps passed, exit 0, with only the ELF/shared libraries inside
  the runtime and deterministic HTTP fixtures outside. This does not yet prove
  HTTP/MCP inside the same interpreter-free runtime. Frozen lexical evaluation
  also passed against retrieval-legacy-v1.json: raw/compiled held-out 30 queries
  each, Recall@5=1.0 and MRR@10=1.0, no required/comparison failures, five search
  repeats and 20 performance repeats. Real embedding semantic quality remains
  unverified. Evidence is .rust-migration-runs/b6-primary/artifacts/; the two
  finite remote sessions llmbase-b6-standalone and llmbase-b6-retrieval ended.
  Review revision's initial API connection failures recovered without restart.

- `6ece0ae`: worker contention recovery package; 10 worker and 7 HTTP tests
  independently passed on the historical coherent snapshot. See
  [its exact limits](checkpoints/worker-contention-2026-09-26/README.md).
- `62f5a70`: current core compilation restored by YAML module wiring and the
  null mapping-key compatibility revision. Independent CURRENT-source checks:
  core check, 8 document contracts, 17 core library unit tests, scoped fmt and
  exact recovery-package replay all pass. All 108 source hashes matched. See
  [the acceptance record](checkpoints/core-yaml-wiring-2026-09-26/README.md).
- `939e885`: B2 query/compile fail-closed visibility accepted. Baseline query and
  compile each failed two new regressions; independent fixed runs passed 12 query
  and 9 compile tests, scoped fmt and replay. All 108 current source hashes match.
  See [the record](checkpoints/query-compile-privacy-2026-09-26/README.md). The
  implementation wrapper timed out after writing its report; independent results
  establish acceptance. B2 implementation and remote test sessions have stopped.
- `d50f7e3`: B3 engine DB/lock/artifact write protection accepted after primary
  review found a HIGH case-alias bypass in the first pass. The delegated revision
  rejects ASCII case variants portably. Its added test fails the first pass;
  independent final verification passes seven tests, scoped fmt, exact 108-file
  source provenance and two-file package replay. See
  [the record](checkpoints/reserved-write-paths-2026-09-26/README.md).
  All B3 implementation/revision/test sessions have exited.
- `40d8bde`: B4 durable exact-byte journal and recovery accepted. Independent
  compat_io 7 and core library 24 tests pass, scoped fmt passes, all 108 source
  hashes match and the four-file recovery package replays exactly. Primary
  review required extra proofs of nonempty overwrite history, schema-v1 data
  preservation and replay under a held file lock; those now pass. See
  [the record](checkpoints/atomic-journal-2026-09-26/README.md). All B4 local
  implementation/revision and remote test sessions have exited.
- B5 implementation has completed; its adoption proof remains unaccepted.
  Task: `.rust-migration-runs/b5-integration/TASK.md`; local implementation
  session `llmbase-b5-k3`, remote runner `llmbase-b5-integration`, new isolated
  root `baypier-prod:/root/llmbase-rust-migration-20260920/b5-integration`.
  Primary independently matched all 3,151 copied Siwen code/test/static files
  against the captured original and matched all required copied Rust/Python/test
  inputs to current llmbase bytes. Receipts are under
  `.rust-migration-runs/b5-primary/`. The implementation session was confirmed
  live at 05:50 Asia/Shanghai; no build/test acceptance is inferred from that.
  The first runner stopped before Rust compilation because remote Python lacks
  ensurepip. Its logs were independently preserved in
  `.rust-migration-runs/b5-primary/initial-run/`. The implementer corrected venv
  setup using --without-pip plus the existing isolated pip bootstrap. Remote
  session `llmbase-b5-integration` was confirmed live after its 05:53 restart,
  compiling llmbase-core/PyO3 for a fresh wheel. No wheel acceptance yet.
  That wheel has now built and installed: llmbase Python suite 573 passed.
  Primary independently matched all 48 installed Python files to build input;
  wheel SHA256 7d42ab9283c43dca9f5ef63bfcf49ce7384589d642dbf40ed734503c28098eb7,
  extension SHA256 e082b48f64dddaa98ebd059b2efb3894b7a299a0f9c0cb45da3b53df2bf45a49.
  Adoption is NOT accepted: the actual Siwen proof fails after the read helper
  _list_existing_concepts registers a guessed base/wiki library, overlapping
  the true metadata root and causing taxonomy's atomic write to be rejected.
  Primary reproduced this with the installed wheel in a fresh temporary library;
  see b5-primary/registration-reproduction.json. B5.1 is assigned to remove that
  inferred-registration side effect while preserving real overlap refusal.
  After adding the omitted scripts/, the copied Siwen suite is 184 passed,
  2 failed. One failure needs the small checked-in docs/kukai-t77-volume-map.tsv
  fixture omitted from the snapshot, not the OCR corpus. The Taixu registry
  source-note failure was independently reproduced with Python -I -S and no
  engine import, against unchanged before-snapshot hashes. No test is waived.
  B5's final dual-site proof reports passing but has not been accepted by primary.
  B5 agent.exit is 0; recorded local descendants are absent and no local/remote
  B5 tmux runner remains. No interruption was needed. See
  .rust-migration-runs/b5-primary/handoff-core-first.md.
  B5.1 local session llmbase-b5-registration-k3 has ended; task and logs are in
  .rust-migration-runs/b5-registration-fix/. Its remote runner is
  llmbase-b5-registration under the matching isolated migration directory.
  The paragraph utility migration is deferred to
  B6. This integration pilot does not imply full native parity or production
  readiness. The earlier conditional pilot estimate is withdrawn after the
  confirmed framework defect. Re-estimate after this bounded repair and a fresh
  native workspace gate; the broad historical estimate is not a promise.
- The user authorizes replacing demonstrably unreasonable inherited internals.
  Each redesign must have a concrete defect, bounded ownership and acceptance
  tests, while preserving necessary downstream contracts and original data.
- Latest user steering: Siwen and Taixu may adapt to a robust framework. Prioritize
  sound core contracts; explicitly document any changed legacy integration and
  its replacement instead of adding core special cases. Keep required generic
  capabilities and all native/data-safety gates. Original downstream edits and
  production switching remain outside this task's authorization.
- Implementation source remains largely untracked; the six commits preserve
  finite replayable task packages and original files, not a complete source
  release. Preserve all existing work. No push, version bump or publication.
- Fresh native diagnostic (B6 snapshot, 109 initial files plus required fixture
  and baseline supplement): fmt fails in catalog.rs/maintenance.rs; all-target
  clippy then reproduces the unexported compat_maintenance test target and a
  recovery unit-test type_complexity warning. Full workspace tests and release
  build are not yet reached. Primary's first diagnostic omitted compile-time
  fixtures, corrected in the same isolated snapshot after that run stopped;
  initial logs are retained separately. B6 repair is now assigned after B5.1
  acceptance. No native diagnostic runner remains active.
- Siwen project remains `/Users/lanchair/Developer/Buddha/Github/siwen`.
  Siwen and Taixu share code with separate libraries/instances. Remote Taixu
  directory `/srv/siwen/wenku-fo` and installed llmwiki0.8.0 were verified on
  baypier-prod. Candidate work stays under the separate migration root; no
  production cutover or downstream edits are authorized.

User scope update2026-09-20: README and release after the Rust upgrade are now delegated to this task. Complete the existing acceptance gates, then update version/README/changelog, perform mandatory staged Codex review and publish through the repository's required channels. Existing no-commit/no-push/no-version-bump restrictions are superseded for that release work only. No production deployment or original Siwen edits are authorized. CLAUDE.md requires git tag, PyPI and ClawHub when the Python package version changes; channel completion must be reported truthfully.

## Prior installed candidate checkpoints (historical)

### Candidate v11, native readers, and remaining direct facade gaps

- Installed candidate v11: **569 Python tests passed (8.65s)**; copied Siwen **89 passed (9.10s)**; actual offline bake **22 required GETs returned 200, 133 JSON and 112 HTML, no skips**. Original Siwen tracked bytes/status and fixture original bytes were preserved. Receipts: `results/python-native-candidate-v11.json`, `results/siwen-candidate-v11.json`. Wheel SHA-256 `4ad0f60e92bad30a114e344585421732eb15296d5da62334d4879deffd1f584e`; loaded extension `86e2952d77fa707da13579fb276732f5c1e2e7780769b3145f06c8eb2837a314`.
- v10's bake failure was real: new private-trail authorization rejected an actually absent trail store. v11 permits only absent or genuinely empty trails publicly; nonempty legacy trails remain private and malformed JSON still fails. No operator token was injected into public bake to bypass this boundary.
- Native HTTP/CLI batch: **26 passed**, including upload5, model6, read3, write1, static-reader3 and transports8; maintenance repair **13 passed**. Raw log `.rust-migration-runs/http-ingest-tests-v2.log`. New `web` serves the existing static React build with SPA fallback and scoped assets; `search serve` restores the lightweight reader and scoped search API.
- Ingest/fetch/plugin/worker gate **19 passed**; compile/search compatibility gate **27 passed**. A new fixture exposed `.original` subtree indexing; the scanner and public compatibility scans now share a reserved-artifact predicate. Original HTML/PDF bytes are stored through Rust write intents, with bounded external parsing kept explicit.
- Live-binding audit found mandatory direct Python helper paths still implemented by the old Python core: utility/config/atomic/worker, lint/resolve, and taxonomy/xici/entities. Their native implementations alone do not prove these direct facades migrated. The three agents own these groups respectively; `contracts/live-bindings-v9.json` records the actual installed bindings. Candidate v11 is therefore **not final parity acceptance**.
- Remote native-check-v2 script is prepared at `/root/llmbase-rust-migration-20260920/native-check-v2.sh`, with logs under `logs/native-check-v2.*`. It will run pinned workspace checks/release, a genuinely interpreter-free chroot lifecycle and the frozen retrieval performance matrix. The initial source transfer is complete; launch awaits the transcription module export/test gate so no new test is silently excluded. No task runtime is serving production.
- GitHub publication permissions are available. ClawHub CLI0.23.3 reports **not logged in**; `.pypirc` and the checked token environment variables are absent. Browser/keyring/trusted-publisher authentication is not yet exhaustively checked. No version bump, commit, tag, upload or release has occurred. All substantive acceptance work continues before any necessary final authentication request.

### Current integration and publication checkpoint

- README is English-first with concise Chinese guidance and explicit native/Python installation and write defaults. CHANGELOG has an Unreleased section; ClawHub SKILL.md is revised with candidate status. Versions remain unchanged; no commit/push/tag/upload has occurred. GitHub reports admin/push access. Publication remains gated on full migration acceptance.
- Isolated candidate v9: **552 Python tests passed**, copied Siwen **89 passed**, populated bake **133 JSON +112 HTML**, zero skips. Receipts: `results/python-native-candidate-v9.json`, `results/siwen-candidate-v9.json`. Later fetch/auth/native maintenance changes require a new installed-wheel run.
- Remote `logs/native-check-v1.exit` is **0**: workspace fmt/clippy/tests and release CLI built. This predates later changes and is not final release proof. Refresh the source before native-check-v2 and the Linux interpreter-free chroot lifecycle.
- `.rust-migration-runs/transport-workflows-v13.log` / `.exit0`: **43 passed** (CLI/HTTP/MCP/SSE19, entities8, repair10, worker6). These exercise actual HTTP fixtures, basic/deep query, CLI format/tone/file-back, failure-state preservation, sections, scoped backlinks, generated maintenance, private partitions, source guards and accepted repairs.
- v11's obsolete unimplemented-kb_compile expectation was updated to authorization-before-model checks; v12's new fixture token was shorter than the existing16-byte minimum. Corrected v13 is green; no permission was relaxed.
- Native `ask` owns evidence-first/raw/read-only/streaming; `query` retains compiled-wiki format/tone/deep/file-back. Native compile all now genuinely re-extracts despite old source receipts; Python keeps the legacy receipt behavior. Ephemeral model/key overrides affect selector/answer while the promotion judge keeps its configured client.
- Native fetch validates every resolved address and pins the socket per redirect. Proxy/special-use IP/size/timeout/redirect/cancel tests passed (integration3+unit5); PyO3 compile passed. Default Python ingest uses this core; requests.get overrides remain trusted Python adapters. See `python-fetch.md`. New Flask authorization/privacy still needs candidate validation.
- Additional entity privacy revocation, taxonomy-preserving repair, precise job selection and legacy HTTP routes are being integrated; they are not covered by the43-test receipt.
- The generic Codex skill validator rejects existing ClawHub-specific frontmatter fields (`version`, `requires`, `install`, etc.). This schema mismatch is not a passing validation. Preserve the ClawHub format and verify YAML plus the actual publisher before release.

Older checkpoints below are historical and do not override these remaining gates.

- Candidate wheel v5, actually installed into an isolated virtualenv with native-required mode and a tests-only multiprocessing harness: **521 passed in3.47s**, `.rust-migration-runs/candidate-full-v5-complete.log` / `.exit0`. This predates the next compile/pipeline additions; v6 is being built for those changes.
- `.rust-migration-runs/combined-v7.log` / `.exit0`: **74 passed** across answers12,compat_compile7,compat_pipeline9,compile10,maintenance9,web_read3,worker6,HTTP/SSE/CLI/MCP18. These include live legacy compile settings, original body preservation, guarded promotion, shared writer scopes, pipeline kernel leases and native finite queue execution.
- Workspace all-target clippy with locked dependencies and `-D warnings`: **exit0**, `.rust-migration-runs/workspace-clippy-v3.log` / `.exit0`; `PYO3_PYTHON` explicitly points to candidate Python3.11+. Earlier clippy failures below were corrected, not waived.
- Local debug native lifecycle preflight **31 steps passed**, `.rust-migration-runs/standalone-preflight-v1/receipt.json`: source→draft→accept→index→worker→lexical/evidence→vectors/cache/hybrid→real SSE→save/promote→export→update/delete→backup/restore/rollback. Only deterministic HTTP fixtures; no provider credential or semantic-quality claim. This run has interpreter-free PATH, but **is not the required Linux chroot release proof**.
- Remote setup `llmbase-rust-setup-v3` and dependency fetch `llmbase-rust-fetch` both **exit0**. Isolated Rust1.98 and Python environment are installed. Ubuntu's missing ensurepip was handled by official get-pip inside the runner venv; the successful script resumed the interrupted download. No system package installation or production mutation.
- Native Web reads now cover the existing frontend config/articles/sources/collections/stats/aliases/export/taxonomy/xici/entities routes with live-file hash guards and scope-filtered private data. Article mutation and other legacy write/model routes remain under implementation.
- Compile/promote inline jobs now atomically enter running state, so general workers cannot steal their intermediate queue records. `job_run_one` executes one durable job through Application with heartbeat, fenced completion, failure records and cancellation. See `jobs.md` for accepted-write cancellation limits.
- Legacy query orchestration, remaining CLI/model maintenance/connectors and actual isolated Siwen candidate/bake proof are still open. No completion gate is inferred from these partial successes.

## Baseline

- Original worktree `/Users/lanchair/Developer/LLM-infra/Github/llmbase`, HEAD `a7c4d9da764462e09c0769ecd8ba548a46d27e83`, Python version 0.9.0.
- Existing untracked `GOAL-lang-contract.md`, `GOAL-rust-refactor.md`, `docs/rust-refactor-research-2026-09-20.md` preserved.
- Implementation worktree `/Users/lanchair/Developer/LLM-infra/Github/llmbase-rust`, branch `rust-core-rebuild`, same starting HEAD. No commits.
- Rust absent from PATH/`~/.cargo/bin`, but found and verified at `/Users/lanchair/.rustup/toolchains/1.98.0-aarch64-apple-darwin/bin`. Rust/cargo 1.98.0, toolchain pinned; MSRV now honestly declared 1.98 instead of an untested 1.85.
- User explicitly authorized local execution as well as remote; first selected `siwen-ecs`, then authorized `baypier-prod` if unavailable. `siwen-ecs` TCP22 connected but SSH closed before key exchange on two attempts (exit255). `baypier-prod` authenticated: hostname ai-studio, Linux x86_64,4 cores,~5.7GiB available RAM,65GiB disk free.
- Remote isolation `/root/llmbase-rust-migration-20260920/{source,logs,cargo,rustup,python}`; source rsync excludes git,venv,node_modules,target,build results. No production directory altered. Remote setup installs only inside runner root; no system toolchain replacement.
- Read CLAUDE.md, README, customization, API, pipeline, Siwen migration docs and relevant source/tests. Old language goal is historical only.

## Checks actually run

From new worktree, using original virtualenv only as the unchanged baseline interpreter:

```
/Users/lanchair/Developer/LLM-infra/Github/llmbase/.venv/bin/python -m pytest tests/test_anchor.py tests/test_search_raw.py tests/test_languages.py -q
```

Exit 0: **37 passed in 0.43s**. Subsequent full baseline and candidate results follow; this initial subset is not the final acceptance record.

Subsequent baseline/full and native runs (all under implementation worktree):

| Command | Result | Raw record |
|---|---|---|
| same baseline Python, `-m pytest -q` with provider credential env unset | exit0,474 passed,36.68s; includes11 new downstream contract tests; not candidate-wheel validation | `.rust-migration-runs/python-baseline.log` / `.exit` |
| `cargo test -p llmbase-core` first slice | exit0,5 tests | `.rust-migration-runs/core-first.log` / `.exit` |
| `cargo test -p llmbase-model` | exit0,2 SSE units+19 offline HTTP tests | `.rust-migration-runs/model-first.log` / `.exit` |
| `cargo test -p llmbase-core` with file journal | exit0,3 recovery units+7 library/application tests | `.rust-migration-runs/core-recovery.log` / `.exit` |
| `cargo test -p llmbase-core` with jobs/vectors | exit0,5 units+7 library+7 vectors | `.rust-migration-runs/core-vector-jobs.log` / `.exit` |
| vector review retest | exit0,7 tests; newer reuse snapshot validation included | `/tmp/llmbase-vector-review.log` |
| `cargo fmt --all` with explicit Rust PATH | exit0 (earlier absolute cargo without PATH could not find cargo-fmt; corrected PATH) | command output |
| model/core all-target clippy first | exit101,model collapsible_if; agent fixing and rerunning, not yet green | `/tmp/llmbase-model-vector-clippy.log` |
| candidate ABI3 wheel v2, installed into isolated venv and run from `/tmp` with `LLMBASE_REQUIRE_NATIVE=1` | exit0,33 tests,4.01s; native search, old raw search and downstream contracts; extension provenance recorded | `results/python-native-search-v2.json` |
| final model adapter tests | exit0,2 SSE units +19 HTTP fixture tests | `/tmp/llmbase-model-final.log` |
| guarded draft compilation | exit0,9 tests,0.24s; includes unindexed external raw edit rejection | `/tmp/llmbase-compile-guard-tests.log` |
| core + CLI integration before answer/catalog addition | exit0; includes real SSE, migration CLI, guarded compile and added CLI stream subprocess tests | `.rust-migration-runs/native-integration.log` / `.exit` |
| pristine HEAD frontend `tsc -p tsconfig.app.json --noEmit` and build | both exit2; independently reproduced9 existing diagnostics | `frontend-baseline.md` |
| minimally fixed frontend typecheck and build | both exit0; no dependency or layout changes | `/tmp/llmbase-frontend-fixed.log`, `frontend-baseline.md` |
| new three-transport draft tests, compile and library tests | first compile exit101 in new answer module's `tokio::select!` syntax; being corrected, not a passed run | `.rust-migration-runs/draft-transports.log` / `.exit` |

Authoritative frozen lexical baseline and full rank outputs: `retrieval-baseline.md`, `results/retrieval-{legacy,native}-v1.json`, `tests/rust_migration/fixtures/retrieval/manifest.json`.30 documents × raw/compiled,36 queries (30 held-out); both legacy and native held-out Recall@5/MRR@10 each1.0; native held-out citation resolution289/289. Native warm p50/p95 raw6.876/7.601ms,compiled6.869/7.564ms, with CLI startup included; legacy runs in process. Small easy fixture, no forced cold-cache experiment and no real semantic evaluation; no language-speedup or semantic-quality gain claimed.

## Implemented work, with remaining integration explicit

- `crates/llmbase-core`: SQLite immutable original revisions, metadata/unknown fields, per-library DB binding, transactional incremental FTS publication, CJK OpenCC/NFKC search-only representation, aliases with explicit ambiguity, original UTF8 spans, tombstones and old-version evidence.
- Accepted source create/update/delete/rename uses DB durable write intent, compare-and-swap hash, fsynced staged file/rename, restart recovery; conflicting external edits remain untouched. Kernel writer lock avoids stale PID ownership. True multi-process kill/restart and Python shared-lock tests still pending.
- Durable jobs queued/running/succeeded/failed/partial/cancelled/interrupted, nonce-fenced leases, heartbeat, explicit retry/backoff, event ledger and finite worker now tested. Legacy JSONL/cache/stage lease native bridge is implemented; next candidate-wheel run validates Python integration.
- `crates/llmbase-model`: real HTTP complete/embedding/SSE production adapter with offline server tests, secret redaction, bounded responses/deadlines/retries/cancel. No actual paid provider used.
- `vector.rs`: durable spec-scoped embedding cache, independently built generations, atomic activation after snapshot verification, real vector+lexical RRF; deterministic fixtures prove engineering only.
- `application.rs`: shared DTO and scope/capabilities; source/job operations recently added, integration tests pending. CLI/HTTP/MCP crate currently building.
- Complete contract inventory in `parity.md` and `contracts/`; still mostly pending parity. Read-only Siwen/riddle audit and11 frozen tests in `downstream.md`; Siwen ongoing independent external changes logged, no changes by this task.
- New draft application guard checks indexed revision and live source bytes under the same writer lock as the accepted write; receipt reconciliation uses the same guard. Historical source reads retain historical titles/filenames and reject old private revisions even when the current revision is public (new privacy regression awaiting run).
- Native compilation now has a scoped Application entry point and additive CLI source/draft/apply, HTTP native operations, and MCP compile_source/get_draft/apply_draft adapters. New transport integration tests are in progress. Operator compilation requires Read+Write+Model+Private because article merge considers existing private material.
- Answer save/promote, native catalog/relations and broader Python KnowledgeBase/ingest facades are under implementation and not yet accepted.
- Active ownership: catalog agent → catalog.rs/tests/docs (frontend task finished); model agent → answers/compile and own adapter modules; downstream agent → PyO3/compat_kb/KnowledgeBase/operations/ingest; parent → application/transports/storage/jobs/recovery/migration/integration/docs.

## Background tasks and inspection

- Remote `baypier-prod` setup and fetch sessions have exited successfully (v3 after the bounded bootstrap retries); logs and paired exit files are in `/root/llmbase-rust-migration-20260920/logs/`. The next finite native check uses tmux `llmbase-native-check-v1`, command `bash scripts/rust-migration/check-native.sh` with isolated CARGO_HOME/RUSTUP_HOME/PYO3_PYTHON; log `logs/native-check-v1.log`, exit `logs/native-check-v1.exit`. Source snapshot must not be modified while it builds.
- Previous local baseline, wheel, frontend, native integration and targeted draft sessions exited; raw logs remain at paths above. New finite retests are started with explicit Rust PATH and CARGO_BUILD_JOBS=2, with paired `.log`/`.exit` records.
- Other tmux sessions seen locally/remotely belong to unrelated work; do not touch.

## Design and references

- User clarified English-first engineering/native defaults, Hanzi/wenyan second priority; preserve foundational Unicode/CJK and all old profile/monkey-patch contracts. Official Rust Unicode identifiers and trait objects permit language-specific data/behavior, but inlined `const` is not a Python-style runtime patch target. No custom Rust syntax/DSL is required.

- SQLite bundled FTS5, original byte spans, immutable source revisions and transactional publication; ADR-0001 records authority tradeoff.
- CJK search representation is separate from originals; Rust OpenCC conversion is being evaluated, with fixed baseline required before tuning.
- Official references consulted: [SQLite FTS5](https://www.sqlite.org/fts5.html), [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/), [Axum](https://docs.rs/axum/latest/axum/), [MCP stdio](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [Cargo MSRV](https://doc.rust-lang.org/cargo/reference/rust-version.html), [ferrous-opencc](https://docs.rs/ferrous-opencc).
- `Cargo.lock` generated with actual resolved dependencies; Rust toolchain/MSRV1.98 pinned. Native3-surface and candidate wheel release builds remain pending.

## Outstanding gates

All section-9 completion gates remain open. No true-provider request or semantic-quality evaluation run. No model credential used. No production/downstream files changed. Current background setup/test tasks above are temporary; no deployed application runtime.
