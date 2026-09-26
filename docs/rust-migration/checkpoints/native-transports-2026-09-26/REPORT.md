# B9 REPORT — final-artifact interpreter-free transport proof

Status: **COMPLETE, all green.** Dispatched after B8 acceptance (checkpoint d937550).
Finished well inside the 20-minute timebox; no remote work left running.

## What was done

Extended the owned harness `scripts/rust-migration/verify-standalone.py` (additive
only; the original 31-step CLI lifecycle and all 20 original asserts are untouched)
with a transport-proof stage invoked after the first CLI lexical search/evidence
resolution and before source update/deletion, using the same chroot prefix, pinned
binary, base `/library`, library ID `standalone`, environment and provider fixture.
Short note added to owned `docs/rust-migration/runbook.md`. New harness sha256:
`22a8d5de73ad73ef054b2be611b83c3d94497ad71ae09582357a226ff9f50696` (local file and
remote overlay verified identical). `python3 -m py_compile` passed locally.

## Pinned artifact and interpreter-free boundary

- Binary: B8 release CLI copied to `/root/llmbase-rust-migration-20260920/b9-native-transports/llmbase-cli-bin`,
  sha256 `f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8` — verified
  before launch and recorded in the receipt. Not rebuilt.
- Source manifest: `accepted-source.sha256` (119 files, checkpoint d937550) present
  in local and remote task roots.
- Remote inputs rsynced to `b9-native-transports/inputs/` with the same directory
  shape; `sha256sum -c inputs.sha256` passed for every pristine input (the overlaid
  harness is the only intentional difference, verified separately above).
- chroot runtime inventory (receipt `runtime_files`): only `bin/llmbase` (ELF),
  `lib/x86_64-linux-gnu/{libc.so.6,libgcc_s.so.1,libm.so.6}`,
  `lib64/ld-linux-x86-64.so.2`, and synthetic library/snapshot/restored data.
  No Python executable or llmwiki package. `ldd.txt` shows libc/libgcc/libm/ld-linux
  only. The Python 3.12.3 test controller
  (`.../b5-registration-fix/candidate/bin/python`) and the deterministic HTTP
  provider fixture ran **outside** the chroot, on the remote runner only.

## verify-standalone --chroot (exit 0)

Command: `nice -n10 $PY inputs/scripts/rust-migration/verify-standalone.py --binary
$ROOT/llmbase-cli-bin --run-root $ROOT/run-20260926-155510 --chroot`
(remote tmux `llmbase-b9-native-transports`, since killed; logs/exit files saved).

- CLI lifecycle: `"passed": true, "steps": 31` — identical to the B6-era proof, now
  against the accepted B8 binary/manifest.
- HTTP leg (`serve --host 127.0.0.1 --port 0 --public-read`, second configured
  library `other=/other` as synthetic empty library; synthetic `LLMBASE_API_SECRET`
  in child env only, value redacted from all receipts):
  - Port 37011 parsed from stderr banner `llmbase HTTP listening on 127.0.0.1:37011`
    (finite read; no port guessing). stderr log contains only that banner; stdout empty.
  - Owner bearer `POST /api/v1/op search` 200 — envelope `{schema_version:1, request_id,
    result}`; evidence identical to the CLI evidence on source_id/start/end/
    exact_quote/content_hash. `evidence` resolve 200, `stale:false`.
  - Visitor-cookie-only `source_write` → **403**, and no `raw/transport-probe.md`
    file appeared.
  - Owner bearer targeting the configured `other` library → **403** (scope proof;
    not an unknown-library 404).
  - Owner probe lifecycle: `source_write` 200 → `index` 200 → search unique token
    200 → resolve 200 (`stale:false`) → `source_delete` by source_id/content_hash
    200 → re-search 200 with zero results. Raw-only probe; no compiled article
    added, so the lifecycle `article_count == 2` assertion still held.
  - SSE `POST /api/v1/ask/stream` (question/library_id/request_id/short_answer,
    top_k 1) 200: fixture provider gated after its first delta; controller
    received `delta` (seq 3) **before** the gate was released — forwarding before
    provider completion is proven. Events: progress(0), evidence(1), progress(2),
    delta(3), delta(4), usage(5), done(6); all `version:1`, matching request_id,
    strictly monotonic sequences, `done` last. Gate always released in `finally`.
  - Shutdown: SIGINT → clean exit 0 (recorded). No fallback needed.
- MCP leg (`mcp`, stdio pipes, stderr to separate log — 0 bytes):
  - initialize (protocol 2025-03-26) id 1 → `serverInfo.name == "llmbase"`, tools
    capability; notification `initialized`; tools/list id 2 → `kb_search` schema
    `required:["query"]`, `resolve_evidence` present.
  - tools/call ids 3–6: `kb_rebuild_index` (article_count 1, not isError),
    `kb_search_raw(query="學而時習之")` — evidence `library_id:"standalone"` and
    `original[start:end] == exact_quote` bytes; `resolve_evidence` → `stale:false`;
    `unknown_tool` → JSON-RPC `error` object. All response ids matched.
  - Stdin close → clean EOF exit 0; stdout drained to protocol-only JSON (no
    banners/logs). Recorded exit 0.
- No stray `llmbase` processes after the run (0 found); both child servers
  terminated inside the proof.

## Frozen retrieval comparison (exit 0)

From remote `inputs/`: `evaluate_retrieval.py --backend native --native-bin
<same pinned binary> --artifact-label b8-final-native --compare
docs/rust-migration/results/retrieval-legacy-v1.json --repeats 5
--performance-repeats 20 --output retrieval-native-b9-20260926-155510.json`.
Fixture/queries/baseline unchanged. Result: raw and compiled, held_out (30) and
tuning (6): recall@5 = 1.0, MRR@10 = 1.0, `required_failures: []`,
`comparison_failures: []`; warm p50 ≈ 5.2–5.5 ms. Fixed lexical/engineering
evidence only — no semantic-quality claim.

## Evidence paths

- Local: `.rust-migration-runs/b9-native-transports/evidence/` (receipt.json,
  ldd.txt, http/mcp stderr logs, verify/eval logs + .exit files, retrieval JSON).
- Remote (retained): `/root/llmbase-rust-migration-20260920/b9-native-transports/`
  with `run-20260926-155510/`, `verify-20260926-155510.{log,exit}`,
  `eval-20260926-155510.{log,exit}`, `retrieval-native-b9-20260926-155510.json`.
  Remote tmux session killed after collection; nothing left running.

## Notes

- Runtime defects discovered: none. No runtime implementations were modified;
  no existing tests weakened; no stage/commit performed.
- Boundary statement: builds/servers/tests ran remote on baypier-prod under the
  isolated migration root with `nice -n10`; local work was source edits, syntax
  check and orchestration only. No production, secrets, real documents, local
  watchers/tmux, subagents, or publication.
- Remaining gates (separate, not claimed here): final-wheel, downstream,
  rollback old-reader and parity proofs.
