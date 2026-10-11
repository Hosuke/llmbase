# C4 — application-scoped Rust extension API v1, 2026-10-11

Implements the bounded public API and external example in
[#11](https://github.com/Hosuke/llmbase/issues/11). A host can now register a typed
Rust operation without editing the engine or installing a process-global patch.
The registry belongs to an Application; every operation names its enabled
libraries and required capabilities. Duplicate names, bare built-in names,
reserved namespaces, unsupported versions and invalid enabled language profiles
fail at construction.

The [API guide](../../extensions.md) defines the supported surface and limits.
Descriptors derive schemas from the Rust input/output types. Authorization and
Serde decoding precede handler invocation. Context binds one library and narrows
capabilities to the declared grants, including private access. Its small service
surface reuses native reads and journaled source writes. This is trusted compiled
Rust, not a sandbox, a dynamic loader or a general JSON Schema validator.

Gateway, native HTTP and MCP dispatch registered operations through the same
application; discovery filters them by selected library and required authority.
MCP publishes object input/output schemas and structured results. The standalone
public-only [example crate](../../../../examples/rust-extension/README.md)
provides a real CLI with invocation and `--list`, an English default and a bare-body
Classical Chinese profile without changing the core.

## Acceptance

The final isolated run passes **357 native tests**: 281 core, 49 CLI/transport,
22 model and 5 external-example tests. The recovery test's one nested subprocess
is not double-counted. This includes **18 new tests** (8 core, 5 transport,
4 example library and 1 actual example-binary test). Formatting and clippy with
`-D warnings` also pass. See the saved logs and `validation.json`.

New checks cover:

- Duplicate/version/name/policy rejection and generated input/output schemas.
- Denial and malformed typed input before any handler invocation or storage open.
- Two libraries' explicit enablement, separate applications' registries/profiles,
  narrowed caller grants and private-source denial.
- Pre-cancellation, in-flight cancellation, future drop and parent-token isolation.
- A real MCP `notifications/cancelled` message cancels the retained context;
  subsequent writes fail and the connection still answers ping.
- Accepted source bytes survive an index failure as a pending write intent;
  repairing the fixture and reopening recovers the intent, while an unsafe retry
  cannot overwrite the accepted revision. A later handler failure also retains
  already accepted writes.
- HTTP discovery/dispatch and MCP schema/results preserve library scope and
  authorization. The real example CLI lists its descriptor and reads original
  CRLF/non-BMP/combining-character bytes. English and `lzh` profiles stay separate.

## Execution and review

Remote isolation directory:
`baypier-prod:/root/llmbase-rust-migration-20260920/c4-rust-extensions-20261011`.
Final finite tmux session: `llmbase-c4-accepted`, command
`bash primary/remote-run.sh`. Each step is bounded to 300 seconds, with two build
jobs and lowered priority. All 137 source/build/fixture inputs match their recorded
SHA-256 values before and after the final run. No production library or paid
provider was used; fake providers used remote loopback only.

Primary owned API design/core implementation and recovery/security tests; bounded
agents owned transport integration and the external example. Independent staged
review found that rmcp cancellation did not automatically drop a handler. The
transport now explicitly selects on RequestContext cancellation, with the actual
protocol regression above; re-review returned LGTM with no remaining HIGH/MEDIUM.

The first remote build lacked the CLI baseline fixture. It was added to the
manifest. The next run passed all tests but clippy found two unnecessary `mut`
bindings in tests. They were removed before the final complete run. Earlier logs
remain in the isolated remote `first-run/` and `second-run/` directories and are
not substituted for final acceptance. Final runner/steps all exit 0; C4 tmux
sessions have ended. Local TypeScript noEmit, Python lint import and staged
whitespace checks also pass. No local long build or server was started.

## Remaining boundaries

This completes the minimal #11 contract on the unreleased branch. Stock CLI
configuration cannot load arbitrary extensions: a host must compile registrations
in. Context does not yet expose model calls or nested extensions. Extension jobs
are rejected explicitly pending #13's registered-handler outcome contract.
Lifecycle hooks (#12) and schedules (#14) remain separate work.

Cancellation is cooperative; a running blocking write can finish after the call
is cancelled. Accepted effects are not rolled back on handler failure. There is
no multi-operation transaction or arbitrary-code sandbox promise.

The previous C3 full GitHub CI passed at `513105a`:
https://github.com/Hosuke/llmbase/actions/runs/38032908787.
This checkpoint leaves normal CI enabled for its own commit. Remote native
acceptance does not claim an installed Python wheel or final dual-site acceptance.
C2's downstream classic privacy blocker in #19 remains. No version bump,
publication or site switch is made. Stop after saving this one block; the broader
Goal remains paused.
