#!/bin/bash
set -u
TASK=/root/llmbase-rust-migration-20260920/b14-heads
TOOLCHAIN=/root/llmbase-rust-migration-20260920
CANDIDATE=$TASK/candidate
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
step source-before sha256sum -c "$TASK/source-after.sha256"
step fmt cargo fmt --all -- --check
step clippy cargo clippy --locked -p llmbase-core -p llmbase-python --all-targets -- -D warnings
step core-heads cargo test --locked -p llmbase-core --test compat_heads --test compat_paragraphs
mkdir -p "$TASK/wheels"
step wheel "$TOOLCHAIN/b11-native-lint/build-env-v2/bin/python" -m build --wheel --no-isolation --outdir "$TASK/wheels" "$TASK/source"
step candidate-venv "$PYO3_PYTHON" -m venv --without-pip "$CANDIDATE"
step install "$PYO3_PYTHON" -m pip --python "$CANDIDATE/bin/python" install --disable-pip-version-check -c "$TASK/runtime-constraints.txt" "$TASK"/wheels/*.whl pytest pytest-timeout 'mcp>=1.0.0' 'pymupdf>=1.24.0' 'opencc-python-reimplemented>=0.1.7'
unset PYTHONPATH OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY XLLM_KEY
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
export LLMBASE_REQUIRE_NATIVE=1
mkdir -p "$TASK/test-harness"
step test-harness rsync -a --exclude=__pycache__ "$TASK/source/tests/" "$TASK/test-harness/tests/"
cd "$TASK/test-harness" || exit 1
step provenance "$CANDIDATE/bin/python" "$TASK/primary/provenance.py"
step focused "$CANDIDATE/bin/python" -m pytest --import-mode=importlib --timeout=60 \
  "$TASK/source/tests/test_normalize.py" \
  "$TASK/source/tests/rust_migration/test_native_heads.py" \
  "$TASK/source/tests/rust_migration/test_native_paragraphs.py" \
  "$TASK/source/tests/rust_migration/test_downstream_contracts.py" -q
step primary-heldout "$CANDIDATE/bin/python" "$TASK/primary/verify-heldout.py"
step primary-callbacks "$CANDIDATE/bin/python" "$TASK/primary/verify-callbacks.py"
step full-python "$CANDIDATE/bin/python" -m pytest --import-mode=importlib --timeout=60 "$TASK/source/tests" -q
cd "$TASK/source" || exit 1
step source-after sha256sum -c "$TASK/source-after.sha256"
