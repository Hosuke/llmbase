# Rust migration plan

2026-10-10: the user resumed one bounded block. C1 identity-verifier repair is
accepted with [focused and dual-site snapshot evidence](results/c1-dual-identity-2026-10-10/README.md).
Issue #10 is still open for the remaining profile/evidence/privacy contracts.
Save this block and stop; do not infer authorization for the next issue.

**Current stop, 2026-09-27:** save full WIP and the [13 issue designs](issues/README.md),
then remain paused until an explicit user resume. No further implementation or
long build is part of this handoff. [ADR 0002](../adr/0002-clean-rust-major-and-extensions.md)
supersedes older requirements for exhaustive historical monkey-patch/private-helper
fidelity: the clean major keeps a documented transition subset and replacement
interfaces. It preserves all generic capabilities and data/recovery guarantees.
Older milestones and execution notes below are historical context.

Full objective: the supplied `GOAL-rust-refactor.md` / attachment, including all eight completion gates in section 9, with the user-approved compatibility strategy recorded above. On2026-09-20 the user additionally delegated the README update and release after the Rust upgrade is complete. This authorizes the corresponding version/commit/push/tag/GitHub release work after acceptance; follow the repository's review and publication process. Production switching and edits to the real downstream worktree remain outside scope.

User steering (2026-09-20): prioritize English, then Hanzi/classical Chinese. Public engineering vocabulary/API/schema/errors and new native answer defaults use English; language/profile settings support wenyan and other languages. Unicode/original-byte evidence and CJK correctness remain foundational. Existing legacy trilingual defaults and Siwen dynamic patches remain compatibility requirements. Report retrieval quality by language without changing the frozen query/relevance set. Rust constants are not runtime patch points: configurable values live in profiles, behavior in explicit extension interfaces, and Python facade reads current overrides/callbacks per call.

| Milestone | Acceptance gate | Current state |
|---|---|---|
| 1. Baseline and contracts | Upstream/downstream inventory, fixed fixtures, baseline tests and retrieval scores | Frozen lexical and original Python baseline recorded; original frontend errors independently reproduced and minimally fixed |
| 2. Native vertical slice | Existing files → transactional index → search/get/evidence through CLI, HTTP, MCP | Implemented and tested on all three transports; native Web reads preserve frontend envelopes and private boundaries |
| 3. Durable complete core | Revisions, source mutation/recovery, vectors/cache/generations, jobs, query/compile/promote, maintenance | Native source/index/vector/ask/draft/promote/catalog/projection recovery and finite worker tested; broader legacy model maintenance and connectors remain |
| 4. Compatibility | Candidate wheel, dynamic overrides, Flask factories, all core routes traced into Rust, isolated Siwen/bake tests | B16 installed candidate651 tests pass; lint, paragraphs, taxonomy reads, headings and native config IO/mkdir accepted. Earlier Siwen/bake evidence remains historical; latest-candidate dual-site and remaining facade/worker parity are required |
| 5. Migration and delivery | Copy-first migration/verify/rebuild/backup/restore/rollback rehearsal; no-Python release full chain; all checks/reports | B9 actual no-Python CLI/HTTP/MCP/SSE and B10 copy-first/actual-old-reader rollback accepted. Final assembled release refresh, full source commit and publication remain pending |

Stop point2026-09-27: B16 accepted and checkpointed as0229032; native26+4,
focused63/full Python651, actual-old differential/repaired deleted-cwd proof,
provenance and10-file replay pass. The user explicitly requests pausing after
this task to conserve quota. No next implementation is assigned. Preserve the
whole objective for a later user resume; see PAUSED-2026-09-27.md. Older
config/paragraph pending descriptions below are historical, not current work.

Execution update2026-09-26: work resumed on the user's instruction; the goal
scheduler later reported blocked and requires UI resume for automatic turns.
This does not stop authorized work in the current turn. Implementation is delegated
one bounded task at a time; the primary agent defines scope, reviews, independently
verifies and saves each accepted task before assigning the next. Do not restart
the three old agents. Latest user correction: primary must own actual diagnosis
and concrete design before delegation, and may directly implement a difficult
repair when delegation is not effective. Delegation is not a reason to defer
problem definition to the implementer.
The user explicitly permits primary-agent review for small tasks; additional
review models are reserved for cases that justify their cost. Long builds/tests
run in isolated baypier-prod snapshots under tmux.

Accepted task checkpoints: `6ece0ae` (worker contention, historical-snapshot
verification), `62f5a70` (current core YAML wiring, 25 related tests), and `939e885`
(B2 query/compile visibility, 21 independently passing focused tests), and
`d50f7e3` (B3 reserved atomic-write destinations and case aliases, seven tests).
`40d8bde` adds B4 exact-byte journaling and recovery (7 compatibility and 24 core
unit tests independently passing, including schema-v1 opening and replay locks).
`6f2b5d9` adds B5.1 transient helper configuration: 575 full Python tests,
25 focused tests, independent registry/journal/overlap checks and unchanged
single-site proof now pass. `cdc8f7d` accepts B6 native module wiring and shared
public visibility (5 maintenance and 24 core unit tests independently pass).
`b42def8` adds the primary's bounded maintenance lock repair. `d937550` adds
the invalid numeric-config test fixture correction; complete native workspace
tests, fmt, Clippy and a fresh release build now pass. All nine are bounded
recovery bundles. `944acae` adds accepted real native transports and fixed
same-binary lexical/performance evidence; it changes only the verifier/runbook.
Their
recovery bundles preserve the task deltas despite the untracked full migration.
See status.md and each checkpoint README for exact provenance and limitations.

User steering2026-09-26: inherited designs may be removed or redesigned when
concrete evidence shows they obstruct correctness, adoption or maintenance.
Define the defect, replacement contract, ownership and acceptance first; delegate
one bounded change at a time. Preserve source bytes, library isolation and needed
downstream compatibility. Prefer explicit profiles/callbacks within Rust and a
thin Python compatibility boundary over new global patch points. This is not
authorization for an unbounded rewrite or a production switch.

Further steering2026-09-26: prioritize a sound framework contract; Siwen and
Taixu can adapt their integration to it. Do not burden the core with special
cases solely to preserve unreasonable downstream coupling. Where an old
integration pattern changes, explicitly document the replacement API and the
downstream adaptation, and test the supported contract. Required generic core
capabilities, source preservation, library isolation and native release gates
remain in scope. This does not authorize editing the original downstream
worktree or switching production. A legacy integration failure is evidence to
classify, not automatic justification to redesign the core around that patch.
The inferred-library registration defect was repaired and accepted in B5.1;
current priority is the remaining native facade boundaries and clear integration
contracts, with site-specific adaptation separately listed.

Current next sequence: B7/B8/B9/B10 are accepted and checkpointed. B9's real
interpreter-free CLI/HTTP/MCP/SSE and frozen lexical proof uses the accepted
release CLI f1b2bdd98971...; primary also verified expiry rejection. B10's
copy-first/actual-old-reader proof was independently accepted in `a27595c`:
34 steps, exact original bytes and full job/Evidence/deleted-source identity.
B11 native deterministic lint facade was assigned to K3 with a frozen210-file
snapshot; after five APIConnectionError retries, primary stopped its verified
process and took ownership of the draft. Primary repaired callback binding,
added15 regressions and a frozen actual-old-lint/real-Siwen-callback comparison.
Final211-source remote fmt/Clippy/core-maintenance5 pass. The fresh installed
wheel passes35 focused and590 full Python tests; primary verified48 packaged
Python files and exact frozen old-lint/actual-Siwen-callback comparisons. Final
runner exit0; session ended. B11 is accepted in checkpoint `355db6f`.
Complete and save each accepted task before the next; no broader parity is waived.
B12 paragraph normalization was concretely defined and assigned after355db6f;
K3 exited before edits on OAuth TLS failure. Primary froze229 old-package cases,
implemented the native action/facade and accepted the new214-source candidate:
fmt/Clippy/core5, focused65/full Python595, frozen229 and independent1200-case
comparisons pass. Remote tasks ended. This excludes normalize_heads/taxonomy.
B12 recovery/source delta and acceptance are saved in `06d2898`. B13 taxonomy
read/value facade was concretely assigned to K3. At its explicit35-minute limit
K3 exited124 with core/facade/tests drafted; primary retained the handoff and
took ownership. Two independent regressions reproduce a malformed-tag panic
and absent aggregate read bound. Primary repaired both plus stale config-error
traces, then independently reproduced and fixed a recursive helper-wrapper
bypass. B13 is accepted in `24f34f5`: native9, focused57/full Python604,
independent357 cases/10 ABI checks and all six live helpers plus exception
identity pass. Exact219-source/48-installed-file provenance and11-file replay
pass; final runner0 and all sessions ended. Taxonomy generation/write recovery
remains in the full goal. B14 heading regex
diagnosis and60 actual-old outputs are prepared. After B13 was saved in24f34f5,
primary finalized the shared Rust scanner/native regex/Python matcher boundary
and assigned B14 with a220-input snapshot. Exact ownership/acceptance is in
.rust-migration-runs/b14-heads/TASK.md; it is not accepted yet.
On2026-09-27 primary explicitly stopped this K3 dispatch after three observed
APIConnectionError failures and no implementation. Exit143, actual process
absence and unchanged220 source inputs are verified. Task/oracles/verifiers
remain available; no blind retry is running. A separately reproduced relative
config-root duplication defect was separately retained for repair after B14.
Primary then implemented and accepted B14 in `a36d463`:223-input snapshot,
fmt/Clippy/native5+paragraph2, fresh-wheel focused79/full Python618, independent
547 comparisons/9 failure checks and live callback/override proof all pass.
Exact8-file replay and48-installed-file provenance pass; runner0, finite tmux
and actual pane PID are absent. B15 then repaired relative config roots and was
accepted in `af16b2d`: previous-candidate5 failures reproduced, new focused40/full
Python628 pass, exact native journal/scope proof and original write comparison
pass.225-source/48-installed-file provenance and3-file replay pass. Native inputs
and actual extension hash are unchanged from B14; native checks are reused with
explicit proof. Review fixed an optional-extension import in the new test;
unbuilt collection and final full remote rerun pass. Runner0/session ended.
Next define native config reading/ensure_dirs; this path correction does not
complete that migration or the remaining maintenance/worker/connectors.
Fresh wheel/downstream integration and remaining facade parity still remain;
passing the native test matrix is not full release or production acceptance.
See status.md and the B8 checkpoint for exact provenance and limits.

Historical bounded sequence: B4 known-library atomic journaling and exact-byte recovery
is accepted. B5 built a fresh candidate (573 Python tests passed) but identified
a framework-side inferred-library registration defect. B5.1 removes that hidden
side effect and is now accepted in `6f2b5d9`. Fresh native diagnostics identify
an unwired maintenance test target, a test type lint and two formatting files;
B6 is assigned to repair those, followed by full native workspace/release gates and a precise
core-contract review. Siwen/Taixu pilot results remain diagnostic until accepted;
site adaptation can follow the stable core contracts under the latest user
steering. The two consumers share code and
retain separate data, scope, branding and runtime overrides. Native transcription
already has implementation/tests and is not a new feature task to restart.

Bounded source check2026-09-26: atomic.py already calls _native.atomic_write,
whose FFI reaches compat_io. Complete its journal in B4; do not redo facade
wiring that is already present. config.load_config/ensure_dirs still execute
Python logic, and normalize_paragraphs/normalize_heads have no corresponding
compat_util actions yet. These remaining items require actual implementation,
not merely a new native trace marker. All B4 implementation and remote test
sessions have exited. B5 implementation has exited normally; its pilot remains
unaccepted as a whole; B5.1 is accepted and its sessions have ended. B6 is now
assigned under .rust-migration-runs/b6-core-gate-fix/TASK.md. Local session
llmbase-b6-core-gate-k3 delegates the bounded native gate repair; the finite
remote runner llmbase-b6-core-gate uses the new isolated
baypier-prod:/root/llmbase-rust-migration-20260920/b6-core-gate-fix directory.
The paragraph task remains deferred and not assigned: normalize_paragraphs through the existing pure
utility FFI, with a frozen Python oracle, live terminator/wrapper overrides and
fresh-candidate validation. Its ownership and gates are in
.rust-migration-runs/b6-paragraphs/TASK.md. normalize_heads remains a separate
task because its caller-provided regex contract needs independent treatment.

Final integration still requires refreshed Linux workspace/release checks,
no-Python chroot lifecycle, frozen performance matrix, final installed wheel and
all original completion gates. Candidate v11's569 Python tests,89 Siwen tests and
133JSON/112HTML bake are historical, not current-tree acceptance. README,
CHANGELOG and publisher content remain unreleased. Final source commit, review,
version alignment, push/tag, release assets, PyPI and ClawHub verification follow
acceptance. No production cutover is implied by the active goal. Offline fixture
tests do not claim real-model semantic validation.
