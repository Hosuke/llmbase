#!/usr/bin/env bash
set -u
TASK=/root/llmbase-rust-migration-20260920/c3-worker-outcomes-20261010
TOOLCHAIN=/root/llmbase-rust-migration-20260920
export CARGO_HOME=$TOOLCHAIN/cargo RUSTUP_HOME=$TOOLCHAIN/rustup
export PATH=$CARGO_HOME/bin:$TOOLCHAIN/python/bin:$PATH
export CARGO_TARGET_DIR=$TOOLCHAIN/source/target CARGO_BUILD_JOBS=2
export PYO3_PYTHON=$TOOLCHAIN/python/bin/python
cd "$TASK/source" || exit 1
step() {
    local name=$1
    shift
    timeout 300 nice -n 10 "$@" > "$TASK/$name.log" 2>&1
    local code=$?
    printf '%s\n' "$code" > "$TASK/$name.exit"
    if [ "$code" -ne 0 ]; then
        printf '%s\n' "$code" > "$TASK/runner.exit"
        exit "$code"
    fi
}
step source-before "$TOOLCHAIN/python/bin/python" "$TASK/primary/verify_source.py"
step fmt cargo fmt --all -- --check
step contracts cargo test --offline --locked -p llmbase-core --lib --test worker --test compat_workflows --test compile --test plugins --test maintenance_repair --test maintenance_model --test entities_model --test answers
step clippy cargo clippy --offline --locked -p llmbase-core --all-targets -- -D warnings
step transport cargo test --offline --locked -p llmbase-cli --test workflows
step source-after "$TOOLCHAIN/python/bin/python" "$TASK/primary/verify_source.py"
printf '0\n' > "$TASK/runner.exit"
