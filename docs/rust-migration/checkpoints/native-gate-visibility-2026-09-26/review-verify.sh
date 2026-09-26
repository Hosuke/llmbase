#!/usr/bin/env bash
set -eu
task_root=/root/llmbase-rust-migration-20260920
run_root="$task_root/b6-primary"
exec >"$run_root/logs/review-verify.log" 2>&1
trap 'code=$?; printf "%s\n" "$code" > "$run_root/logs/review-verify.exit"' EXIT
export CARGO_HOME="$task_root/cargo"
export RUSTUP_HOME="$task_root/rustup"
export PATH="$CARGO_HOME/bin:$task_root/python/bin:$PATH"
export PYO3_PYTHON="$task_root/python/bin/python"
export CARGO_TARGET_DIR="$task_root/source/target"
export CARGO_BUILD_JOBS=2
cd "$task_root/b6-core-review/source"
sha256sum -c "$run_root/review-current.sha256"
nice -n10 cargo test -p llmbase-core --locked --test compat_maintenance
nice -n10 cargo test -p llmbase-core --locked --lib
sha256sum -c "$run_root/review-current.sha256"
