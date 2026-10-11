# Rust: add an application-scoped v1 extension API and an external example

**Implemented and accepted in C4, 2026-10-11.** See the
[public API guide](../extensions.md), [external example](../../../examples/rust-extension/README.md)
and [357-test acceptance record](../results/c4-rust-extensions-2026-10-11/README.md).
The original design below is retained for traceability. Native worker extension
outcomes, lifecycle hooks and scheduling belong to #13, #12 and #14 respectively;
this checkpoint does not publish the major version or authorize a site switch.

## Starting point
`crates/llmbase-core/src/profile.rs` already implements language profiles, and `application.rs` owns authorization and built-in dispatch. There is no general native extension registry. Use ADR 0002 as the architecture decision.

## Deliverable
Add a small public Rust API configured at Application construction. An extension descriptor contains a unique namespaced operation name, contract version, input/output schema and required capabilities. Its async handler receives validated input plus a library-scoped context with cancellation; shared services retain authorization and journaled writes. Reject duplicate names and built-in collisions. Treat in-process extensions as trusted Rust code, not a sandbox.

Keep prompt/language values in validated per-library profiles. Provide an external example crate that registers one read operation and a single-section wenyan profile without editing the engine. Register operations once in shared dispatch so CLI/HTTP/MCP can expose the same descriptor.

## Acceptance
- Example compiles against the public crate surface, with no private imports.
- Two applications and two libraries cannot inherit each other's registration/profile state.
- Capability denial occurs before handler invocation; built-in collisions and unsupported versions fail explicitly.
- Cancellation reaches the handler; failed writes use the normal journal/recovery path.
- English remains the default and a wenyan bare-body profile preserves original source bytes.

## Boundary and dependencies
Depends on 01's site requirements. No dynamic Rust ABI, hot loading, WASM runtime or reproduction of Python global patch points. Freeze only this minimal v1 API, not an extensibility framework for hypothetical use cases.

## Working rule
Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
