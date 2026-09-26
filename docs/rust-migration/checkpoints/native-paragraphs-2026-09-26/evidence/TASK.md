# B12 — native paragraph normalization, after accepted355db6f

Implement this bounded task in /Users/lanchair/Developer/LLM-infra/Github/llmbase-rust.
Primary has diagnosed the missing native action and frozen the independent
legacy oracle. Do not reopen architecture or delegate discovery. No other task,
subagent, git stage/commit/push, version change or downstream edit is authorized.

Own exactly:
- crates/llmbase-core/src/compat_util.rs
- new crates/llmbase-core/tests/compat_paragraphs.rs
- llmwiki/normalize.py (native normalize_paragraphs branch only)
- new tests/rust_migration/test_native_paragraphs.py
- .rust-migration-runs/b12-paragraphs/ runner/report files, excluding primary/

Primary owns the frozen tests/rust_migration/fixtures/paragraphs.json. It has229
real old-package results including38 existing test functions, fixed-seed block
combinations, all Python splitlines separators, CJK/non-BMP/combining characters,
mixed fence closers, Unicode case/digits, containers and live custom punctuation.
SHA256:d02ee222308ad13f54b8a176c97e8bfefff305c5a0414f1e0e6b43ea7026b11e.
Source SHA256:b0411b409377b6c19fc0ecca29b07dd8481fe74b859f066029f00a92cd25c4e1.
Do not change/generate oracle expectations, existing tests or normalize_heads.

## Implementation contract

Existing path: _native.utility_transform -> Python util FFI -> compat_util::transform.
Add action normalize_paragraphs with required string fields body, terminators,
wrappers; return an actual string. Use that existing pure boundary. No new
protocol, services, dependencies, library registration, file/DB IO or model call.
Rust regex is already available; use existing whitespace(c) when matching Python
strip/isspace semantics (extra U+001C..U+001F matter).

Translate the existing paragraph state machine exactly. Preserve fence opening
and close length/type rules, containers/lazy continuations, list continuation
across blanks, indented code, headings including empty headings, tables, HTML,
references and thematic breaks. Python Unicode regex matching has subtleties:
do not replace Unicode digit/space predicates with ASCII-only rules. The legacy
fence-close regex allows mixed backtick/tilde sequences; match observed behavior.
Its HTML IGNORECASE includes Python's additional Unicode letter matches. The
oracle contains adversarial examples; inspect the current regexes/state machine.

Python splitlines includes CR/LF/CRLF, VT/FF, U+001C..U+001E, NEL, U+2028/U+2029,
but not U+001F. Empty input yields no lines and one terminal separator does not
create a final empty element. Output chooses CRLF if ANY CRLF occurred; otherwise
LF. Trailing newline restoration counts only original terminal LF/CRLF as in the
existing helper. Preserve original line contents except legacy merge trimming.

At normalize_paragraphs entry read the CURRENT SENTENCE_TERMINATORS and
CLOSING_WRAPPERS and forward each call. Native errors propagate. Retain current
unbuilt-source fallback under _native.available(); installed required-native
must fail closed. Do not change default constants or cache effective values.
Preserve public helper imports and the untouched normalize_heads algorithm.

## Tests and acceptance

Rust integration target loads the frozen fixture via include_str! and executes
the actual transform for ALL229 cases. Also cover bad DTO fields/unknown actions.
Existing compat_util target must pass unchanged. Python new tests use the actual
fresh installed extension, compare all frozen cases and verify native operation
trace. Poison old Python helper bodies to prove the algorithm did not run there.
Two successive calls must observe changed terminators AND wrappers. Check that
configured library roots are unchanged. Exercise development fallback separately
and ensure native errors are propagated rather than silently falling back.
Do not mock native results as evidence. Existing test_normalize.py stays intact.

## Frozen snapshot and remote runner

Task root .rust-migration-runs/b12-paragraphs. before/ stores the two existing
owned files. source/ is primary-supplied coherent B11 snapshot plus the frozen
oracle. source-before.sha256 identifies all inputs. Overlay ONLY the four owned
implementation/test paths after editing, then write source-after.sha256. The
frozen oracle remains unchanged. Verify every source hash before and after tests.

Remote task root:baypier-prod:/root/llmbase-rust-migration-20260920/b12-paragraphs.
Remote finite tmux:llmbase-b12-paragraphs. Create root before rsync; preserve paths.
No /srv, production corpus, system Python changes, cargo clean, provider calls.
Use shared isolated CARGO_HOME/RUSTUP_HOME below the migration root, PATH with
cargo/bin and python/bin, PYO3_PYTHON=.../python/bin/python, CARGO_BUILD_JOBS=2,
CARGO_TARGET_DIR=.../source/target and nice10. B11 is terminal; no shared-target
build is running. Local builds/full tests forbidden.

Use the proven B11 primary/verify.sh and launch.sh pattern. Important: runner
Python has no ensurepip. Create build/candidate venvs --without-pip, install via
existing .../python/bin/python -m pip --python NEW_VENV/bin/python. Build with
setuptools-rust (python -m build --wheel --no-isolation), NOT maturin. Reusing
B11 build-env-v2 only as a build tool is allowed; wheel output and installed
candidate MUST be fresh under B12. Freeze dependencies from B11 candidate,
excluding llmwiki/pip/setuptools/wheel, then install wheel and the same test extras.

Phases with separate log/exit receipts:
1. source hashes; cargo fmt --all -- --check; cargo clippy -p llmbase-core
   -p llmbase-python --all-targets -- -D warnings.
2. cargo test -p llmbase-core --test compat_util --test compat_paragraphs.
3. Fresh wheel/install/provenance: all48 Python file bytes equal snapshot;
   actual llmwiki/_core origins inside candidate; record wheel/extension SHA256.
4. Outside source cwd, copied test-harness helpers, LLMBASE_REQUIRE_NATIVE=1,
   pytest --import-mode=importlib --timeout=60: tests/test_normalize.py,
   tests/rust_migration/test_native_paragraphs.py, existing downstream contracts.
   No native-test skips accepted. Then run full Python suite once if focused pass.
5. Reverify all source hashes. Overall finite timeout1500s; record runner.exit.

Clear PYTHONPATH and provider key/endpoint environment as in B11 runner. Retain
failed receipts and fix actual owned failures; do not hide failures or alter the
oracle. No full native workspace/release rebuild in this pure bounded change.

Save REPORT.md before minute30 with implementation, exact commands, all results,
actual process/session/log state, failures and boundaries. Stop after this one
task. If provider/API errors prevent progress, preserve files/report promptly;
do not spend the whole interval retrying. Local orchestration has a35-minute
hard deadline. Primary reviews, independently verifies and checkpoints before
the next assignment.
