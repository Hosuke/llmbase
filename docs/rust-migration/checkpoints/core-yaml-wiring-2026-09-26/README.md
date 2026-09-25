# Core YAML wiring checkpoint — 2026-09-26

The current Rust core could not compile: `document.rs` called
`crate::compat_yaml::parse`, but the module was not declared in `lib.rs`.
This bounded task registers that existing parser and tests its document entry
points. Wiring exposed a failing migration contract: the fixture's nested
`null: null` mapping was rejected by the parser's JSON conversion.

The revision accepts a plain null mapping key as the JSON key `"null"` while
preserving the typed `Node::Null` key in `parse_typed`. A mapping containing both
plain null and quoted `"null"` keys fails explicitly in either order, preventing
silent metadata loss. Other non-string mapping keys still require a typed
adapter. Scalar values and parser resource limits are unchanged. The original
migration fixture and assertions were not modified.

## Scope and saved files

Parent Git checkpoint: `6ece0ae`. The migration's runtime source is still
untracked; this commit saves a replayable task package, with the applied source
also retained in the worktree.

- `crates/llmbase-core/src/lib.rs`: module registration and declaration ordering.
- `crates/llmbase-core/src/compat_yaml.rs`: null mapping-key JSON conversion and
  collision rejection.
- `crates/llmbase-core/tests/document_yaml_contracts.rs`: eight focused tests for
  real document parsing, YAML scalar types, UTF-8 original bytes/hash/body
  offsets, malformed input, alias/cycle bounds and null-key compatibility.

`baseline-files.tar.gz` contains the two affected pre-existing files;
`change.patch` reconstructs the changes and adds the new test file.
`files-sha256.json` records before/after hashes (a null before hash denotes the
new file). The package was extracted and applied in a fresh temporary directory,
and all three reconstructed after-hashes matched the working files.

## Acceptance

Implementation was delegated, reviewed and returned once for the specific
null-key compatibility failure. The reviewer did not implement the engine fix.
The first-pass library test failure is preserved in
`evidence/b1-core-test-lib.log` and its exit file (101, 16 passed / 1 failed).
After revision, the primary agent independently ran `evidence/verify.sh` against
the CURRENT source in the isolated directory
`baypier-prod:/root/llmbase-rust-migration-20260920/b1-core-source`.
All 108 Rust/Cargo/toolchain source files matched the local worktree hashes,
with no extra remote source files; the full manifest is in
`evidence/b1-accept-source-sha256.json`.

| Check | Independent result |
| --- | --- |
| `cargo check -p llmbase-core --lib` | exit 0 |
| `cargo test -p llmbase-core --test document_yaml_contracts` | exit 0; 8 passed |
| `cargo test -p llmbase-core --lib` | exit 0; 17 passed, including the unchanged migration contract |
| `rustfmt --check --edition 2024 --config skip_children=true` on the three owned files | exit 0 |
| Local frontend `npx --no-install tsc --noEmit` | exit 0 |
| Local candidate Python `from llmwiki.lint import lint` | exit 0 |
| Archive/patch reconstruction and SHA-256 comparison | passed |

Each independent remote command has a separate raw `.log` and `.exit` in
`evidence/`; the runner's overall termination alone is not a test result.
Only the three assigned source files changed. The primary agent reviewed the
final patch under the user's explicit authorization for direct review; the
disposition is recorded in `evidence/review.txt`.

This accepts the bounded core build/parser-contract task. Full workspace
builds/tests, Python/CLI builds, all YAML compatibility and end-to-end privacy
remain separate gates. No claim of complete YAML conformance or release readiness
is made. Siwen/Taixu code, data and production services were not changed. Task
agents and task-specific remote test sessions have exited.

## Replay

In a fresh scratch directory:

```sh
bundle=/absolute/path/to/core-yaml-wiring-2026-09-26
scratch=$(mktemp -d)
tar -xzf "$bundle/baseline-files.tar.gz" -C "$scratch"
git -C "$scratch" apply --check "$bundle/change.patch"
git -C "$scratch" apply "$bundle/change.patch"
```

Compare reconstructed files to the `after` hashes. This reconstructs this task's
files only; a complete migration checkout is required to build them.
