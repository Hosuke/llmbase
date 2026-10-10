#!/usr/bin/env bash
set -u
TASK=/root/llmbase-rust-migration-20260920/c2-privacy-evidence-20261010
TOOLCHAIN=/root/llmbase-rust-migration-20260920
cd "$TASK" || exit 1
export PYTHONDONTWRITEBYTECODE=1
timeout 180 "$TOOLCHAIN/b16-native-config/candidate-v2/bin/python" primary/run_probe.py > diagnostic.log 2>&1
probe_code=$?
printf '%s\n' "$probe_code" > diagnostic.exit
export CARGO_HOME=$TOOLCHAIN/cargo RUSTUP_HOME=$TOOLCHAIN/rustup
export PATH=$CARGO_HOME/bin:$TOOLCHAIN/python/bin:$PATH
export CARGO_TARGET_DIR=$TOOLCHAIN/source/target CARGO_BUILD_JOBS=2
cd "$TOOLCHAIN/b16-native-config/source" || exit 1
timeout 300 nice -n 10 cargo test --offline --locked -p llmbase-core --test library --test web_read > "$TASK/native-tests.log" 2>&1
test_code=$?
printf '%s\n' "$test_code" > "$TASK/native-tests.exit"
if [ "$test_code" -ne 0 ]; then
    printf '%s\n' "$test_code" > "$TASK/runner.exit"
    exit "$test_code"
fi
printf '%s\n' "$probe_code" > "$TASK/runner.exit"
exit "$probe_code"
