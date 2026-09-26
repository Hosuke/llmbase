# Rust: publish and test the supported CLI and MCP v1 contracts

## Starting point
Native CLI and MCP work and have transport tests; full legacy flags/exit behavior and the 19-tool MCP surface are not accepted as one current candidate.

## Deliverable
Record a versioned supported command/tool manifest, including arguments, defaults, output envelopes, stable errors, exit codes and capability requirements. Classify old entries as retained or replaced with an exact migration command. Use application descriptors for built-in and registered operations; avoid divergent dispatch logic.

## Acceptance
Run table-driven contract checks on the same binary for every declared command/tool. Include malformed input, unknown operation, wrong library, denied model/private/write access, cancellation and provider error. MCP stdout contains protocol traffic only; client disconnect cancels cancellable work. Unsupported old entries produce a useful migration error.

## Boundary and dependencies
Depends on 02 and the operations delivered by 04–08. Own CLI/MCP adapters, manifest and transport tests. No requirement to reproduce undocumented private Python quirks; no HTTP/UI changes here.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
