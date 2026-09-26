# B12: native paragraph normalization

Accepted after355db6f. normalize_paragraphs now runs a real Rust state machine
through the existing pure utility FFI. The facade reads current sentence
terminators/closing wrappers on every call; native failures propagate. Legacy
unbuilt-development behavior remains. Heading normalization is unchanged.

K3 exited1 before editing because OAuth TLS setup failed. Primary implemented,
reviewed and verified this task under the user's takeover authorization. The
failure log is retained; no K3 success is claimed.

## Evidence

- Frozen229 cases from the unchanged baseline Python body, including38 existing
  tests, Markdown containers/fences and Python Unicode line/space semantics.
- Native fmt/Clippy and five utility/paragraph tests pass, with actual transform
  comparison against all229 cases.
- Fresh installed wheel:65 focused and595 full Python tests pass. The native
  tests poison old algorithm helpers and check live settings/error propagation.
- Independent1200-case fixed-seed differential comparison passes against the
  saved old Python body. Actual native receipts and no library registration.
- All214 build inputs and48 installed Python files match the current worktree.
- Wheel SHA256:0bd61af1b9b3f38af563bad4ac3d86197062531ce31c494d862acf2866950f73.
- Extension SHA256:8cf8173360d3bc20647035ea126180e3728bd7a430ca8e9f6e1b32d3cb4583c2.

Remote:baypier-prod:/root/llmbase-rust-migration-20260920/b12-paragraphs.
Command:bash primary/launch.sh; finite tmux:llmbase-b12-paragraphs. Runner exit0
and session ended; independent heldout verifier exit0. No local build, provider
request, real corpus or production/downstream mutation. Evidence includes exact
commands, source hashes, wheel provenance, raw logs and results.

## Recovery and review

This is a five-file recovery checkpoint, not a complete source release. Extract
baseline-files.tar.gz into an empty directory outside any Git worktree, apply
change.patch and compare files-sha256.json. Exact replay passes. Initial replay
failed because the packager split literal Unicode separators inside JSON; the
included corrected packager uses LF patch boundaries. Oracle/runtime bytes were
not changed. The original failure is retained.

Primary review: LGTM, no HIGH findings. TypeScript noEmit and Python lint import
pass. Broader facade/transport/downstream/release gates remain. In particular,
the accepted B8 standalone binary predates this core utility change; full native
checks/release and dual-site integration must run on the final assembled source.
