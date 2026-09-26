# Accepted native heading normalization

Predecessor: `24f34f5`. Accepted on 2026-09-27 after primary implementation,
review and isolated remote verification. The attempted K3 dispatch ended143
after three connection errors without runtime edits; no K3 implementation is
attributed to this checkpoint.

Rust now scans/reconstructs Markdown headings, including fence and HTML block
state, exact heading groups and legacy newline behavior. Native declarative
profiles use bounded Rust regex rules. The Python facade supplies only its live
Python regex matcher; lookaround, backreferences and compiled flags remain
available in that explicit extension boundary. Eight replaced parser helpers
activate the real Python override path; restoring them restores native execution.
Callback exceptions retain identity, are not retried, and clear stale/nested
success traces. No roots or libraries are inferred or registered.

This is an eight-file source recovery bundle, not the full Rust source release.
The existing untracked migration remains in the worktree. `baseline-files.tar.gz`
contains the four existing before-files; `change.patch` adds the implementation,
tests, frozen oracle and normalization contract. `files-sha256.json` lists exact
before/after hashes. An outside-Git replay passed for all eight files. Existing
Python paragraph/helper bodies and the retained heading fallback body are
unchanged (the heading fallback is renamed only). No dependency or lockfile
change, original-test edit, model request, downstream edit or production IO.

## Verified evidence

Remote: `baypier-prod:/root/llmbase-rust-migration-20260920/b14-heads`.
Finite tmux `llmbase-b14-heads`, command `bash primary/launch.sh`; launch runs
`timeout --kill-after=15s 1500s bash primary/verify.sh`. Final runner exit0.
The session and its actual pane PID557647 are absent. The failed local K3
session/PID74284 are also terminal; no task process remains.

- All223 source inputs match before/after the run and the current worktree.
- `cargo fmt --all -- --check` and core/Python all-target Clippy pass.
- Native heading tests5 and unchanged paragraph tests2 pass. The native rules
  execute all52 shared-dialect frozen cases without Python callbacks; unsupported
  regex syntax, invalid DTOs, size limits and eager validation are checked.
- A fresh wheel/venv passes79 focused and618 full Python tests. All60 frozen
  original outputs (including real Taixu rules and four upstream fixtures) pass
  through the actual Rust scanner/Python matcher, with the Python algorithm and
  helper bodies poisoned while identity anchors remain unchanged.
- Independent547 actual-old comparisons and9 error-class/trace checks pass.
  Separate callback evidence proves two eligible headings, four ordered matcher
  calls, no calls inside skipped blocks, exact error identity/no retry, nested
  trace cleanup and real parser override/restoration.
- All48 installed Python files match the snapshot. Wheel SHA256:
  `992ad596831ccecc7d3515e19ae4cee031ca87fe01299571cdf6a06cd2467067`.
  Extension SHA256:
  `7e557279d61616b589589630690acb0dcc28d7d12de5c310a475af6d6bbee373`.
- TypeScript noEmit and Python lint import pass. The primary reviews the staged
  bundle under the user's explicit authorization for primary-agent review.

`evidence/` preserves the exact runner/verifiers, input manifests, phase logs,
candidate provenance and independent receipts. On an isolated host with the
recorded toolchain and full matching source snapshot, run `bash primary/launch.sh`;
the evidence scripts retain their original isolated task paths. Do not run them
against production data.

## Remaining release gates

This accepts heading normalization only. Configuration-root repair, remaining
maintenance/catalog facades, worker/connector parity, final-candidate Siwen/Taixu
integration, final workspace/transport/frontend checks, full source commit and
release are still required. The B8 standalone binary predates this utility
change; its historical acceptance is not a final assembled release proof.

The native regex dialect is explicitly narrower than Python regex. Trusted
Python patterns keep their original complexity behavior. The body-size bound
also applies to a no-rule call. Site-specific title promotion/deduplication
remains downstream policy.
