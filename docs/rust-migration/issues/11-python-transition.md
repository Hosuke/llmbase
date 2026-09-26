# Rust major: define the optional Python transition surface and migration guide

## Starting point
The candidate keeps llmwiki imports and many live overrides, but clean-major release requirements no longer imply exact emulation of every historical monkey-patch/private helper.

## Deliverable
Create a machine-readable manifest of documented public imports, CLI entries and downstream customizations. For each, record supported, replaced or retired, the versioned native/profile/extension equivalent, and any required downstream change. Keep the Python package optional for native runtime. Supported facades must delegate core algorithms/storage to Rust; format parsers and explicitly declared callbacks may stay Python.

Use existing frozen baselines as historical evidence. Mark intentional major-version differences explicitly rather than weakening tests silently. Documentation and package errors must not promise unsupported monkey-patch behavior.

## Acceptance
Every documented contract in customization.md/CLAUDE.md and the actual dual-site inventory maps to a manifest entry. Tests prove representative supported overrides are read at the promised time and replaced entry points produce actionable errors. Installed-package tests verify loaded native module location and absence of fallback to the original source checkout.

## Boundary and dependencies
Depends on 01/02 and service/transport decisions. Own compatibility manifest, migration docs and minimal facade changes required by it. Do not port all private helpers or remove generic native functionality under the label of cleanup.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
