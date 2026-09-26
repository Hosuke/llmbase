# B9: final-artifact interpreter-free transport proof

DISPATCHED after B8 acceptance/checkpoint d937550. All native gates are green.
Verification/harness task, not new engine features. Implement now.

Gate 1 already has a 31-step CLI Linux chroot proof on the earlier B6 binary,
but no corresponding HTTP/MCP proof inside that interpreter-free environment.
Reconcile all evidence to the new accepted source manifest and release binary.

Primary has now checked the transport code and frozen contract tests. Use the
actual release binary directly; in-process router tests do not establish that
the release executable's HTTP startup works. Do not spend time investigating
an alternative in-process/compiled-test-executable proof.

Concrete implementation, extending the existing verify-standalone.py:
1. After the first lexical search/evidence resolution and before source update/
   deletion, invoke a new transport-proof helper using the same chroot prefix,
   binary, base `/library`, library ID `standalone`, environment and fixture.
2. HTTP: start prefix + global base/id + `--library other=/other serve --host
   127.0.0.1 --port 0 --public-read`. Create runtime/other as synthetic empty
   library. Add a synthetic LLMBASE_API_SECRET only to child env (no real key).
   Parse the assigned port from stderr `llmbase HTTP listening on 127.0.0.1:N`;
   do not guess or reserve/release a shared-host port. Read with a finite timeout.
3. HTTP uses POST /api/v1/op with JSON {library_id,operation,args}; envelope is
   {schema_version:1,request_id,result}. Check owner search/evidence against the
   existing CLI evidence. With owner bearer, source_write a temporary raw
   `transport-probe.md` containing a unique searchable token; search it, resolve
   its returned evidence, then source_delete by evidence source_id/content_hash.
   Verify the probe no longer searches. No extra compiled article is created,
   so the existing lifecycle's article_count assertions stay intact.
4. Reject a visitor-cookie-only source write with 403 and check no target file
   appeared. Reject owner token targeting the configured `other` library with
   403; unknown-library 404 is not a scope proof. Public-read is default-library
   only. Keep bearer values out of output receipts.
5. Add one real HTTP SSE request to /api/v1/ask/stream with question, library_id,
   request_id and short_answer. Reuse the existing provider; optionally add a
   one-shot threading.Event gate after its first delta for this probe only.
   The controller must receive a `delta` before releasing that gate, proving
   forwarding before provider completion. Collect evidence/done, version=1,
   matching request_id and monotonic sequences. Always release gate in finally.
   Existing native ask_sse tests already cover cancel/timeout/ownership; do not
   build a second cancellation framework in this bounded task.
6. MCP: start same chroot prefix + global base/id + `mcp`, stdin/stdout pipes,
   stderr separate. Follow existing tests/transports.rs: initialize protocol
   2025-03-26, notification initialized, tools/list schema check, tools/call
   kb_rebuild_index, kb_search_raw(query="學而時習之"), resolve_evidence with
   returned evidence, and unknown_tool protocol error. Check IDs, valid JSON-only
   stdout, expected library/original quote and clean EOF exit. Additive evidence
   tool name is exactly resolve_evidence (mcp.rs:39,155).
7. Use selectors/bounded I/O or an equivalent simple finite controller. Existing
   Python controller is outside the chroot. Shut down HTTP with SIGINT and wait,
   fallback terminate/kill on timeout; close MCP stdin and wait, likewise clean
   up on any assertion failure. No daemon survives the proof.
8. Preserve original 31 lifecycle assertions. Add a transports receipt with
   real commands/statuses/results, stderr logs and final process exits. Verify
   the same pinned binary hash and interpreter-free runtime inventory.

Source references already inspected by primary: main.rs ServeArgs and command
Serve; http.rs grants_from_environment/native_op/serve; tests/transports.rs
stdio_initialization... and http_scope...; tests/ask_sse.rs actual_delta_reaches...
These provide exact existing schemas; no API/framework design work is delegated.

Ownership: scripts/rust-migration/verify-standalone.py, one narrowly scoped new
transport verification script if needed, docs/rust-migration/runbook.md and this
private task directory. Do not edit runtime implementations or weaken existing
tests. Report discovered runtime defects for a separately defined repair.

Required proof:
1. Use the actual freshly built release CLI and record its hash, source manifest,
   runtime file inventory and ldd. Runtime contains ELF/shared libraries and
   synthetic data only, with no Python executable or llmwiki package. Test
   controller and deterministic fixtures may run outside; state that precisely.
2. Reuse existing HTTP/MCP schemas and the 31-step CLI harness as specified
   above. Exercise the actual pinned release binary, not a stale/debug CLI or
   an in-process router presented as executable startup proof.
3. HTTP exercises real authenticated reads, a mutation and resulting retrieval,
   evidence/source resolution, rejected unauthenticated or wrong-scope action,
   and request isolation. MCP uses real stdio initialization, discovery, a tool
   call and protocol error; stdout remains protocol-only. Use supported routes
   and schemas already frozen by the native tests. No actual provider requests.
4. Prefer existing complete targets if running them is cheap; include generated
   stream/cancel fixtures where already available. A health 200 or advertised
   tool list alone is insufficient. All child servers terminate in finally/
   trap even on failure; finite remote tmux only and loopback listeners.
5. Fresh same-binary CLI lifecycle and frozen retrieval comparison. Do not
   alter fixture/query/baseline for a pass; no real semantic quality claim.
6. Record exact commands/exits, request/results, binary and fixture hashes,
   interpreter-free boundary and remaining gate coverage. Current final-wheel,
   downstream, rollback old-reader and parity proofs are separate gates.

All builds, servers, test execution remote on baypier-prod below the isolated
migration root, two cargo jobs and nice -n10. Local source edits, short syntax
check and orchestration only. No production, secrets, real documents, local
watchers/tmux, subagents, stage/commit, publication or next task. Save finite
runner identity and logs; no restart solely for an observation timeout.

Exact baseline: checkpoint d937550, source manifest (119 files) copied into this
task directory as accepted-source.sha256. Remote immutable B8 source is
/root/llmbase-rust-migration-20260920/b8-web-read-config/source.
Actual release CLI is ../llmbase-cli-bin under that B8 task root, SHA256
f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8.
Do not rebuild it; all engine tests, fmt/clippy and release build already pass.
Copy that pinned artifact to your own remote task root and verify its hash.

Local before/ preserves your two existing owned files. inputs/ holds current
standalone/evaluation scripts, frozen retrieval fixture and legacy result. Use
these exact inputs; do not replace queries or baseline. Remote task root:
/root/llmbase-rust-migration-20260920/b9-native-transports
Remote tmux: llmbase-b9-native-transports. External fixture interpreter:
/root/llmbase-rust-migration-20260920/b5-registration-fix/candidate/bin/python

After creating the parent remote root, rsync inputs/ to remote inputs/ with the
same directory shape; overlay only your modified harness there. Check hashes
and existence before launching the finite script (B8 had an avoidable wrong
rsync destination; do not repeat it). Script should fail on copy/hash failure,
retain failed runs under separate names, and save all actual step exits.
Run modified verify-standalone.py --binary <pinned copy> --run-root <fresh>
--chroot. Then from inputs/, run scripts/rust-migration/evaluate_retrieval.py
--backend native --native-bin <same binary> --artifact-label b8-final-native
--compare docs/rust-migration/results/retrieval-legacy-v1.json --repeats 5
--performance-repeats 20 --output <new result.json>. This is fixed lexical
quality and engineering evidence, not a real semantic quality claim.

Timebox 20 minutes, hard 25. Save REPORT.md with actual process state before
deadline; if remote finite proof continues, give its tmux/log/exit path and stop.
No local background polling process: primary will collect after your report.
Do not end with a shell background task and claim you will be called back.
Primary owns the sole local agent wrapper. Stop after REPORT.md for independent
review/acceptance. No stage/commit, next task or runtime source modifications.
