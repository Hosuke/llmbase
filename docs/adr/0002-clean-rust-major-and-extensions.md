# A clean Rust major with explicit downstream extensions

Accepted direction, 2026-09-27. The user prioritizes a clean, efficient, easy-to-integrate Rust major: the generic lifecycle and authoritative storage belong in Rust, configurable language/prompt policy belongs in per-library profiles, and custom behavior belongs in explicit versioned Rust extension interfaces. English is the default engineering/API language; Unicode, original-byte evidence and a single-section Classical Chinese profile remain first-class requirements, without altering Rust syntax. Python is an optional transition/format host with a documented supported surface; historical private-helper or global monkey-patch fidelity is not by itself a native-major release gate, and every changed documented integration needs an explicit replacement and downstream acceptance.

## Consequences

Preserve the full generic lifecycle, source preservation, library isolation, recovery and publication gates. Use application-scoped trusted in-process Rust extensions first, with no stable dynamic Rust ABI, hot loading or site-specific branches in the core. This is a target contract, not a claim that the extension registry is implemented. Existing accepted compatibility behavior and frozen evidence stay intact until a reviewed migration change explicitly replaces them. Siwen and Taixu may adapt to the public contract; this decision authorizes neither original-site edits nor production cutover.

Work is paused for quota conservation. The [issue handoff](../rust-migration/issues/README.md) is the authoritative next-work queue; older chronological plans remain evidence, not automatic authorization to resume.
