# Rust: freeze the Siwen and Taixu integration contract on the current candidate

## Starting point
B16 is accepted at checkpoint `0229032`, but its 651 Python tests are not current dual-site acceptance. Earlier Siwen bake and standalone proofs use older candidates. The existing `scripts/rust-migration/prove_dual_site_candidate.py` is diagnostic; its library identity and global monkey-patch assumptions must be checked.

## Deliverable
Own only the dual-site verifier, sanitized fixtures and `docs/rust-migration/integration-pilot.md`. Use copies of actual downstream entry code and isolated synthetic libraries. Record upstream/downstream commit IDs, the engine commit, binary/wheel hashes and loaded native module path. Define the minimum site contract for read/search, evidence, English/wenyan profiles, compilation/review, hooks/jobs, and static export. Classify each mismatch as engine defect, downstream adaptation, or deliberately replaced legacy behavior; name the replacement interface.

## Acceptance
- Both distinct library IDs work with the same source slug and different sentinel content; wrong-library/private access is denied.
- Exercise each site's real entry path for read/search/query and bake; report unsupported steps explicitly, not as passes.
- Read-only operations leave source hashes unchanged; generated artifacts belong only to isolated output directories.
- Produce a small machine-readable pass/fail matrix plus a concrete adaptation list. Missing Taixu application code is an explicit prerequisite, never a fabricated pass.

## Boundary and dependencies
First task after an explicit resume; no production switch, original-site edits or broad core rewrite. This task identifies integration requirements; later issues implement them.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
