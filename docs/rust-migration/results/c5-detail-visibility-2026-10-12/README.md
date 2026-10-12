# C5 — bounded detail visibility, 2026-10-12

Fixes [#24](https://github.com/Hosuke/llmbase/issues/24): the Flask gate invoked
`compat_kb_visibility` before every public article, sections and raw-source detail
request. Reading every document through these routes therefore repeatedly scanned
the entire library. This was an HTTP adapter defect; the Rust source is unchanged.

The three original built-in views now check the current target document directly.
The same parsed post supplies both the privacy decision and response. Alias and
backlink targets receive the same path and privacy checks; symlinks below the
configured root, unsafe paths and malformed metadata fail closed. Python's
`private` flag interpretation now matches Rust (only absent/null/false is public).
No cross-request visibility cache is introduced. A downstream replacement view
does not inherit the exemption merely by retaining a built-in endpoint name.
Aggregate reads, opaque views and mutations retain their existing snapshot gate.

## Measured result

Both fresh wheels use the same Rust extension SHA-256. The baseline is commit
`90394d020a2862f1c57599558332c059abd42232`; the candidate is pinned by all 252
source/build/test input hashes in `primary/candidate-inputs.json`. Each wheel was
installed in its own virtual environment outside the checkout. See
[installed provenance](provenance.json) and [comparison](comparison.json).

On the same synthetic corpus and remote host, three-request median latency for
`/api/articles/p0` was:

| Documents | Baseline | Fixed | Observed speedup |
| --- | ---: | ---: | ---: |
| 100 | 2.594 ms | 0.981 ms | 2.64x |
| 1,000 | 15.083 ms | 0.831 ms | 18.14x |
| 10,000 | 130.837 ms | 0.843 ms | 155.13x |

At 10,000 documents the sections route fell from 130.170 ms to 0.848 ms.
Enumerating all article and sections routes for 100 documents (200 requests)
took 0.499 s before and 0.153 s after; for 1,000 documents (2,000 requests),
28.856 s before and 1.547 s after. This is a synthetic route enumeration, not
a real Siwen/Taixu full-site bake or a comparison against the old Python release.

All response bytes match between wheels, and all 11,100 Markdown fixture files
retain their before/after hash. The baseline records one native snapshot dispatch
per request; the fixed routes record zero. Tests additionally replace the snapshot
function with a hard failure to enforce that these paths cannot scan the library.
Timings are observations rather than a flaky CI timing threshold. The benchmark
loads neither Siwen nor a profiler and uses no production corpus or paid provider.

## Acceptance and execution

- Fresh installed candidate: **59 focused tests, 693 full Python tests and
  18 subtests pass**. The focused suite includes 26 new cases for no-scan detail
  reads, live privacy changes with identical size/mtime, insertion/deletion,
  malformed flags, aliases, symlinks, backlinks, replaced views, private-token
  access, cross-app isolation and preserved aggregate/mutation denials.
- Source hashes match before and after execution and match the local candidate.
  Both wheels contain the identical native extension. Rust code is unchanged;
  C4's native acceptance and successful full CI apply to those unchanged inputs:
  https://github.com/Hosuke/llmbase/actions/runs/38117225055.
- Local TypeScript noEmit, Python lint import and whitespace checks pass.
  Independent staged source/test/proof review returned LGTM with no HIGH/MEDIUM
  findings; it independently checked the 250 baseline/252 candidate hashes,
  response/source equality, final markers and the limits of the measured claims.

Remote isolation directory:
`baypier-prod:/root/llmbase-rust-migration-20260920/c5-detail-visibility-20261012`.
Finite tmux session: `llmbase-c5-final`; runner: `bash primary/remote-run.sh`.
Steps use a 600-second timeout, lowered priority and at most two build jobs.
Step logs are `<step>.log`; `validation.json` saves every final exit code.

The initial setup lacked `ensurepip`; virtual environments now use `--without-pip`
and the toolchain pip's `--python` option. The first test-copy attempt lacked its
parent directory. After adding that directory, the completed wheel builds were
retained and acceptance resumed with `bash primary/remote-run.sh --resume-tests`.
Earlier setup logs remain under remote `setup-attempt/`. Final 22 step/runner
markers are all zero, and the task tmux has ended. No local long build/server ran.

## Remaining boundaries

The article route still reads `backlinks.json` and the actual backlink targets;
this change does not optimize that metadata index or aggregate requests. A real
site's bake must be measured on its copied corpus and final downstream code.
Siwen's separate custom classic-route privacy fix remains tracked by #19 and its
site owner; this upstream repair does not certify those custom routes or publish
anything. No version bump, package release or production switch is performed.
Save this bounded fix and stop; the broader Goal remains paused.
