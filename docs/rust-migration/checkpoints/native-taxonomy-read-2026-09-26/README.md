# B13: native taxonomy reads and value helpers

Accepted 2026-09-27 after checkpoint `06d2898`; task started 2026-09-26.
Seven default Python entry points now delegate to real Rust taxonomy reads,
fallback grouping and localization. Config loading remains an explicit Python
boundary. Live helper replacements retain Python orchestration, original
callback exceptions and truthful trace attribution, including recursive wrappers.
This does not migrate taxonomy generation, writes or standalone mutable helpers.

K3 implemented the core/facade/test draft and reached the explicit35-minute
deadline (exit124, actual process absent). Primary retained that handoff, reviewed
it, independently reproduced defects, repaired them and completed acceptance.
No successful K3 handoff or full release is claimed.

## Evidence

- Frozen actual-old oracle:76 pure cases and3 custom-root libraries, including
  cached/uncached/missing data, five language views and unchanged source hashes.
- Before repair:7 native tests passed/2 failed. File-derived `tags: [42]`
  caused a panic; multiple individually valid files bypassed the64MiB aggregate
  read bound. The same tests pass after shared input validation and bounded reads.
  Uncached builds reuse already-read titles; cache reads remain bounded to4MiB.
- fmt, all-target Clippy for core/Python and all9 taxonomy native tests pass.
- Fresh final wheel in candidate-v2:57 focused tests and604 full Python tests pass.
- Independent357-case fixed-seed comparison with the saved old implementation
  passes; no mutation of caller values or inferred library registration.
- Ten ABI checks cover malformed file metadata, aggregate reads,16MiB input
  limit, config exceptions, unchanged files/directories and stale trace rejection.
- All six effective build helpers were independently replaced: outputs, argument
  types and invocation counts match the old implementation. Six exception identity
  checks pass with one invocation and no retry/stale trace. This review exposed
  and repaired a recursive `_localize_tree` wrapper bypass.
- All219 build inputs and48 installed Python files match the current worktree.
  Only the facade and its regression test changed after native acceptance;
  the other217 inputs and the extension hash are unchanged. The reuse verifier
  checks this before reusing those native results; they were not rerun gratuitously.
- Cargo.lock adds only `unicode-case-mapping=1.0.0` and its core dependency entry.
  No existing package version or checksum changed.
- Wheel SHA256:6977bb2c5ca73e4934ba4118918e08c2afea93a86516cf1c1c775355d72b40d5.
- Extension SHA256:8bb7d1c85ef2ef19f50df6fce8a20fc1bd233333aec096fb277f2f2a03b174f4.

Remote:baypier-prod:/root/llmbase-rust-migration-20260920/b13-taxonomy-read.
Final command:`B13_REUSE_NATIVE=1 bash primary/launch.sh`, finite tmux
`llmbase-b13-taxonomy-read`; runner exit0 and session ended. The prior regression
and independent-check sessions also ended. No provider requests, real corpus,
production changes or downstream worktree edits. K3 ran short local incremental
checks; the independent regression, candidate build and full suite ran remotely.

Earlier failures are retained separately: the malformed-tag/read-budget
regressions, one Clippy warning, a primary verifier's mistaken helper name, and
the live recursive wrapper discrepancy. Frozen expectations were not altered.

## Recovery and limits

This is an11-file recovery checkpoint, not a complete source release. Extract
baseline-files.tar.gz outside any Git worktree, apply change.patch and compare
files-sha256.json. Exact before/after byte replay passes. The seven legacy
algorithm bodies and12 other taxonomy functions are unchanged; the retained
legacy bodies serve unbuilt development and explicit override boundaries.

The native task uses the existing document parser and trusted local filesystem
ABI. It adds no remote caller-selected path operation, write or inferred library.
Generation/assignment, maintenance/config/worker parity, complete transport
contracts, final native release checks and latest-candidate Siwen/Taixu acceptance
remain required. Earlier single-site/bake results are not dual-site acceptance.

Primary staged review: LGTM, no unresolved HIGH findings in this bounded change.
TypeScript noEmit and Python lint import passed before commit. See evidence/ for
commands, source hashes, provenance, original logs and independent verifiers.
