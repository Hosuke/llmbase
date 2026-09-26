# Primary review of initial B9 harness (pending final implementation)

Concrete issues observed in the initial 494-line script, not assumptions about
the engine. Recheck against K3's final diff before requesting repairs: it is
still active and may already correct them while running its finite proof.

1. HTTP exit evidence is discarded: http_transport returns before its finally
   assigns exit_code, and transport_proof explicitly sets receipt.http.exit to
   None using an `if False` expression. Return only after cleanup, record the
   actual exit and cleanup escalation, join stderr drainer, close its log and
   pipe on every path. No fabricated/null successful exit evidence.
2. MCP closes stdin then immediately calls stop_process(SIGINT), which signals
   before waiting for normal EOF termination. Wait for a bounded clean exit
   after stdin close first, use signal/terminate/kill only after timeout, and
   distinguish clean exit from forced cleanup in the receipt. Remove dead
   `signal.SIGPIPE if False` branch. Current original transport contract expects
   real EOF exit0, not a signal race.
3. Provider DELTA_GATE.wait(30) ignores its false/timeout result. SSE keepalives
   can keep HTTP reads alive, so a buffered delta arriving after provider timeout
   could be accepted as proof of live forwarding. Gate timeout must make the
   proof fail. Record gate-entered/expired/finished state, assert first forwarded
   delta is observed while the gate is held, then release. Set entered before
   flushing the first delta to avoid a controller race. Always release finally.
4. Require actual evidence and done SSE frames (and usage if reported), no
   error event; retain their JSON payloads for audit, plus matching request ID,
   version1 and strictly increasing sequence. Existing code checks only delta
   and done presence, and receipt drops evidence contents.
5. HTTP call/MCP exchange receipts record only names/status/IDs/timing. Preserve
   actual request JSON and response JSON as required, without bearer/cookie
   headers. The fixtures are synthetic and bounded. Compare complete core
   Evidence fields incl library_id, revision, passage_id and offset_unit as well
   as bytes/hash, not only a subset of the identity.
6. Retain original31 CLI assertions. Final proof must run the same accepted
   f1b2bdd... release CLI in the no-Python chroot, then frozen lexical eval.

No engine changes are requested. These are harness/provenance corrections. A
green script exit alone is insufficient until its assertions prove the claims.

Recheck during implementation: K3 has now fixed normal MCP EOF wait and the
HTTP result dict's real exit assignment. Do not redundantly demand these fixes
again if retained. HTTP stderr cleanup/join and gate/receipt assertions remain.
Also give the SSE loop an absolute deadline: per-read socket timeout alone can
be extended indefinitely by periodic keepalive frames. Include failure state
and real process cleanup on any incomplete stream.

Final acceptance: K3 ended with exit0; primary retained its implementation and
corrected the remaining gate/deadline/evidence/cleanup issues directly. Final
remote proof and the expired-gate negative check both exit0, with clean native
server exits, actual JSON receipts and matching harness hashes. The full
original31 CLI chain passes and all20 original main assertions remain. Same
accepted binary/fixture/evaluator lexical evidence is unchanged. Exact two-file
replay, TypeScript, Python import and staged whitespace checks pass. Review of
the staged patch and its receipts: LGTM; no HIGH finding remains. No engine
change, new release, downstream/production or full-goal claim is made.
