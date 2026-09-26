# B15 accepted by primary — 2026-09-27

Canonicalized the base in config._defaults before constructing effective paths.
Native scope/writer/journal code remains byte-identical to accepted B14.

Baseline installed B14: new tests5 failed/5 passed. Fresh B15: focused40/full
Python628 passed; independent actual-old/candidate relative/absolute write
matrix has four exact-byte successes. Regression tests inspect exact applied
journal before/after bytes and retain out-of-scope/reserved-file refusal.

225-source/48-installed-file hashes pass.222 previous inputs are unchanged and
the new extension hash equals B14, validating reuse of prior native checks.
Exact3-file replay outside Git passes. TypeScript/import checks pass.
Primary review corrected an optional-extension import in the new test, verified
10 clean skips on the unbuilt local checkout, then repeated the complete remote
runner:40 focused/628 full tests pass. Runtime sources were unchanged; the first
acceptance and red baseline test are retained. Final wheel SHA256:
b79f436119c8047ff0994705836ef989113ae9f009124764cc5c7b0bc66e2b76.

Finite baypier-prod session llmbase-b15-config-roots ran bash primary/launch.sh;
runner.exit0 and final session absent; first-run actual PID573683 absent. No background task
remains. Complete evidence is collected/ and the checkpoint bundle is
docs/rust-migration/checkpoints/canonical-config-roots-2026-09-27/.
No full native config migration or release completion is claimed.
