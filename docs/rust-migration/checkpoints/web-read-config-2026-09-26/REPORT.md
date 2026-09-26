# B8 REPORT — raw-preview configuration regression repair

Status: RUNNING (remote checks in flight). This file is saved before the
deadline with actual process state; it will be updated when the finite remote
run finishes. Remote tmux session: `llmbase-b8-web-read-config` on baypier-prod.
Remote run root: `/root/llmbase-rust-migration-20260920/b8-web-read-config`.

## Defect (diagnosed by primary, confirmed in source, not re-diagnosed)

- Fixture wrote `serde_yaml::to_string(&serde_json::Value)`; under
  `arbitrary_precision` that emits `$serde_json::private::Number` tagged maps,
  so `sources.max_content_chars` loaded as `{"$serde_json::private::Number":"3"}`
  instead of integer 3 (`crates/llmbase-core/src/web_read.rs:411-423` then falls
  back to the 50_000 default and returns the full raw body).
- Loader (`config.rs:23-24` → `document.rs:31-36` `parse_metadata`) explicitly
  tries JSON first, then `compat_yaml`; both are documented supported syntaxes.

## Repair (ownership: crates/llmbase-core/tests/web_read.rs only)

1. `fixture(Some(config))` now writes `serde_json::to_string(&config)` — the
   supported JSON config syntax, which preserves integer types.
2. Existing raw-preview test
   `web_article_pagination_sections_raw_and_statistics_follow_real_content`
   now asserts `settings["sources"]["max_content_chars"].as_i64() == Some(3)`
   (cannot silently pass a mapping). All existing expectations kept, including
   `content == "𠮷原典"` (line 181 area).
3. New test `web_source_literal_yaml_cap_loads_integer_and_preserves_raw_bytes`:
   `fixture(None)`, drops the Library handle, writes literal
   `sources:\n  max_content_chars: 3\n`, reloads `LibraryConfig`/`Library` with
   the same `"web"` ID; asserts integer 3, `web_source` content exactly
   `𠮷原典` (non-BMP), and `raw/book/index.md` bytes unchanged.
No production code, dependency, or other test changes. No new YAML framework.

## Inputs (119-file snapshot, immutable)

- Local: `.rust-migration-runs/b8-web-read-config/snapshot/` — all 119 hashes
  match `manifest.sha256` (shasum -a 256 -c, 0 mismatches).
- Remote overlay root: snapshot rsynced to
  `baypier-prod:/root/llmbase-rust-migration-20260920/b8-web-read-config/source`,
  `sha256sum -c ../manifest.sha256` → REMOTE_HASH_MISMATCHES=0.
- Repaired test: `fixed/web_read.rs`
  sha256 `1b981abfc9d9e18a441bdefe04ee14b477b875b7e3987103319932f91c9373fa`,
  overlaid onto the remote snapshot only AFTER the pristine before-run.
- Before-state owned file preserved in `before/crates/.../web_read.rs`.

## Remote run (finite tmux `llmbase-b8-web-read-config`, script `run_checks.sh`)

Toolchain per dispatch: CARGO_HOME/RUSTUP_HOME/PYO3_PYTHON migration paths,
CARGO_TARGET_DIR=/root/llmbase-rust-migration-20260920/source/target (shared
cache, no clean), CARGO_BUILD_JOBS=2, nice -n10. Started 15:21:11 CST.

Steps (each writes `<name>.log` + `<name>.exit` under the run root):
1. `before-web_read`: pristine snapshot `cargo test -p llmbase-core --test web_read --locked`
   (expected exit 101 — actual before-failure evidence; primary's
   b8-primary/diagnose.log and b6-primary/yaml-cap-diagnostic.log preserved).
2. Overlay repaired test, record `fixed-overlay.sha256`.
3. `fmt`: `cargo fmt --all -- --check`.
4. `fixed-web_read`: full web_read target.
5. `document-contract`: `--test document_yaml_contracts`.
6. `clippy`: `cargo clippy --workspace --all-targets --locked -- -D warnings`.
7. `workspace-tests`: `cargo test --workspace --locked --no-fail-fast`.
8. `release-cli` (only if 3-7 all exit 0): `cargo build --release -p llmbase-cli --locked`,
   binary copied to run root as `llmbase-cli-bin` with sha256 + `--version`.

## Results

### Pass 1 (15:21:11) — before-failure evidence captured; post-overlay steps invalid

- `before-web_read.exit = 101` (pristine snapshot): panicked at
  `crates/llmbase-core/tests/web_read.rs:181:5` —
  `left: String("𠮷原典 body beyond limit.")`, `right: "𠮷原典"` — exactly the
  frozen regression. 3 other web_read tests passed. Log: remote
  `$R/before-web_read.log`.
- Pass-1 post steps ran on the PRISTINE tree: `run_checks.sh`'s
  `cp $R/fixed/web_read.rs …` failed (`fixed/web_read.rs` had not landed from
  the batched rsync; script had no `set -e`), so `fixed-web_read` (101),
  `workspace-tests` (101), `fmt` (0), `clippy` (0), `document-contract` (0)
  are pristine-tree results. They are preserved as `*.pass1.log/.exit` and are
  NOT valid fixed evidence. Release build was skipped (exit 99 placeholder).

### Pass 2 (15:25:38, tmux `llmbase-b8-web-read-config`, script `run_fixed.sh`)

- Overlay verified BEFORE relaunch: remote `fixed/web_read.rs` sha256
  `1b981abfc9d9e18a441bdefe04ee14b477b875b7e3987103319932f91c9373fa`
  (matches local fixed file and repo working copy); `run_fixed.sh` hard-fails
  (exit 96 → `overlay.exit`) if the cp fails, and records
  `fixed-overlay.sha256` after copying.
- Steps: fmt --check → web_read (fixed) → document_yaml_contracts →
  clippy --workspace --all-targets -D warnings → workspace tests
  --locked --no-fail-fast → (if all 0) release `llmbase` build + binary
  sha256/version under `$R/`.
- Marker `$R/done2`; exits/logs at `$R/{fmt,fixed-web_read,document-contract,
  clippy,workspace-tests,release-cli}.{exit,log}`.

FINAL: pass-2 exits pending at REPORT save time (saved before hard deadline
with actual process state). Remote session/log paths above; local waiter
collecting `$R/done2`. Update below if the run completes in-window.

PASS2-FINAL (completed ~15:31 CST, marker `$R/done2`, tmux session exited):

- `fmt.exit = 0` — `cargo fmt --all -- --check`
- `fixed-web_read.exit = 0` — 5 passed, 0 failed, including
  `web_article_pagination_sections_raw_and_statistics_follow_real_content`
  (the frozen regression, `content == "𠮷原典"` holds) and the new
  `web_source_literal_yaml_cap_loads_integer_and_preserves_raw_bytes`.
- `document-contract.exit = 0` — `document_yaml_contracts` target.
- `clippy.exit = 0` — `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `workspace-tests.exit = 0` — `cargo test --workspace --locked --no-fail-fast`;
  every suite `0 failed` (full list in `$R/workspace-tests.log`).
- No other failures to report; nothing outside ownership was changed.
- Overlay proof: `$R/fixed-overlay.sha256` =
  `1b981abfc9d9e18a441bdefe04ee14b477b875b7e3987103319932f91c9373fa`
  `crates/llmbase-core/tests/web_read.rs` (matches local fixed file).
- Release CLI (built only after all checks passed, from the overlaid tree):
  `cargo build --release -p llmbase-cli --locked` (1m39s), binary copied to
  `$R/llmbase-cli-bin`, sha256
  `f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8`,
  `--version` = `llmbase 0.1.0` (`$R/llmbase-cli-bin.sha256/.version`).
  Build evidence only — not published; no staging or commit performed.

Outcome: regression repaired within ownership; before/fixed exits preserved
(101 → 0); all acceptance gates green.
