# B10 REPORT — copy-first migration and actual legacy reader proof

Status: **COMPLETE, all green.** Dispatched after accepted B9 checkpoint 944acae.
Final remote attempt `run-20260926-163031` on baypier-prod: verify exit **0**, 34
recorded child invocations, `passed: true`. Remote tmux
`llmbase-b10-migration-old-reader` killed after collection; nothing of this task
left running. No runtime defect found; no runtime code, engine, facade or test
changes; no commits.

## Owned changes

- `scripts/rust-migration/verify-migration.py` (new, owned). Final sha256
  `845547e5ac64edf906225b80ac8b44564884dcc9043fe2812259c3c414fd9cd8` (local ==
  remote, recorded in the run log). `py_compile` clean.
- `docs/rust-migration/runbook.md`: one paragraph in "Backup, restore and
  rollback" describing this proof. `before/` holds the pre-task copy.
- This run directory; collected evidence in `evidence/` (final summary.json,
  verify log, exit file).

## Pinned inputs (all hash-verified on the runner before execution)

- Release binary copied once from the B8 root to the isolated B10 root, sha256
  `f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8` — OK. Not rebuilt.
- `legacy-a7c4d9d.tar.gz` sha256 `fda78e50bcb93aa0992aeabe9cd45ae1e39b9600fa24b8814ea240e792e1e222`
  — OK; extracted into a fresh `legacy-source/`; all 46 archived file hashes
  verified against `legacy-source-sha256.json`, no extra files.
- Three frozen downstream fixtures verified via `inputs.sha256` — all OK.
- Accepted native119-source manifest `accepted-source.sha256` present in the root.
- External interpreter `…/b5-registration-fix/candidate/bin/python` (3.12.3,
  yaml/frontmatter confirmed) used only for the controller and the separate
  legacy reader. Nice -n10, finite tmux, per-child timeouts, finally cleanup.

## Proof results (final attempt)

1. Synthetic legacy library on custom paths (`originals/`, `knowledge/…`) with
   the bare-body Classical Chinese profile, built byte-exact on the frozen
   fixtures plus only synthetic additions (accepted output, aliases, locked
   manual taxonomy, backlinks, wiki asset, `.pipeline` history, `.chunk_cache`).
   Pre-invocation inventory: 10 files hashed.
2. Actual baseline reader (separate Python subprocess, legacy-source first in
   `sys.path`): 6 loaded `llmwiki` modules all under the pristine snapshot with
   manifest-matching hashes, `llmwiki._core` absent. Real
   `config.load_config`/`export.export_article`/`search.search_raw` baseline:
   article `xue` (keys: backlinks/content/created/outgoing_links/
   related_by_tags/slug/sources/summary/tags/title/updated), alias `時習`→`xue`
   resolved, raw search for 學而時習之 = 1 hit. **This stage legitimately uses
   Python and is not interpreter-free runtime evidence.**
3. `migration dry-run`: original inventory byte-identical afterwards, no native
   DB (`native_database` false, no DB files), unknown metadata reported
   (`canon`, `chapter`, `editorial_extension`, `review_state`), every scanned
   size/hash matches the custom-root source file, parse/identity errors empty.
   `migration copy` + `verify`: receipt complete, `source` == canonical
   original path, source_manifest == dry-run, every mapped target file
   byte-exact (custom-root mapping), original config preserved as
   `migration-original-config.yaml`, copy indexed (`wiki/_meta/llmbase.sqlite3`),
   original untouched. First copy then kept IMMUTABLE.
4. `migration restore` into working, verified before writes. Scratch raw
   source written, finite index job enqueued/run (`job_id
   534c760b-489f-430a-afc5-bfc9b21f11f0`, succeeded, attempts 1), searched,
   evidence obtained (`source_id 0d9b2ae1-2613-4d30-bdca-7bb4c423fed2`),
   updated with expected hash (revision 2, evidence stale), deleted by revision;
   re-search empty; evidence `deleted: true`.
5. `migration backup` of working into a second immutable snapshot, verified,
   restored into a fresh target, verified before any access. Accepted/manual
   content, asset, original bytes, aliases/taxonomy/backlinks, pipeline history
   and chunk cache all byte-exact; completed job identity intact; deleted raw
   passage not searchable; old evidence still resolves with deleted **and**
   stale true, same source_id/content_hash and the historical exact quote
   ("Synthetic scratch document b10zxqwvscrat…"). Informational `get` on the
   deleted source: exit 0, payload shows revision 2 (recorded, not asserted).
6. Real `migration rollback` on the immutable first copy: returned ScanReport
   equals the original dry-run exactly; immutable copy inventory unchanged;
   baseline reader re-run on the original deep-equals the saved baseline
   (modules, config paths, languages, article, raw hits); every original file
   hash unchanged across the whole proof.
7. Resume contract (labeled: **simulated incomplete receipt and conflict
   recovery, NOT a measured SIGKILL/crash experiment**): v1 `copying` receipt
   constructed from the real dry-run output (exact source path/library_id/
   source_manifest, empty destination_files), one correctly seeded file and one
   deliberately conflicting file. Real copy failed (exit 1) without overwriting
   either; receipt stayed `copying`; only the fixture conflict repaired; same
   command retried → `complete`; verify passed; all bytes exact.
8. Negative verify: tampered throwaway restore → real verify exit 1, empty
   stdout. Immutable snapshots never altered; snapshot inventory unchanged.
   Provider fixture received **zero** requests (no model calls).

## Attempt history (all retained remotely, failures were my script's fixture bugs)

- `run-20260926-162553` exit 1: controller wrongly treated scan paths as
  source-relative; fixed to decode the custom-root mapping.
- `run-20260926-162701` exit 1: scratch search token contained 𠮷, matching the
  frozen rare-character article after deletion; token made purely Latin.
- `run-20260926-162849` exit 1: controller typo (`CompletedProcess.exit`).
- `run-20260926-163031` exit 0: full pass (34 steps).

Every actual argv/exit/stdout/stderr is in `evidence/summary.json` (`steps`),
with inventories, module origins, retained IDs and labels. No assertion was
relaxed to pass; no failure was wrapped into exit 0.
