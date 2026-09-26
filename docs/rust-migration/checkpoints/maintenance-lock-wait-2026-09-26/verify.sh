#!/usr/bin/env bash
set -u
task_root=/root/llmbase-rust-migration-20260920
run_root="$task_root/b7-primary-fix"
mkdir -p "$run_root/logs"
exec >"$run_root/logs/runner.log" 2>&1
trap 'code=$?; printf "%s\n" "$code" > "$run_root/logs/runner.exit"' EXIT
export CARGO_HOME="$task_root/cargo" RUSTUP_HOME="$task_root/rustup"
export PATH="$CARGO_HOME/bin:$task_root/python/bin:$PATH"
export PYO3_PYTHON="$task_root/python/bin/python"
export CARGO_TARGET_DIR="$task_root/source/target" CARGO_BUILD_JOBS=2
cd "$run_root/source" || exit 97
step() {
  name=$1; shift
  "$@" > "$run_root/logs/$name.log" 2>&1
  code=$?
  printf '%s\n' "$code" > "$run_root/logs/$name.exit"
  echo "$name exit=$code"
  return "$code"
}
step before-manifest sha256sum -c "$run_root/before.sha256" || exit 98
step before-tests nice -n10 cargo test -p llmbase-core --locked --test maintenance_repair
cp "$run_root/maintenance_repair.fixed.rs" crates/llmbase-core/src/maintenance_repair.rs
step fixed-manifest sha256sum -c "$run_root/fixed.sha256" || exit 98
step repair nice -n10 cargo test -p llmbase-core --locked --test maintenance_repair || exit 1
step worker nice -n10 cargo test -p llmbase-core --locked --test worker || exit 1
for attempt in 1 2 3; do
  step "http-$attempt" nice -n10 cargo test -p llmbase-cli --locked --test http_model_contracts || exit 1
done
step fmt nice -n10 cargo fmt --all --check || exit 1
step clippy nice -n10 cargo clippy --workspace --all-targets --locked -- -D warnings || exit 1
step workspace nice -n10 cargo test --workspace --locked --no-fail-fast
workspace_code=$?
step post-manifest sha256sum -c "$run_root/fixed.sha256" || exit 98
exit "$workspace_code"
