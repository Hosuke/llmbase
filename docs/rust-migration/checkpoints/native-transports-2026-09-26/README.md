# Native release transport and lifecycle proof

Accepted bounded harness/runbook checkpoint after d937550. Primary specified
the transport commands, schemas and acceptance; K3 implemented the harness and
ran the initial proof. Primary reviewed it, strengthened the stream/receipt/
cleanup assertions, and independently reran the final harness on baypier-prod.
No engine source was changed. All 119 native inputs still match accepted B8.

Pinned actual release CLI: SHA256
f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8,
version llmbase0.1.0. The same artifact runs the CLI, HTTP and MCP inside a Linux
chroot containing the ELF, shared libraries and synthetic library data only.
The Python test controller and deterministic model fixture run outside chroot;
they are not part of the native runtime. No real model/credential/corpus is used.

Primary final proof, remote b9-primary/accepted, exit0:

- All original 31 CLI steps pass: source write, draft/apply, indexing, persisted
  finite job, search/evidence, vector cache/hybrid engineering, streaming/ask,
  save/promote, source update/delete, export and backup/restore/rollback checks.
  All 20 original main assertions are retained unchanged (AST comparison).
- Actual HTTP `serve` starts on an OS-assigned loopback port. Authenticated
  search returns the same complete Evidence object as CLI; a new raw source
  is written, searched, resolved and deleted. Visitor-cookie write and owner
  access to another configured library both return403; denied write has no file.
- Actual HTTP SSE delivers its first delta while the provider is held before
  completion. Only then does the controller release it. Evidence/usage/done,
  version, request ID and increasing sequences are checked and recorded.
- Actual MCP stdio initializes protocol2025-03-26, discovers schema, rebuilds
  metadata, searches original CJK text, resolves its exact UTF-8 byte evidence,
  and returns a protocol error for an unknown tool. EOF exits0; stdout is JSON.
- HTTP clean SIGINT exit0 and MCP clean EOF exit0 need no forced cleanup.
  Receipts retain actual request/response JSON, exits and shutdown actions;
  authorization headers are not written. Runtime file inventory has no Python
  executable/package. Harness hashes match before/after and local source.

Negative proof, reject-expired-gate.py, exit0: intercepting a forwarded delta
until the provider's gate expires triggers the intended assertion. There is no
successful lifecycle receipt, and the HTTP server still exits cleanly0. This
closes the first harness's unasserted gate-timeout possibility; the first-pass
report alone is not the final stream proof. Primary also added an absolute
stream deadline, complete JSON evidence, and joined/closed stderr resources.

Same-binary frozen lexical evaluation (first-pass-evidence), exit0: raw and
compiled each have30 held-out and6 tuning queries, Recall@5=1.0, MRR@10=1.0,
no required/comparison failures, 60 unique citations resolved. Five search
repeats and20 performance repeats use unchanged fixtures and legacy baseline.
Held-out warm p50/p95: raw5.23/6.65ms, compiled5.37/6.45ms. These include fresh
CLI launch, not Rust-only execution; OS cache was not evicted. Real semantic
quality and real provider latency remain unverified, separately from these
lexical/engineering checks. No evaluator rerun was needed after harness-only
review changes: binary, evaluator, query set and baseline are unchanged.

Local syntax, TypeScript and Python lint import pass. Primary review: LGTM,
no HIGH issue remaining; exact two-file replay passes. The user authorizes
primary review for bounded tasks. All B9 K3/remote/primary runners have ended.

Still separate: copy-first and actual old-Python-reader rollback proof, fresh
wheel/downstream acceptance, remaining required facade parity and release
publication. This checkpoint is not full goal completion or production approval.

Replay: extract baseline-files.tar.gz outside an existing Git worktree, verify
before hashes, git apply --check/apply change.patch, then compare after hashes
to files-sha256.json and the working implementation. Logs/scripts/receipts here
permit auditing the accepted assertions and actual command exits.
