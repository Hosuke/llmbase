# Primary B11 supplementary acceptance (passed candidate)

Final candidate comparison passed, along with35 focused and590 full Python
tests. `collected/primary/candidate-comparison.json` records exact output,
unchanged fixture bytes, extension identity and the actual callback invocation.
The procedure below records the independently frozen acceptance requirements.

While K3 implements the defined facade, primary froze a real legacy result
matrix with `capture-baseline.py` using the unchanged original baseline package
at a7c4d9da764462e09c0769ecd8ba548a46d27e83. No Siwen or original package file was
changed. `baseline-identity.json` records all actual imported module hashes and
the clean tracked-package check; checks.py bytes also match B11's before file.

`baseline-library/` is an eight-file synthetic custom-root library exercising
all eight lint result categories: structural, broken links, orphans, missing
metadata, dirty tags, duplicates, stubs and uncategorized. Default CJK policy
reports17 issues, permitting CJK slugs reports16. The exact arrays and hashes
are frozen in `baseline-lint.json`; do not alter expectations to fit a candidate.
No native core was loaded during baseline capture and original bytes stayed
unchanged.

After K3's fresh wheel passes its own focused tests, copy this fixture and
`verify-candidate.py` to a separate remote primary directory. Run that script
with the NEW candidate interpreter, REQUIRE_NATIVE=1, from outside source.
Arguments: --library COPIED_LIBRARY --baseline baseline-lint.json
--output candidate-comparison.json --siwen-before b11-primary-before.json.
The last file is the prior primary repro record in the parent runs directory.

Require exact legacy results in both CJK modes, maintenance_run Rust traces,
candidate module origins and unchanged source bytes. The unchanged AST-extracted
real Siwen callback is then exercised once under profiling, with its expected
result and hook-independent issue count checked. This is a real callback proof,
not full Siwen integration. No candidate pass is claimed until it is run.
