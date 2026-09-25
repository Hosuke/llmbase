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
python3 - <<'PY'
import hashlib, json
from pathlib import Path
files = sorted([*Path('crates').rglob('*'), Path('Cargo.toml'), Path('Cargo.lock'), Path('rust-toolchain.toml')])
rows = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}
Path('../logs/k1-final-source-sha256.json').write_text(json.dumps(rows, indent=2)+'\n')
PY
cargo test -p llmbase-core --test worker > "$TASK_ROOT/logs/k1-final-worker.log" 2>&1
echo $? > "$TASK_ROOT/logs/k1-final-worker.exit"
cargo test -p llmbase-cli --test http_model_contracts > "$TASK_ROOT/logs/k1-final-http.log" 2>&1
echo $? > "$TASK_ROOT/logs/k1-final-http.exit"
rustfmt --check --edition 2024 crates/llmbase-core/src/application.rs crates/llmbase-core/tests/worker.rs crates/llmbase-cli/tests/http_model_contracts.rs > "$TASK_ROOT/logs/k1-final-fmt.log" 2>&1
echo $? > "$TASK_ROOT/logs/k1-final-fmt.exit"
echo done > "$TASK_ROOT/logs/k1-final.done"
