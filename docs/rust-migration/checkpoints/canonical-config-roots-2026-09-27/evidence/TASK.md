# B15 — canonical default config roots, after accepted a36d463

Primary owns this bounded repair after repeated K3 network failures. No new
K3 dispatch, provider request, production IO or downstream edit.

Actual B13 installed diagnostic: missing config + relative base + native atomic
write fails PermissionError; original Python succeeds and absolute base succeeds.
The default paths already include the relative base prefix; the native FFI joins
that root to an absolute base again. B14 does not change these files.

Repair: resolve the base in config._defaults before forming the five default
paths. Keep the effective config roots absolute, including direct _defaults calls,
relative/current/symlink/nonexistent bases. Existing-file _read_config continues
to resolve paths relative to the library and preserves partial/unknown fields.
Keep _read_config read-only and non-registering; load_config remains the explicit
registration boundary. Preserve live BUILTIN_LANGUAGES/deepcopy and _defaults
helper calls; do not add provider identifiers or schema defaults in Rust.

Ownership: llmwiki/config.py (_defaults base resolution only), new
tests/rust_migration/test_native_config_roots.py, new
docs/rust-migration/config-roots.md, this run directory. No Rust/core scope guard,
writer/journal, dependency/lockfile, old test, or other facade changes.

Acceptance: run new regressions against accepted B14 first to retain real
failures. Test missing/present YAML; relative/absolute/dot/default/symlink/missing
component+.. base forms; config loading creates no directories; registry changes
only after explicit load; paths remain stable after cwd change. Real native
atomic write must preserve original bytes, report library coordination/journal,
and record applied exact before/after bytes in SQLite. A direct scoped native
out-of-root write and reserved-database write remain rejected before mutation.
Verify custom roots, partial/unknown config fields and unchanged YAML bytes;
live language defaults stay deep-copied. Do not touch any source corpus.

Use isolated baypier-prod /root/llmbase-rust-migration-20260920/b15-config-roots,
finite tmux llmbase-b15-config-roots, timeout1500. Fresh wheel/venv required for
the changed Python package; prove native inputs unchanged from B14 and reuse its
passing fmt/Clippy/native checks. Verify48 installed Python file hashes and frozen
input manifest. Run focused config/utilities and full Python once if green.
Primary reviews, packages exact replay, runs precommit checks and saves one Git
checkpoint before starting the next implementation.

This fixes a real integration regression. It does not complete the still-required
native config read/ensure_dirs migration or other full release gates.
