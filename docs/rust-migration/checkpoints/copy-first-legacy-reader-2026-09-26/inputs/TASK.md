# B10: copy-first migration and actual legacy reader proof

DISPATCHED after accepted B9 checkpoint 944acae. Implement now. Primary
has inspected migration.rs, its unit tests, the CLI migration test and the
baseline Python export/search/config code. This is a verification script task,
not an invitation to redesign migration or discover the release requirements.

Ownership: one script scripts/rust-migration/verify-migration.py, migration
runbook paragraph and this private run directory. Create synthetic fixtures
inside the script's fresh run root; no unrelated new framework or fixture suite.
No engine, Python facade or existing test changes. Report a concrete runtime
defect for primary diagnosis; do not silently relax assertions.

Use the accepted release binary/hash from B8 and the current source manifest.
All execution on baypier-prod in a new isolated directory, finite tmux. No real
documents, production, model requests or original downstream tree mutations.

Concrete implementation:
1. Create a synthetic legacy library with custom raw/wiki/concepts/outputs/meta
   paths and a bare-body Classical Chinese language profile. Reuse the already
   frozen downstream config/rare-character article as a basis. Include raw
   original bytes, accepted concept/output, unknown nested metadata, aliases,
   taxonomy with manual/locked data, backlinks, a wiki asset, pipeline history
   and chunk-cache file. Record every original file hash before any invocation.
2. Primary will supply a pristine archive of baseline commit
   a7c4d9da764462e09c0769ecd8ba548a46d27e83, its archive hash and dependency-ready
   external Python interpreter. The old-reader subprocess must import llmwiki
   from that archive (assert all loaded llmwiki.__file__ origins and hashes;
   llmwiki._core absent from sys.modules), then call the actual old
   config.load_config, export.export_article and search.search_raw. Save a
   deterministic baseline of content/language/raw search, not just imports.
   Do not edit any legacy source to make it pass. This separate reader proof
   legitimately uses Python; do not call it interpreter-free runtime evidence.
3. Run real native JSON migration dry-run on original. Assert no original
   files/directories were added/changed, no native DB, unknown metadata is
   reported, sizes/hashes are valid, parse/identity errors are empty. Then
   migration copy to a separate target and migration verify. Assert exact
   original bytes, copied content/custom-root mapping and valid library scope.
4. Keep that first migration copy IMMUTABLE for the final rollback check.
   Use migration restore from it into a new working library. Verify before
   exercising writes (verify compares an immutable receipt). In working,
   source_write a scratch raw document, obtain evidence, update with expected
   hash and delete; enqueue and run a finite index job. Retain actual source,
   revision, evidence and job IDs. All are synthetic.
5. Backup working to a new immutable snapshot, verify it, restore to a fresh
   target and verify before any mutating access. Check accepted/manual content,
   assets, original bytes, metadata, completed job and deleted source/history
   survive. A search must not return deleted raw passages; old evidence must
   still resolve to its historical quote and deleted/stale status. Include
   source revision/job identity checks, not just article counts.
6. Real migration rollback on the initial immutable copy must verify the
   preserved original. Invoke the ACTUAL baseline Python reader on that
   original again and compare baseline results, source/package provenance and
   every original file hash. Rollback verifies switching back to the retained
   original; it does not mutate the new runtime or reverse its format in place.
7. Deterministic resume contract: separate synthetic source and target. From
   its real dry-run output construct a v1 `copying` receipt with exact source
   path/library_id/source_manifest and empty destination_files, as produced
   before the copy loop (migration.rs). Seed one correctly copied source and
   one conflicting byte file. Real copy must fail without overwriting either;
   repair only that deliberate fixture conflict, retry same command, verify
   completed receipt and exact bytes. Label this simulated incomplete receipt
   and conflict recovery, NOT a measured SIGKILL/crash experiment. Existing
   native recovery tests separately establish process-crash behavior.
8. Negative verify: tamper with a separately restored throwaway copy, assert
   real verify exits nonzero. Never alter immutable accepted snapshots.

Capture each actual argv/exit/stdout/stderr, binary/source/legacy/fixture hashes,
old-reader module origins, before/after inventories and final summary JSON.
No wrappers that turn failures into exit 0. New run root required; finite child
timeouts and finally cleanup. Do not broaden into another migration platform.

Exact inputs supplied by primary:
- Accepted native119-source manifest: accepted-source.sha256 in this task.
- Fresh pinned release binary on baypier-prod:
  /root/llmbase-rust-migration-20260920/b8-web-read-config/llmbase-cli-bin
  SHA256 f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8.
  Copy once into your isolated root and verify. No cargo build/test is needed.
- Local legacy-a7c4d9d.tar.gz is a pristine git archive of the baseline package
  and pyproject, created by primary. SHA256
  fda78e50bcb93aa0992aeabe9cd45ae1e39b9600fa24b8814ea240e792e1e222.
  legacy-source-sha256.json records all46 archived file hashes. Extract safely
  into a fresh legacy-source directory; verify actual module file hashes.
- External Python/controller/legacy dependencies are available at:
  /root/llmbase-rust-migration-20260920/b5-registration-fix/candidate/bin/python.
  Legacy reader is a SEPARATE subprocess with legacy-source first in sys.path;
  assert every loaded llmwiki module is from that snapshot, NOT installed wheel,
  and llmwiki._core is absent. Return import identities and actual read results.
- inputs/tests/rust_migration/fixtures/downstream/library/ contains three
  frozen example files for custom paths/bare-body/rare characters. Use them as
  fixture basis, adding only synthetic content as the specified proof needs.
- before/ saves the current runbook. verify-migration.py is a new owned file.

Command/schema references, already checked by primary:
crates/llmbase-core/src/migration.rs (inspect, copy_first, verify_copy,
verify_rollback, restore; copying receipt fields); CLI migration test in
crates/llmbase-cli/tests/transports.rs:170. Real mutation/job/evidence command
forms are in scripts/rust-migration/verify-standalone.py main(): use op
source_write/source_delete/get/search/evidence/job_enqueue/job_run_one/job_get
with --json and --args JSON, not invented legacy API names.

Remote root /root/llmbase-rust-migration-20260920/b10-migration-old-reader;
finite remote tmux llmbase-b10-migration-old-reader. Local primary wrapper is
llmbase-b10-migration-k3; do not manipulate/reuse it or create local tmux/pollers.
Create destination dirs before rsync; preserve paths, check input hashes before
run, save every actual argv/stdout/stderr/exit and script hash. Nice -n10.
No repeat full workspace, real credentials, providers, production or live corpus.
Timebox20 minutes, hard25. Save REPORT.md with actual process state before
deadline; primary collects any still-running finite remote runner. Do not leave
a local background wait task expecting a notification. Stop after report.
No commit/stage/publish/next task, runtime source changes or subagents.
