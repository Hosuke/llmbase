# B11 — native deterministic lint facade (primary-defined implementation task)

B10 is reviewed, independently verified and checkpointed as a27595c. Implement
this one task now. No simultaneous B10/B11 implementation or new subagents.

## Diagnosed defect and scope

`compat_maintenance.rs` already implements the snapshot, eight checks and Rust
sequencing. `crates/llmbase-python/src/maintenance.rs` exists but lib.rs does not
register it. `_native.py` has no bridge; `lint.checks.lint()` still runs Python.
Siwen additionally installs `_check_stubs_siwen(cfg)`, whereas old lint passes
`articles=`. Primary verified these facts directly; do not reopen architecture.

An actual before-fix reproduction and source hashes are supplied in
`.rust-migration-runs/b11-primary-repro.py` and `b11-primary-before.json`.
The unchanged Siwen callback fails binding before its body runs. Preserve this
failure evidence; the new candidate must also pass the equivalent cfg-only
regression. This extracted-callback proof is not full downstream integration.

Own only:

- crates/llmbase-python/src/lib.rs
- crates/llmbase-python/src/maintenance.rs
- llmwiki/_native.py
- llmwiki/lint/checks.py
- new tests/rust_migration/test_native_lint.py
- the assigned private run directory and REPORT.md

Do not change Rust core algorithms, existing tests, Siwen, worker, normalizers,
taxonomy generation, lint_deep/fixes/dedup standalone routines, versions or
release metadata. Report a demonstrated core mismatch with a reproducer; the
primary will define a repair. Do not git stage/commit/push.

## Required implementation

1. Declare/register the existing maintenance FFI. Format the newly wired file.
   Add `_native.maintenance_transform(action, args)` and
   `_native.run_maintenance(action, args, callback)` using the existing
   utility_transform/run_query JSON and ContextVar trace conventions. Preserve
   the original Python callback exception, not a generic wrapper or retry.
   Clear stale traces on entry/error; only successful Rust work gets its trace.

2. Add a native default branch to `_load_articles(concepts_dir)` using transform
   `load`. Restore each article `path` to Path at the Python boundary. Retain the
   source-checkout legacy branch under the existing `_native.available()`
   policy; installed candidates and REQUIRE_NATIVE must fail closed.

3. Native `lint(base_dir)` still calls current load_config/ensure_dirs and the
   live `_load_articles` callable once. Prepare a JSON DTO with concepts_dir,
   meta_dir, articles and current ALLOW_CJK_SLUGS; convert article path fields
   explicitly, not arbitrary metadata with `default=str`. Native `prepare`
   supplies aliases, backlinks and index presence. Do not infer/register a
   second library root from concepts_dir. Explicit config is authoritative.

4. Capture stable default check identities after definitions, before downstream
   patching. At each Rust callback stage resolve the live module attribute.
   Default structural/broken_links/orphans/missing_metadata/dirty_tags/
   duplicates/stubs return null so Rust executes the check. A replaced callable
   is a genuine Python extension and returns {override:true,value:result}.
   Keep one shared Path-valued article snapshot for the Python callbacks.
   Pass cfg; articles for the first seven; additionally existing_slugs as a set
   and aliases for broken_links. Respect cfg-only overrides.

5. Bind callback arguments before invocation using inspect.signature. Retain
   only supported optional kwargs unless **kwargs is accepted; preserve required
   parameters and positional-only handling. If binding fails, raise without
   executing. If introspection is unavailable, invoke the historical full shape
   exactly once and propagate its error. Never catch a callback body's TypeError
   to retry with fewer arguments. Check functions must return actual lists.

6. Uncategorized runs last, as before. If replaced, call it with supported cfg/
   base_dir arguments. Otherwise call the live taxonomy.build_taxonomy(base_dir,
   lang='en') at this stage, then call the native `check` transform with
   name='uncategorized', the prepared snapshot and returned categories; return
   that native result through the host override envelope. This intentionally
   retains taxonomy as a separately documented boundary and avoids eagerly
   building taxonomy when a replacement handles this stage.

7. Rust run_lint owns ordering, total and emit stage. The host dispatches the
   actual hooks.emit once with the original event/kwargs. Do not add a second
   Python issue-counting loop or second hook. The final trace identifies the
   actual maintenance_run operation. Record Python callback/taxonomy boundaries
   honestly; this task does not make the entire maintenance suite native.

## Required regression evidence

Use an isolated synthetic custom-root library, classical bare-body content and
non-BMP text; original raw/concept bytes and unknown metadata stay unchanged.

- Actual installed fresh wheel, outside source cwd, REQUIRE_NATIVE=1. Record
  wheel/extension/source hashes and imported llmwiki origins; no skips accepted
  for the new native tests. Existing B5.1 wheel is not this candidate.
- Default lint produces the eight concrete result arrays and correct total;
  native prepare/load/check/run receipts correspond to actual calls. Instrument
  Python default check/load code entry to prove those old algorithms were not
  entered; taxonomy may be an explicit fixture callback for this assertion.
- Replace check_stubs with cfg-only callable: invoked once, result counted.
  Replace again between lint calls and change ALLOW_CJK_SLUGS: current values
  take effect. Test full-signature, supported-subset and **kwargs overrides.
- A callback raises a sentinel TypeError: preserve that exception, execute once,
  no success trace and no after_lint_check event. Also test binding failure.
- Callback sees Path-valued article paths, set-valued existing_slugs and aliases.
  A replaced `_load_articles` is honored once. No accidental phantom library
  registration is introduced.
- Default uncategorized uses the live taxonomy function at the final stage;
  replacing check_uncategorized avoids taxonomy execution. Validate base_dir.
- Real registered after_lint_check hook receives correct total/results exactly
  once; callback ordering matches the old eight-stage sequence.
- Existing tests/test_lint.py and downstream contract tests retain expectations.
  Do not edit baseline tests to absorb a compatibility regression.
- Report unsupported DTO types explicitly with reproducer; do not silently
  stringify YAML values or present partial compatibility as complete.

## Remote validation and handoff

Build only in a new isolated baypier-prod task directory using existing isolated
CARGO_HOME/RUSTUP_HOME, CARGO_BUILD_JOBS=2, nice 10 and finite tmux. Do not use
/srv or original downstream trees. Primary supplies the exact snapshot/runner
at dispatch. Record commands, logs and exit codes; leave source unchanged during
build. First fmt/clippy and compat_maintenance tests, then a fresh wheel and the
focused native lint + old lint + downstream contract tests. Run full Python
candidate suite only after focused tests pass. Do not repeat unrelated native
full-workspace tests when native core is unchanged; final integration gate will
cover the assembled candidate.

Timebox implementation and focused validation to 30 minutes initially; save
REPORT before the limit even if a remote finite build remains active. Include
actual PID/session/log paths, owned file diff, failures, callback boundaries and
remaining limitations. Primary reviews, verifies and checkpoints before the
next task.

## Exact dispatch inputs and runner

Local task root: `.rust-migration-runs/b11-native-lint`. `before/` contains the
four existing owned files; the new test has no baseline. `source/` is a frozen
complete build/test snapshot supplied by primary, with `source-before.sha256`.
After implementing in the worktree, overlay ONLY the four owned files and new
test into this private snapshot, preserving their relative paths, and record
`source-after.sha256`. Verify the complete remote snapshot against it before
building and after tests. Do not pull runtime sources from an older task root.
The installed candidate's packaged Python files and extension must match this
snapshot/build receipt; save the module origins and wheel/extension hashes.

Remote task root:
`baypier-prod:/root/llmbase-rust-migration-20260920/b11-native-lint`.
Remote finite tmux: `llmbase-b11-native-lint`. Use a fresh source directory and
fresh build/wheels/candidate/test-harness directories under this task root.
Create directories before rsync. Preserve snapshot paths; no source edits while
build/tests run. Supply a finite overall timeout and paired log/exit files.

Existing isolated toolchain (do not install system tools or cargo clean):

```
CARGO_HOME=/root/llmbase-rust-migration-20260920/cargo
RUSTUP_HOME=/root/llmbase-rust-migration-20260920/rustup
PATH=$CARGO_HOME/bin:/root/llmbase-rust-migration-20260920/python/bin:$PATH
PYO3_PYTHON=/root/llmbase-rust-migration-20260920/python/bin/python
CARGO_TARGET_DIR=/root/llmbase-rust-migration-20260920/source/target
CARGO_BUILD_JOBS=2
```

The repository uses setuptools-rust (pyproject.toml), NOT maturin. The existing
`scripts/rust-migration/check-candidate.sh` documents the build/install/provenance
pattern. Create a fresh venv, build with `python -m build --wheel`, install that
new wheel into a different fresh candidate venv with pytest/pytest-timeout and
the needed existing project test extras. Reusing the old candidate's installed
llmwiki is not acceptance. No provider credentials, live model calls or real
corpus; clear provider credential/endpoint environment before tests.

Rust checks: cargo fmt --all -- --check; cargo clippy -p llmbase-core
-p llmbase-python --all-targets -- -D warnings; cargo test -p llmbase-core
--test compat_maintenance. Nice 10, jobs 2. No unrelated full workspace rebuild.

Candidate tests from a directory outside the source tree, REQUIRE_NATIVE=1:
first new native lint tests + tests/test_lint.py + the actual downstream
contract tests under tests/rust_migration (locate names; do not invent a pass).
Use --import-mode=importlib and a test-harness containing copied test helpers
for multiprocessing, as in check-candidate.sh. Check actual llmwiki origins are
inside the candidate venv. After the focused group passes, run the full Python
suite once with a finite pytest timeout and preserve full output. A real new
failure gets a reproducer/report, never a hidden skip or adjusted expectation.

Local primary wrapper `llmbase-b11-lint-k3` runs this task. Do not manipulate it,
create local pollers, start a second agent, stage or commit. Hard wrapper deadline
35 minutes. Save REPORT before minute30 with remote process/session/log state;
primary collects a still-running finite remote job. Stop after the report.
