# B11: native lint facade and live callback acceptance

Accepted bounded change after a27595c. Rust now executes the default article
load and eight lint checks, stage ordering, issue total and emit stage through
the installed Python extension. Live Python overrides remain callable; the
actual extracted Siwen cfg-only check executes once. Taxonomy generation,
config setup, standalone check functions, deep lint and repair/dedup remain
separate unfinished facade work. This is not full dual-site acceptance.

The five-file patch includes the FFI registration/bridge, lint facade and15 new
regression cases. The primary reviewed the implementation, fixed three callback
binding defects in the delegated draft and added exception/trace coverage.
K3 was intentionally stopped after five observed APIConnectionError retries;
its draft and interruption evidence are retained. Its exit143 is not a test
failure or a successful implementation handoff.

## Verified candidate

- Fresh wheel built and installed outside the source tree on baypier-prod;
  REQUIRE_NATIVE=1, actual extension origins and all48 packaged Python files
  checked against source. Complete211-file snapshot matches current worktree.
- fmt, all-target Clippy for core/Python, five native maintenance tests: exit0.
- Focused35 tests and full Python590 tests: exit0; no focused skips.
- Frozen actual legacy lint results match exactly: eight categories,17 issues
  by default and16 with CJK slugs enabled. Fixture bytes remain unchanged.
- Actual unchanged Siwen callback: one invocation, expected result and native
  maintenance_run receipt. This proves that callback, not full site integration.
- Wheel SHA256:838d07753eb153cdd2e35833a78e37e048741529e3f6663ac2b201232b8afdf2.
- Extension SHA256:9d255adaf3320573d21747a5296e6a6e3ba7c9f799bdc4c1ea3d93bad6747ee4.

Evidence, commands and original logs are under `evidence/`. Initial venv setup
failed because ensurepip is absent; `attempt1/` preserves that failure. The
resume runner uses isolated pip --python with --without-pip venvs and retains
already-passing Rust receipts. Final runner exit0; finite remote tmux ended.
No production library, original downstream tree or provider API was used.

Remote root:baypier-prod:/root/llmbase-rust-migration-20260920/b11-native-lint.
Command:bash primary/launch.sh; session:llmbase-b11-native-lint. Per-step logs
and exit files are retained. The earlier B8 full native workspace/release gate
remains historical evidence; this task changed only Python transport Rust files
and did not repeat the unchanged core matrix.

## Recovery and review

`baseline-files.tar.gz` contains the four existing before-files; the new test
is represented by a null before hash. Extract into an empty directory outside
any Git worktree, run `git apply --check change.patch`, then apply and compare
every SHA256 against `files-sha256.json`. This is a replayable task checkpoint,
not a complete source release; the larger migration remains in its worktree.

The primary is the user-authorized reviewer. Review covers argument binding,
live override lookup, single invocation, original BaseException propagation,
native provenance, exact legacy output, source scope and recovery replay.
Required TypeScript noEmit and Python lint import checks pass. No HIGH finding
remains in this bounded change; all broader parity/release gates remain open.
