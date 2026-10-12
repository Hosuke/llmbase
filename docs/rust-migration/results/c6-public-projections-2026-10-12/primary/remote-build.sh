#!/usr/bin/env bash
set -eu
TASK=/root/llmbase-rust-migration-20260920/c6-public-projections-20261012
TOOLCHAIN=/root/llmbase-rust-migration-20260920
export CARGO_HOME=$TOOLCHAIN/cargo RUSTUP_HOME=$TOOLCHAIN/rustup
export PATH=$CARGO_HOME/bin:$TOOLCHAIN/python/bin:$PATH
export CARGO_TARGET_DIR=$TOOLCHAIN/source/target CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true
export PYO3_PYTHON=$TOOLCHAIN/python/bin/python
export PYTHONDONTWRITEBYTECODE=1 LLMBASE_REQUIRE_NATIVE=1
unset PYTHONPATH OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY
unset LLMBASE_API_SECRET LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED PORT
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
step() {
    local name=$1 code=0
    shift
    timeout 600 nice -n 10 "$@" > "$TASK/$name.log" 2>&1 || code=$?
    printf '%s\n' "$code" > "$TASK/$name.exit"
    if [ "$code" -ne 0 ]; then
        printf '%s\n' "$code" > "$TASK/build-runner.exit"
        exit "$code"
    fi
}
cd "$TASK"
if [ "${1:-}" != "--resume-tests" ]; then
step source-before python primary/verify_source.py
step build-venv python -m venv --without-pip build
step build-tools python -m pip --python build/bin/python install --disable-pip-version-check pip 'setuptools>=68' wheel 'setuptools-rust>=1.12,<2'
step wheel build/bin/python -m pip wheel --no-deps --no-build-isolation --wheel-dir wheels engine/
step candidate-venv python -m venv --without-pip candidate
step candidate-install python -m pip --python candidate/bin/python install --disable-pip-version-check -c "$TOOLCHAIN/b16-native-config/runtime-constraints.txt" wheels/*.whl pytest pytest-timeout mcp pymupdf opencc-python-reimplemented
fi
step harness mkdir -p test-harness
step harness-copy rsync -a --exclude=__pycache__ engine/tests/ test-harness/tests/
cd "$TASK/test-harness"
step focused "$TASK/candidate/bin/python" -m pytest --import-mode=importlib "$TASK/engine/tests/rust_migration/test_http_public_projections.py" "$TASK/engine/tests/rust_migration/test_http_detail_visibility.py" "$TASK/engine/tests/rust_migration/test_http_auth.py" "$TASK/engine/tests/test_web_articles.py" -q --timeout=60
step full-engine "$TASK/candidate/bin/python" -m pytest --import-mode=importlib "$TASK/engine/tests" -q --timeout=60
cd "$TASK/siwen"
step siwen-get "$TASK/candidate/bin/python" -m pytest tests/test_visibility.py tests/test_visibility_exports.py tests/test_visibility_aggregates.py tests/test_visibility_index_web.py tests/test_visibility_lite_compatibility.py -q --timeout=60
# Observe the unchanged complete downstream suite. Its known model/write auth
# migration assertions are reported separately; an exit 1 is never a pass.
code=0
timeout 600 nice -n 10 "$TASK/candidate/bin/python" -m pytest tests -q --timeout=60 --junitxml="$TASK/siwen-full.xml" > "$TASK/siwen-full.log" 2>&1 || code=$?
printf '%s\n' "$code" > "$TASK/siwen-full.exit"
if [ "$code" -gt 1 ]; then exit "$code"; fi
cd "$TASK"
step source-after python primary/verify_source.py
printf '0\n' > "$TASK/build-runner.exit"
