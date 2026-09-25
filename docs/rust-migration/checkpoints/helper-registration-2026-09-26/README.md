# Transient compile-helper configuration

Accepted bounded repair after `40d8bde`; this is a replayable source checkpoint,
not a complete Rust migration release. The ongoing migration working tree is
preserved and the three source files remain applied there.

Calling a read helper with a concepts directory used to register its inferred
parent as another library. Its metadata root overlapped the real library, so a
later ordinary taxonomy write failed with "multiple configured libraries".
The helper now reads a transient configuration without publishing that inferred
identity. Public `load_config` still registers explicitly loaded libraries;
genuine overlapping registrations still require an explicit writer context.

Changes are limited to `llmwiki/config.py`, the implicit configuration branch
in `llmwiki/compile.py`, and two native utility regressions. No Rust runtime,
conflict policy, downstream source, proof assertions or production data changed.

Validation on baypier-prod in isolated `b5-registration-fix`:

- Newly installed wheel: full Python suite **575 passed**; focused utility,
  compile and downstream contract tests **25 passed**.
- Primary independently ran all six native utility tests: **6 passed**. Both
  new regressions fail against the predecessor's installed wheel specifically
  because inferred libraries appear in the registry.
- Primary independently exercised populated raw/concept/index reads, unchanged
  registered configuration and original source bytes, no database from reads,
  two exact before/after BLOB+hash journal records in `applied` state, genuine
  overlap refusal and successful explicit writer selection.
- All **48 installed Python source files** match the current source bytes.
  Only the three owned source/test files differ from the earlier B5 snapshot.
- The unchanged single-site proof now passes its real taxonomy callback/write,
  Flask extension and bake flow: **133 JSON, 112 HTML, zero skips**, clean output.
- TypeScript check, local Python lint import and installed-wheel lint import
  pass. Exact three-file baseline/patch replay passes (`replay.json`).

Wheel SHA256: `0ee48f574d3303bbf377ef8cd935893e04b1eb0e30f3b32687486a4b05bd6c72`.
Extension SHA256: `e082b48f64dddaa98ebd059b2efb3894b7a299a0f9c0cb45da3b53df2bf45a49`.
The extension is byte-identical to B5 because Rust inputs did not change.
The wheel was built again with the repaired Python files and installed in a
new candidate environment, outside the source package.

The retained single-site receipt's `external_network: blocked` denotes its
Python socket guard and explicit offline model callbacks. It is not an OS-level
network-isolation claim for arbitrary native code or real-provider validation.

The first implementation test run caught a missing Path import in the new test;
the corrected test and manifest were rerun without changing installed modules.
Primary's first extra diagnostic used an unsupported flat raw-file layout;
it was corrected to the existing raw/source/index.md contract. Its first local
import used an environment missing python-frontmatter; the configured project
environment and actual installed candidate both pass. These harness failures
are retained in private run evidence and are not product acceptance exceptions.

Replay into a scratch tree: extract `baseline-files.tar.gz`, verify the before
hashes in `files-sha256.json`, run `git apply --check change.patch` and then
`git apply change.patch`, and verify after hashes. Do not apply blindly over an
unrelated dirty tree. The archive preserves the required pre-repair files.

Full native workspace checks still have separately reproduced formatting,
module-wiring and test lint failures; no current-tree native release acceptance
is claimed. Dual-site proof review, interpreter-free lifecycle, migration,
performance and publication gates remain. All B5.1 implementation/test runners
have ended. Next task addresses the bounded native gate failures.
