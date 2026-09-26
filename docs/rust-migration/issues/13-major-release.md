# Rust: publish the accepted major version with matching packages and migration documentation

## Starting point
This branch is an unreleased WIP. Local Cargo development version and published Python version are not a final release decision.

## Deliverable
After issue 12 passes, choose one documented release version/package mapping. Build from a clean checkout of the accepted commit using pinned dependencies; verify source distribution and wheels include required Rust sources. Update README, CHANGELOG, customization/extension docs, deployment/rollback runbook and the llmwiki skill to match the supported v1 contract and tested platforms. Prepare release artifacts with checksums and an explicit compatibility-break list.

Then commit/push the reviewed release change and publish matching tag, GitHub Release, PyPI package and ClawHub skill as required by repository policy. If credentials or a publishing service are unavailable, retain a concrete prepared release and report that surface as unpublished.

## Acceptance
Fresh installation from each published surface resolves the intended version and native engine, README commands work, checksums match, and release notes link issue 12 receipts and the two-site migration recipe. Supported platform/interpreter requirements and unverified provider/embedding claims are accurate. Do not claim completion on a tag alone.

## Boundary and dependencies
Depends on 12 and a reviewed release diff. No production switch, downstream deployment or unrelated package/name change. Future fixes are Rust changes through the documented extension/core boundary, not hidden site-specific patches.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
