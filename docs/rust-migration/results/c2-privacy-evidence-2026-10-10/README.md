# C2 — private data and evidence diagnostic, 2026-10-10

This bounded #10 slice is complete as a diagnostic. **Dual-site privacy is
not accepted.** The frozen Siwen application used for both Siwen and Taixu
leaks explicitly private classics through its custom APIs, HTML and BakeRun.
The verifier correctly exits **2** (`contract_failures`): 22 checks pass and
14 fail across two libraries. No engine or downstream runtime fix is claimed.

## What passed

- The installed Rust candidate resolves evidence to the exact original UTF-8
  bytes, including frontmatter offsets, CRLF, a non-BMP character and combining
  accent. Both named libraries reject foreign-library evidence and forged
  quotes. After a deliberate source update, the old revision remains exact and
  resolution reports `stale: true`, current revision 2.
- These installed-wheel evidence calls use **privileged local FFI**. They do
  not test public evidence transport/authentication. Separately, the existing
  Rust `library` and `web_read` suites pass **16 tests** on matching source,
  including public denial after an unindexed privacy edit and old-revision
  denial. See `native-tests.txt`.
- In the real copied application, visitor article/source lists omit private
  content and details return 404. Turning the previously public raw and
  compiled samples private without reindexing makes those details return 404.
- The 166 engine/build/test inputs, 48 installed Python files, candidate wheel
  and extension hashes match the B16/C1 provenance. All 60 frozen downstream
  files match before and after. Read/bake operations preserve document hashes;
  the two deliberate fixture changes per library are recorded separately.

## Observed downstream blocker

The same results occur for both named library instances:

| Visitor surface | Observation with `private: true` |
| --- | --- |
| `/api/classics` | 200, private title and slug included |
| `/api/classics/private-proof` | 200, private body included |
| `/api/classics/private-proof/sections` | 200, private heading included |
| `/classics/private-proof` | 200, private text included in HTML |
| Previously public classic API and HTML after revocation | Still 200 with revoked text |
| Focused BakeRun (3 JSON + 2 HTML requests per library) | Private content in three output files |

BakeRun writes private data to `api/classics.json`,
`api/classics/private-proof.json` and `classics/private-proof/index.html`.
Its `_verify_clean()` returns success despite these content leaks. Its
metadata/path scrubbing is not a document visibility policy.

The frozen `siwen/web.py::_scan_classics`, classic detail/sections handlers and
the prerender path read source files without the shared visibility boundary.
`llmwiki.web` deliberately scopes its built-in gate to built-in endpoints;
custom handlers have their own contract. Broadening that gate by endpoint
names would not make downstream raw-file readers or prerendering trustworthy.

This is a finding about the **C1 frozen dirty snapshot** (HEAD `cb7ccbb` plus
the recorded file hashes), not a claim about current production or subsequent
concurrent edits. No production libraries or deployments were accessed in this
block, and the original Siwen checkout was not modified.

## Next bounded implementation, saved for #19

Owner: downstream classic visibility adapter. Work in a separate Siwen
worktree; first reconcile the current source with this frozen reproduction.
Scope: catalog/list, classic detail/sections, prerender catalog/page and bake
selection for those same routes. Share public visibility decisions across
these paths, default to public-only, and define any private-capability access
explicitly. Cached catalogs, pagination/navigation and conditional responses
must not retain private entries or serve stale public content after revocation.

Acceptance: retain all public JSON/HTML controls; omit private titles/slugs
from lists/navigation; deny private details; revoke immediately without an
index rebuild, including a warmed cache; emit no private files/text during
bake; keep both library identities and source hashes intact. The C2 diagnostic
must exit 0 for the repaired subset. Add targeted conditional-cache and
`visibility: private` cases alongside the existing `private: true` fixtures.
Do not patch the original checkout or solve this by granting visitors private
capability. Other custom route families require their own follow-up checks.

## Verification, review and execution

- Local verifier tests: **16 pass** (10 new C2 negative controls plus 6 C1).
  Wrong errors/empty output cannot stand in for valid public/denial controls;
  private identifiers/artifact names are checked as well as content markers.
- Both Python entry points reject `-O`/disabled assertions. Bake acceptance
  requires its validator and each expected public JSON/HTML output. Evidence
  offsets require integer, nonnegative and in-bounds values before slicing.
- TypeScript noEmit, Python lint import and staged whitespace checks pass.
- An independent Codex reviewer found five verifier weaknesses. All five were
  fixed and the staged code was re-reviewed: LGTM, no remaining findings.
  Primary also checked receipt counts, failure claims, provenance and scope.

Remote task directory:
`baypier-prod:/root/llmbase-rust-migration-20260920/c2-privacy-evidence-20261010`.
Initial finite tmux session: `llmbase-c2-privacy`, command
`bash primary/remote-run.sh`; diagnostic timeout 180s, native tests timeout
300s. After review, `llmbase-c2-recheck` ran `bash primary/recheck.sh` against
a fresh `proof-v2` directory, preserving the first run. Only the diagnostic
was repeated. Final logs: `diagnostic-v2.log` / `.exit` (2),
`native-tests.log` / `.exit` (0). Both task sessions have ended.

`proof-receipt.json` and `candidate-identity.json` are copied from the final
run. Their script hashes match the committed scripts. To reproduce, copy
`primary/` and both proof scripts from `scripts/rust-migration/` into a fresh
runner directory, arrange the manifest-matching B16/C1 siblings, and invoke
the B16 candidate interpreter on `primary/run_probe.py`. Never overwrite the
archived proof directories. The reusable verifier itself accepts `--snapshot`
and a new `--run-root`.

No provider calls, server/listener or full release build was used. The Python
socket guard is a diagnostic tripwire, not an OS egress sandbox. The existing
wheel was reused; the selected Rust test binaries compiled in 9.66 seconds.
Historical B5 assets were reused, so current frontend/browser acceptance,
public evidence API, English-vs-wenyan profile comparison and all remaining
#10/#19 contracts stay open. This commit skips long CI for a verifier/docs-only
change with matching runtime hashes; it is not a release acceptance.

Stop after commit/push and issue updates. The broader Goal remains paused;
no next implementation or production switch is started automatically.
