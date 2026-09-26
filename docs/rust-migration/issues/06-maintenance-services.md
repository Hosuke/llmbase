# Rust: finish the shared deep-lint and repair application contract

## Starting point
Native maintenance, model-assisted maintenance and repair implementations already exist in maintenance*.rs. B11 and later deterministic lint work are accepted. Remaining gaps include deep lint, fixes/dedup orchestration and some Python entry paths.

## Deliverable
Route deep lint and supported repair steps through typed application commands, reusing existing native algorithms: garbage cleanup, metadata repair, alias-aware broken-link repair, duplicate merge and uncategorized repair. Separate proposed changes from apply; use expected hashes and the existing journal for mutation. Any taxonomy generation calls the shared catalog service rather than a second implementation.

Document which legacy functions delegate, which are replaced by these commands, and which optional Python callbacks remain host-side. This is not an instruction to port every private helper.

## Acceptance
Focused tests cover stale-hash conflict, private visibility, no-op, malformed/truncated model response, partial provider failure and source preservation. Duplicate merge requires a reviewable proposal and cannot silently discard content. Same command through direct core and one transport produces the same outcome; denial occurs before model call/write.

## Boundary and dependencies
Depends on 01/02 and uses 03's event contract. No broad lint rewrite and no new heuristics unrelated to a reproduced gap.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
