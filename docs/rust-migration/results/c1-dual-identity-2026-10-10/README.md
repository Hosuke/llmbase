# C1 — observed dual-library identity, accepted 2026-10-10

This is one completed slice of issue #10. It repairs the verifier, not the
engine. The old proof declared named sites but accepted `default` native IDs;
the updated proof scopes sequential operations, checks actual doctor/journal
identities and preserves environment state on success and exceptions. Both
libraries now receive checked, independent journal writes. Source and accepted
document hashes are checked across the complete proof.

## Evidence

- Six focused unittest cases pass: default/wrong/missing observed IDs cannot be
  certified, and absent/present/nested environment state is restored after errors.
- `candidate-identity.json`: all 166 source/build inputs and 48 installed Python
  files match the accepted B16 runtime. Wheel and native-extension hashes match.
  Engine source is `951dfb1`; no new Rust build was needed or claimed.
- `identity-acceptance.json`: independent installed-candidate exercise writes
  one applied journal row under each real ID, preserves synthetic raw bytes,
  and receives PermissionError when the other ID tries to open that database.
  This tests database identity binding, not remote caller authentication.
- `proof-receipt.json`: actual copied Siwen code is used for both site profiles.
  Each passes 24 required Flask requests, two alternating callback-backed asks,
  and BakeRun output of 159 JSON / 135 HTML files. Cross-site content/origin
  sentinels are excluded; source/accepted document hashes stay unchanged.
- Runner exit is 0 and its finite tmux session is absent. The copied 60-file
  downstream source manifest matches before and after the remote run.
- TypeScript noEmit, Python lint import and staged whitespace checks pass.
  Primary review covers the script/tests and receipt claims; no full-suite or
  current-frontend acceptance is inferred from these focused checks.

## Frozen source and remaining limits

`downstream-before.json` records HEAD `cb7ccbb` plus hashes of the 60 copied
working files, including pre-existing uncommitted changes. The isolated copy
stayed unchanged. The original checkout changed during the run: later hashes
of `siwen/prerender.py` and `siwen/web.py` differ, and its Git status changed.
These observations are retained, not erased, in `downstream-preservation.json`.
This task only read/copied that checkout; do not claim the entire original tree
was unchanged or that subsequent concurrent changes have been accepted.

The two sites share application code and use distinct instances/roots. Selection
via environment is sequential test scaffolding; Python globals are not isolated
concurrently. Both site profiles use wenyan; an English site-profile comparison,
end-to-end evidence contract, native compiler/hooks/jobs, full private-capability
denial and current frontend build remain open. The internal-type raw fixture
checks content filtering, not all privacy behavior. The bake used historical B5
static assets copied within the remote runner. All model calls in this proof
are explicit Python callbacks; no real provider or semantic-quality claim.

See `matrix.json` for the machine-readable scope. Issue #10 stays open; this
acceptance is neither final dual-site adoption nor a release gate completion.

## Commands and saved execution

Local focused check:

```sh
/Users/lanchair/Developer/LLM-infra/Github/llmbase/.venv/bin/python -m unittest discover -s tests/rust_migration -p test_dual_site_proof.py -v
```

Remote task directory:
`baypier-prod:/root/llmbase-rust-migration-20260920/c1-dual-identity-20261010`.
The finite session was `llmbase-c1-dual-identity`, command
`bash primary/remote-run.sh`; logs are `runner.log` / `runner.exit`, with a
180-second bound. `primary/` here preserves the actual acceptance scripts.
The independent command was the B16 candidate-v2 interpreter running
`primary/accept_identity.py`; the full diagnostic used `primary/run_probe.py`.

To repeat, use a fresh sibling task directory with `primary/`, the saved source
manifests in `primary/`, a manifest-matching `siwen-snapshot/`, and the current
proof script copied into `primary/`. Use the verified installed interpreter;
do not overwrite the historical proof directory or blindly rerun the archived
runner's fixed path. The standalone verifier accepts a fresh `--run-root`.

## Delegation and stop

Kimi K3 was tried with a bounded process. It correctly identified an incorrect
trace-field assumption in the first task definition; the corrected task uses
doctor output and actual journal rows. It produced no patch before primary
stopped the repeated read-only investigation and completed this small change.
Both started model processes ended with exit 143; no local task tmux remains.
No new agent or build is left running. Do not retry those sessions automatically.

Stop after committing/pushing this slice. The user requested one block at a
time; the broader Goal scheduler remains paused. The commit skips the long CI
workflow because engine bytes are unchanged and this verifier-only slice has
the focused acceptance above. Next work requires another user instruction.
