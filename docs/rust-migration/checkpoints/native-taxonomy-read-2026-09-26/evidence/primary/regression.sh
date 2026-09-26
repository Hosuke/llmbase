#!/bin/bash
TASK=/root/llmbase-rust-migration-20260920/b13-taxonomy-read
TOOLCHAIN=/root/llmbase-rust-migration-20260920
export CARGO_HOME=$TOOLCHAIN/cargo RUSTUP_HOME=$TOOLCHAIN/rustup
export PATH=$CARGO_HOME/bin:$TOOLCHAIN/python/bin:$PATH
export PYO3_PYTHON=$TOOLCHAIN/python/bin/python CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=$TOOLCHAIN/source/target
cd "$TASK/source" || exit 1
sha256sum -c "$TASK/source-k3.sha256" > "$TASK/regression-source.log" 2>&1 || exit 1
echo running > "$TASK/regression.exit"
timeout --kill-after=15s 180s nice -n 10 cargo test --locked -p llmbase-core --test compat_taxonomy > "$TASK/regression.log" 2>&1
code=$?
echo "$code" > "$TASK/regression.exit"
exit "$code"
