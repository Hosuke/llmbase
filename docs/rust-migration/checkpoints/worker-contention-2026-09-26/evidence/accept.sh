#!/bin/bash
set -u
TASK_ROOT=/root/llmbase-rust-migration-20260920
export CARGO_HOME="$TASK_ROOT/cargo"
export RUSTUP_HOME="$TASK_ROOT/rustup"
export PATH="$CARGO_HOME/bin:$TASK_ROOT/python/bin:$PATH"
export PYO3_PYTHON="$TASK_ROOT/python/bin/python"
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$TASK_ROOT/source/target"
cd "$TASK_ROOT/k1-accept" || exit 97
cp crates/llmbase-core/src/application.rs "$TASK_ROOT/k1-accept-fixed.rs" || exit 96
python3 - <<'PY'
from pathlib import Path
p = Path('crates/llmbase-core/src/application.rs')
s = p.read_text()
a = 'let mut library = open_operation_library(&config, &cancel, open_barrier)?;'
b = 'let mut library = { if let Some(barrier) = &open_barrier { barrier(); } Library::open(config)? };'
assert s.count(a) == 1
p.write_text(s.replace(a, b))
PY
cargo test -p llmbase-core --test worker claimed_repair_retries_transient_recovery_lock_contention_under_same_lease > "$TASK_ROOT/logs/k1-accept-before.log" 2>&1
echo $? > "$TASK_ROOT/logs/k1-accept-before.exit"
cp "$TASK_ROOT/k1-accept-fixed.rs" crates/llmbase-core/src/application.rs || exit 95
cargo test -p llmbase-core --test worker > "$TASK_ROOT/logs/k1-accept-worker.log" 2>&1
echo $? > "$TASK_ROOT/logs/k1-accept-worker.exit"
cargo test -p llmbase-cli --test http_model_contracts > "$TASK_ROOT/logs/k1-accept-http.log" 2>&1
echo $? > "$TASK_ROOT/logs/k1-accept-http.exit"
echo done > "$TASK_ROOT/logs/k1-accept.done"
