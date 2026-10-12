# C6 — explicit Flask public read projections, 2026-10-12

Resolves the two R2 integration boundaries in
[#25](https://github.com/Hosuke/llmbase/issues/25). C5 correctly optimized original
bounded views, but Siwen's date wrapper lost that exemption. The default opaque
aggregate gate also ran before Siwen's safe public entities/statistics/export
pipeline. C6 adds an explicit host registration contract instead of inferring
trust from `__wrapped__` or weakening default gates.

See [public read projections v1](../../public-read-projections.md) for the API,
security responsibilities and R3 integration steps. Registrations bind the exact
view, all endpoint read-only rules and ordered before/after hooks in one app.
Changed bindings fail closed. A final check after mutable response hooks preserves
read authorization and fresh/no-store responses, including conditional requests.
No principal gets extra scopes; no Rust source or source-visibility policy changes.

## Installed engine and real downstream proof

- A fresh installed wheel passes **85 focused tests and 719 full Python tests,
  plus 18 subtests**. This includes 26 new registration/security regressions.
- All **49 installed Python files** match candidate source; the Rust extension
  hash is identical to the C5 baseline. All **656 engine/Siwen source and test
  inputs** match before and after the accepted remote runs.
- The downstream is committed Siwen `71984826661e8be90ce6bcaad9aa6cca19c98f04`,
  frozen twice. Baseline uses C5's installed `775c8b7` wheel. Candidate adds only
  the [saved registration patch](primary/siwen-public-read.patch) and C6 wheel.
  The original Siwen repository has concurrent uncommitted work and was untouched.
- Actual copied Siwen corpus: 17,880 files / 55,874,771 bytes, including one
  generated path-only config. No original config or credentials were copied.
  Every copied file retains the same before/after hash in both runs.

For three requests per route, on the same remote host and copied corpus:

| Route | C5 + original Siwen median | C6 + registered Siwen median | Snapshots/request |
| --- | ---: | ---: | --- |
| `/api/articles/ben-lai` | 640.771 ms | 21.948 ms | 1 → 0 |
| `/api/articles/ben-lai/sections` | 2.853 ms | 2.867 ms | 0 → 0 |

Every response is 200, with identical bytes: article 5,575 bytes, sections 567
bytes. Their hashes also match the R2 report's respective response hashes.
The probe uses code-local `sys.monitoring` only for `visibility_snapshot`, plus
native dispatch counters; no global profiler or replacement of the tested
function. See [comparison](comparison.json), [baseline](baseline.json) and
[candidate](candidate.json). Timings are observations; zero scans and unchanged
content establish the bounded fix. This is not a full-site bake measurement.

Separate fresh mixed fixtures run through the real frozen Siwen factory:

- Anonymous entities, statistics and public article export change from 401 to
  200, with positive public title/body/person controls and no private sentinel.
- Private article/raw details and Siwen's `public: false` records remain 404;
  alias/backlink filtering remains in the composed pipeline.
- Public article and raw files changed to private with the same size and mtime
  return 404 on the next conditional request. There is no cached public 304.
- All candidate probe routes record zero visibility snapshots/native dispatches.

## Downstream suite: accepted scope and remaining failures

The unchanged focused Siwen GET/privacy/export/index files pass **136 tests**.
The complete **committed** Siwen suite was also run without elevating anonymous
requests: **610 passed, 14 failed, 8 skipped** (632 collected). This is not the
user's reported 673-test working-tree run. No uncommitted tests were copied or
silently counted as accepted.

All 22 R2 mixed-library public GET failures are gone. The 14 remaining failures
are explicitly retained in [the full-suite ledger](siwen-full-summary.json):

- Ten QA tests still expect anonymous model/write success; the native adapter
  requires explicit narrow credentials. Their function assertions remain blocked
  by 401 and are not claimed to pass.
- Three mixed POST tests expect Siwen's 403 body but encounter the earlier native
  401. These are the same documented R2 authorization migration differences.
- One large-hub renderer fixture replaces the registered `api_articles` view
  after composition. C6 correctly refuses that changed binding, so the fixture's
  fabricated list is not returned. Compose fixture views before registration or
  seed actual public documents; do not disable the production binding check.

Four deployment-script tests require zsh/rsync and skip because this isolated
Linux environment lacks zsh; four live-corpus/catalog tests lack their optional
local inputs. Those eight skips remain unaccepted. R3 must account for these
conditions, adapt the auth fixtures, run both real full bakes and check private
sentinels and positive-content equality before considering a site switch.

## Execution, review and handoff

Remote task directory:
`baypier-prod:/root/llmbase-rust-migration-20260920/c6-public-projections-20261012`.
Finite tmux sessions: `llmbase-c6-build` (`bash primary/remote-build.sh`) and
`llmbase-c6-proof` (`bash primary/remote-proof.sh`). Every command has a 600-second
bound and lowered priority; compilation uses two jobs. Step logs are `<step>.log`.
Both sessions ended. The full-Siwen exit 1 is recorded separately from the 20
successful step/runner markers; runner completion does not convert it to success.

The first full-engine run lacked importable test helper modules in the isolated
working directory, causing two multiprocessing tests to time out with
`ModuleNotFoundError: tests`. Copying the same tests into the harness fixed that
setup issue. The unchanged wheel then passed the complete 719-test run in 14.04 s
via `bash primary/remote-build.sh --resume-tests`; initial logs remain under
remote `first-attempt/`. No engine change or timeout increase was used to pass.

Primary owned diagnosis, runtime design, integration and acceptance. Bounded
agents authored regression/probe code and independently reviewed the implementation.
Review required binding the entire hook pipeline and rechecking after response
hooks; both are implemented and tested. Final independent staged review verified
the engine/installed-source hashes, probe bytes/timings and failure ledger and
returned LGTM with no HIGH/MEDIUM findings. Local TypeScript noEmit, Python lint
import and whitespace checks pass. C5's full CI passed:
https://github.com/Hosuke/llmbase/actions/runs/38204433112.

The saved site patch covers both normal and no-frontend-assets factory returns.
Installing the wheel alone does not opt in the site: integrate that patch after
the final site wrappers/hooks, then run R3 on an immutable engine/site pair.
Neither the original site code nor production content was changed. No full bake,
deployment, version bump or package publication is claimed. Save this one block
and stop; the broader Goal remains paused.
