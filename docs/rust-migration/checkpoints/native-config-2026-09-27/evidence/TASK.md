# B16 — shared native config IO and directory orchestration

Predecessor af16b2d. Primary owns diagnosis/implementation after repeated K3
connection failures. A separate read-only design reviewer may inspect pitfalls;
no concurrent source writer. No provider/corpus/downstream edits or dependencies.

The current Python config loader does filesystem IO and path/creation sequencing.
The native LibraryConfig already has its own Rust YAML parser. PyYAML safe_load
supports dates/nonfinite numbers/binary/sets/ordered pairs/cyclic aliases that
must not be flattened through JSON. Use an explicit format-parser callback for
the Python facade, as allowed by the full goal's external parser boundary.
Native standalone keeps its actual Rust parser; never call Python from native CLI.

Shared core compat_config:
- bounded config.yaml read (actual bytes <=16MiB), missing returns None; no dirs,
  registry or DB. LibraryConfig::load uses this helper with its existing parser
  and existing strict base/root/default contract.
- read_with owns missing/existing branch, decoder/default invocation, five path
  keys raw/wiki/outputs/meta/concepts in legacy order, truthy path normalization,
  and final base_dir assignment. Callback steps adapt an opaque Python document;
  do not serialize arbitrary config values through JSON. Preserve alias identity.
- ensure_dirs_with owns five-key sequential lookup/create_dir_all, preserving
  existing partial-side-effect/error order rather than eagerly validating all.
- default_paths returns the five canonical absolute root values for a caller
  supplied base; profile/default policy and live language deepcopy stay Python.

PyO3 host retains original callback exceptions/no retry. Python YAML decoding is
explicit in the receipt (parser=python_yaml); core=rust refers to actual IO and
orchestration, not native YAML parsing. Decoder receives the read bytes, performs
the legacy UTF-8/universal-newline text adaptation and calls the live safe_load.
Path coercion stays a thin Path/fspath adapter, no Python read/mkdir algorithm.
Opaque cfg get/set in the host retains unknown fields, typed keys, cycles, live
default helper results and original error ordering. No implicit defaults merge.
Bridge clears trace on entry and any exception, including reentrant native calls.
_read_config never registers; load_config remains the explicit Python registry
boundary. Unbuilt fallback stays available; required native fails closed.

Independent design review: keep compat_io::resolve_path unchanged in this task.
It also serves authorization/recovery callers; changing it would unnecessarily
expand this config task's scope. Add a config-specific bounded non-strict native
resolver. Follow symlinks before processing parent components, and test against
actual pathlib on isolated normal, missing, parent, alias, dangling-target and
loop cases. Do not claim to repair the separate compat_io limitations.

Preserve the first exists observation: absent/dangling config uses defaults;
an open/read failure after observed existence remains an error, not defaults.
Opaque get/truth/coerce/set operations and mkdir must remain sequential. Preserve
the original Python exception object for callbacks, and OS errno/filename and
exception subclass for native IO errors. LibraryConfig::load shares only the
bounded bytes reader, keeping its native parser and existing validation.

Ownership: core config.rs/lib.rs and new compat_config.rs;
Python FFI lib.rs/new config.rs; llmwiki/config.py/_native.py; new core config
tests and Python native config tests; config contract doc; this task run files.
No existing tests/baselines, dependencies, other domain algorithms or release edits.

Acceptance: frozen actual accepted-config comparisons and unknown typed-value
identity, live defaults/parser exceptions, partial mkdir failures/OS error classes,
no read-only side effects/registration, poison legacy read/mkdir paths, native
standalone core tests with no Python callbacks, original B15 path/journal tests.
Fresh remote wheel/provenance; fmt/Clippy and core tests (including recovery),
focused and full Python. Remote isolated baypier-prod b16-native-config under
finite tmux llmbase-b16-native-config; keep logs/exits/source hashes. Primary
reviews/checkpoints after acceptance, then continues remaining full goal.
