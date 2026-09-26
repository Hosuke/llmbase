# B8: diagnose and repair the raw-preview configuration regression

DISPATCHED after accepted B7 checkpoint b42def8. Implement now.
Primary has diagnosed the exact cause. Do not spend this task rediscovering it.

Frozen failing test in the current native workspace:
`web_article_pagination_sections_raw_and_statistics_follow_real_content`
in crates/llmbase-core/tests/web_read.rs, line 181. Expected `𠮷原典`, got the
entire raw body. Keep that exact user-facing Unicode truncation expectation.

Primary isolated CLI diagnostic on the B6 first-pass release shows literal
YAML `sources:\n  max_content_chars: 3\n` returns exactly `𠮷原典`, with
original bytes unchanged. Evidence:
.rust-migration-runs/b6-primary/yaml-cap-diagnostic.log and its script.
The fixture currently writes serde_yaml::to_string(&serde_json::Value).
Primary remote diagnostic confirms the generated YAML is:
sources:
  max_content_chars:
    $serde_json::private::Number: '3'
The loaded setting is {"$serde_json::private::Number":"3"}, not an integer.
The frozen test consequently returns the full body (exit 101). Evidence is in
baypier-prod:/root/llmbase-rust-migration-20260920/b8-primary/diagnose.log.
The literal YAML native CLI probe already passes. Production truncation and
configuration parsing do not need changes for this defect.

Ownership ONLY: crates/llmbase-core/tests/web_read.rs and this private task
directory. No production code, dependency or other test changes.

Exact implementation:
- In fixture(Some(config)), replace serde_yaml::to_string(&config) with
  serde_json::to_string(&config). The config loader explicitly supports JSON
  syntax and this preserves integer types under arbitrary_precision.
- Assert in the existing raw-preview test that the loaded cap is integer 3.
  Keep every existing expected response, especially content == "𠮷原典".
- Add a small literal YAML test: build fixture(None), drop its Library handle,
  write `sources:\n  max_content_chars: 3\n`, load LibraryConfig and Library
  again using the same "web" ID. Assert loaded integer 3, actual web_source
  returns exactly "𠮷原典", and raw/book/index.md bytes remain unchanged.
  This retains explicit YAML coverage when the general fixture uses JSON.
- Do not add a new YAML conversion framework or relax production validation.

Acceptance:
1. Preserve primary's existing before-failure evidence; a further diagnosis run
   is unnecessary. Freeze the new test/expected values before the fixed run.
2. If the fixture is invalid, correct its encoding to a documented supported
   config representation. Preserve all existing expected responses and checks;
   do not skip assertions, relax content caps or change production defaults.
3. A real literal YAML input must exercise the config loader and native web
   read path with numeric 3, non-BMP original text and unchanged source bytes.
   Ensure the fixture cannot silently pass a mapping instead of an integer.
4. Run the complete web_read and document-contract targets; workspace fmt,
   all-target Clippy -D warnings, and workspace tests --locked --no-fail-fast.
   Report other failures without fixing beyond ownership.
5. If all workspace tests pass, build the actual release CLI and record its
   source manifest and binary hash; no stale target-artifact proof.

All cargo test/clippy/build remote under finite tmux on baypier-prod, isolated
/root/llmbase-rust-migration-20260920/b8-web-read-config. Use the configured
migration CARGO_HOME/RUSTUP_HOME/PYO3_PYTHON, shared target cache, two build jobs,
nice -n10. Local editing/rustfmt/orchestration only. No local tmux, polling
driver, provider request, secrets, production, subagents, stage/commit or next
task. Preserve source snapshots and actual before/fixed exits. No rm/clean.

Timebox 12 minutes, hard 15. Save REPORT.md before the deadline with actual
process state and remote log/session paths if a finite check still runs.

Source snapshot is prepared at this task's snapshot/ (119 files), exact current
tree including accepted B7 repair. before/ stores your only owned file.
Use this immutable snapshot, overlay the repaired test in a fresh remote root,
and record all 119 input hashes. Do not copy diagnostic B8 or stale B6 source.
Toolchain: CARGO_HOME=/root/llmbase-rust-migration-20260920/cargo;
RUSTUP_HOME=/root/llmbase-rust-migration-20260920/rustup;
PATH=$CARGO_HOME/bin:/root/llmbase-rust-migration-20260920/python/bin:$PATH;
PYO3_PYTHON=/root/llmbase-rust-migration-20260920/python/bin/python;
CARGO_TARGET_DIR=/root/llmbase-rust-migration-20260920/source/target;
CARGO_BUILD_JOBS=2; nice -n10. Reuse cache, never cargo clean.
Remote tmux llmbase-b8-web-read-config. No local tmux: primary owns wrapper.
After all checks pass: cargo build --release -p llmbase-cli --locked, then copy
that exact fresh CLI under your run root and record its sha256/version. This is
build evidence, not permission to publish. Batch related reads/commands, save
REPORT.md before deadline and stop. No next task, staging or commit.
