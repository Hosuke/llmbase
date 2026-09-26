#!/bin/bash
set -u
TASK=/root/llmbase-rust-migration-20260920/b11-native-lint
TOOLCHAIN=/root/llmbase-rust-migration-20260920
export CARGO_HOME=$TOOLCHAIN/cargo RUSTUP_HOME=$TOOLCHAIN/rustup
export PATH=$CARGO_HOME/bin:$TOOLCHAIN/python/bin:$PATH
export PYO3_PYTHON=$TOOLCHAIN/python/bin/python CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=$TOOLCHAIN/source/target
step() {
  local name=$1
  shift
  nice -n 10 "$@" > "$TASK/$name.log" 2>&1
  local code=$?
  echo "$code" > "$TASK/$name.exit"
  echo "$name exit=$code"
  if [ "$code" -ne 0 ]; then exit "$code"; fi
}
cd "$TASK/source" || exit 1
for name in source-before fmt clippy core-maintenance; do
  read -r previous < "$TASK/$name.exit"
  if [ "$previous" != 0 ]; then echo "Missing passing prerequisite: $name"; exit 1; fi
done
step source-resume sha256sum -c "$TASK/source-after.sha256"
step build-venv-v2 "$PYO3_PYTHON" -m venv --without-pip "$TASK/build-env-v2"
step build-deps "$PYO3_PYTHON" -m pip --python "$TASK/build-env-v2/bin/python" install --disable-pip-version-check build wheel 'setuptools>=68.0' 'setuptools-rust>=1.12,<2'
mkdir -p "$TASK/wheels"
step wheel "$TASK/build-env-v2/bin/python" -m build --wheel --no-isolation --outdir "$TASK/wheels" "$TASK/source"
step candidate-venv "$PYO3_PYTHON" -m venv --without-pip "$TASK/candidate"
"$TOOLCHAIN/b5-registration-fix/candidate/bin/python" - <<'PY' > "$TASK/runtime-constraints.txt"
from importlib.metadata import distributions
for d in sorted(distributions(), key=lambda d: d.metadata['Name'].lower()):
    if d.metadata['Name'].lower() not in {'llmwiki', 'pip', 'setuptools', 'wheel'}:
        print(f"{d.metadata['Name']}=={d.version}")
PY
code=$?
echo "$code" > "$TASK/constraints.exit"
if [ "$code" -ne 0 ]; then exit "$code"; fi
step install "$PYO3_PYTHON" -m pip --python "$TASK/candidate/bin/python" install --disable-pip-version-check -c "$TASK/runtime-constraints.txt" "$TASK"/wheels/*.whl pytest pytest-timeout 'mcp>=1.0.0' 'pymupdf>=1.24.0' 'opencc-python-reimplemented>=0.1.7'
unset PYTHONPATH OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY XLLM_KEY
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
export LLMBASE_REQUIRE_NATIVE=1
mkdir -p "$TASK/test-harness"
step test-harness rsync -a --exclude=__pycache__ "$TASK/source/tests/" "$TASK/test-harness/tests/"
cd "$TASK/test-harness" || exit 1
step provenance "$TASK/candidate/bin/python" "$TASK/primary/provenance.py"
step focused "$TASK/candidate/bin/python" -m pytest --import-mode=importlib --timeout=60 \
  "$TASK/source/tests/rust_migration/test_native_lint.py" \
  "$TASK/source/tests/test_lint.py" \
  "$TASK/source/tests/rust_migration/test_downstream_contracts.py" -q
step primary-comparison "$TASK/candidate/bin/python" "$TASK/primary/verify-candidate.py" \
  --library "$TASK/primary/baseline-library" --baseline "$TASK/primary/baseline-lint.json" \
  --output "$TASK/primary/candidate-comparison.json" \
  --siwen-before "$TASK/primary/b11-primary-before.json"
step full-python "$TASK/candidate/bin/python" -m pytest --import-mode=importlib --timeout=60 "$TASK/source/tests" -q
cd "$TASK/source" || exit 1
step source-after sha256sum -c "$TASK/source-after.sha256"
