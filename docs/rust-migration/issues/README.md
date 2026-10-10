# Rust major handoff — 2026-09-27

2026-10-10 next bounded blocks: [C2 privacy/evidence diagnostic](../results/c2-privacy-evidence-2026-10-10/README.md)
records a downstream classic privacy blocker in #19;
[C3 typed Rust worker outcomes](../results/c3-worker-outcomes-2026-10-10/README.md)
implements the first #13 slice. These do not close the parent issues or authorize
site adoption. Stop after each accepted block; the broader Goal remains paused.

2026-10-10 bounded progress: [C1 observed library identity](../results/c1-dual-identity-2026-10-10/README.md)
is accepted within design 01 / issue #10. This verifier-only block passed focused
and frozen dual-site checks. Issue #10 remains open; stop after this block until
the user requests the next one.

**Paused; unreleased.** The user requested saving complete WIP source and explicit issues instead of continuing development. The branch is `rust-core-rebuild`; accepted B16 evidence remains checkpoint `0229032`. This handoff introduces no engine feature and does not certify the assembled source as a release candidate. The latest B16 receipt records 651 installed Python tests; current dual-site/final-release gates remain open.

## Architecture and scope

[ADR 0002](../../adr/0002-clean-rust-major-and-extensions.md) defines the clean major: Rust generic services, per-library profiles and explicit Rust behavior extensions, with optional documented Python transition support. Existing legacy parity logs are historical contracts to classify, not an instruction to port every private helper before native adoption. Generic functionality, source bytes, CJK correctness, isolation and recovery remain required.

## Execution

Start with 01 after an explicit user resume. The owner diagnoses the specific gap and supplies the issue contract to an implementer; accept and commit one bounded change before moving on. If a saved assumption is contradicted by code or the site inventory, amend the issue with evidence before expanding implementation. K3 may implement when available; do not blindly retry its former connection failures. The primary owner reviews and verifies. No implementation agent or long build was started for this handoff, and the Goal remains paused.

Use an isolated remote directory for long builds/tests; the previous runner was `baypier-prod:/root/llmbase-rust-migration-20260920`. Never use production library paths. The existing native CI remains enabled for future work; this archival source commit uses `[skip ci]` to avoid starting a long build now. This is not a passing CI result.

## Queue

[GitHub tracker #23](https://github.com/Hosuke/llmbase/issues/23) links all 13 saved tasks. Local Markdown bodies retain the detailed designs if GitHub is unavailable.

- Design 01: [#10](https://github.com/Hosuke/llmbase/issues/10) — [Rust: freeze the Siwen and Taixu integration contract on the current candidate](01-dual-site-contract.md)
- Design 02: [#11](https://github.com/Hosuke/llmbase/issues/11) — [Rust: add an application-scoped v1 extension API and an external example](02-rust-extension-api.md)
- Design 03: [#12](https://github.com/Hosuke/llmbase/issues/12) — [Rust: define and emit the ten library-scoped lifecycle events](03-lifecycle-hooks.md)
- Design 04: [#13](https://github.com/Hosuke/llmbase/issues/13) — [Rust: make worker outcomes explicit and stop treating swallowed errors as success](04-typed-worker-outcomes.md)
- Design 05: [#14](https://github.com/Hosuke/llmbase/issues/14) — [Rust: move supported worker schedules into durable per-library state](05-native-scheduling.md)
- Design 06: [#15](https://github.com/Hosuke/llmbase/issues/15) — [Rust: finish the shared deep-lint and repair application contract](06-maintenance-services.md)
- Design 07: [#16](https://github.com/Hosuke/llmbase/issues/16) — [Rust: finish taxonomy, introduction and entity generation through shared services](07-catalog-generation.md)
- Design 08: [#17](https://github.com/Hosuke/llmbase/issues/17) — [Rust: expose the seven connector workflows through the existing process adapter](08-connector-wrappers.md)
- Design 09: [#18](https://github.com/Hosuke/llmbase/issues/18) — [Rust: publish and test the supported CLI and MCP v1 contracts](09-cli-mcp-contracts.md)
- Design 10: [#19](https://github.com/Hosuke/llmbase/issues/19) — [Rust: verify HTTP contracts and the existing frontend against both site profiles](10-http-ui-integration.md)
- Design 11: [#20](https://github.com/Hosuke/llmbase/issues/20) — [Rust major: define the optional Python transition surface and migration guide](11-python-transition.md)
- Design 12: [#21](https://github.com/Hosuke/llmbase/issues/21) — [Rust: accept one immutable release candidate across recovery, performance and both sites](12-release-candidate-acceptance.md)
- Design 13: [#22](https://github.com/Hosuke/llmbase/issues/22) — [Rust: publish the accepted major version with matching packages and migration documentation](13-major-release.md)

## Resume criteria

The user must explicitly request further work (or use `goal resume` for automatic continuation). Announce any resume. No production change, new paid-provider call, version bump or release publication was performed during this handoff. Final gates are saved in 12 and publication in 13; no remaining-duration promise replaces their evidence.
