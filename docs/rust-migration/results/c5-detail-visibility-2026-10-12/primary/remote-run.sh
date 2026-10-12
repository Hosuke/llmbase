#!/usr/bin/env bash
set -eu
TASK=/root/llmbase-rust-migration-20260920/c5-detail-visibility-20261012
TOOLCHAIN=/root/llmbase-rust-migration-20260920
export CARGO_HOME=$TOOLCHAIN/cargo RUSTUP_HOME=$TOOLCHAIN/rustup
export PATH=$CARGO_HOME/bin:$TOOLCHAIN/python/bin:$PATH
export CARGO_TARGET_DIR=$TOOLCHAIN/source/target CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true
export PYO3_PYTHON=$TOOLCHAIN/python/bin/python
unset PYTHONPATH OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
step() {
    local name=$1
    shift
    local code=0
    timeout 600 nice -n 10 "$@" > "$TASK/$name.log" 2>&1 || code=$?
    printf '%s\n' "$code" > "$TASK/$name.exit"
    if [ "$code" -ne 0 ]; then
        printf '%s\n' "$code" > "$TASK/runner.exit"
        exit "$code"
    fi
}
cd "$TASK"
if [ "${1:-}" != "--resume-tests" ]; then
step baseline-source python primary/verify_source.py baseline
step build-venv python -m venv --without-pip build
step build-tools python -m pip --python build/bin/python install --disable-pip-version-check pip 'setuptools>=68' wheel 'setuptools-rust>=1.12,<2'
step baseline-wheel build/bin/python -m pip wheel --no-deps --no-build-isolation --wheel-dir baseline-wheels source/
step baseline-venv python -m venv --without-pip baseline
step baseline-install python -m pip --python baseline/bin/python install --disable-pip-version-check -c "$TOOLCHAIN/b16-native-config/runtime-constraints.txt" baseline-wheels/*.whl
step apply-patch rsync -a patch/ source/
step candidate-source python primary/verify_source.py candidate
step candidate-wheel build/bin/python -m pip wheel --no-deps --no-build-isolation --wheel-dir candidate-wheels source/
step candidate-venv python -m venv --without-pip candidate
step candidate-install python -m pip --python candidate/bin/python install --disable-pip-version-check -c "$TOOLCHAIN/b16-native-config/runtime-constraints.txt" candidate-wheels/*.whl pytest pytest-timeout mcp pymupdf opencc-python-reimplemented
fi
export LLMBASE_REQUIRE_NATIVE=1 PYTHONDONTWRITEBYTECODE=1
step installed-provenance candidate/bin/python primary/provenance.py
step harness-directory mkdir -p test-harness
step harness rsync -a --exclude=__pycache__ source/tests/ test-harness/tests/
cd "$TASK/test-harness"
step focused "$TASK/candidate/bin/python" -m pytest --import-mode=importlib "$TASK/source/tests/rust_migration/test_http_detail_visibility.py" "$TASK/source/tests/rust_migration/test_http_auth.py" "$TASK/source/tests/test_web_articles.py" -q --timeout=60
step full-python "$TASK/candidate/bin/python" -m pytest --import-mode=importlib "$TASK/source/tests" -q --timeout=60
cd "$TASK"
step prepare-fixtures candidate/bin/python source/scripts/rust-migration/prove_detail_request_scaling.py prepare --root "$TASK/fixtures"
step baseline-benchmark baseline/bin/python source/scripts/rust-migration/prove_detail_request_scaling.py probe --root "$TASK/fixtures" --label 90394d0 --out "$TASK/baseline.json"
step candidate-benchmark candidate/bin/python source/scripts/rust-migration/prove_detail_request_scaling.py probe --root "$TASK/fixtures" --label c5-working-source --out "$TASK/candidate.json"
step benchmark-compare candidate/bin/python source/scripts/rust-migration/prove_detail_request_scaling.py compare --baseline baseline.json --candidate candidate.json --out comparison.json
step source-after python primary/verify_source.py candidate
printf '0\n' > "$TASK/runner.exit"
