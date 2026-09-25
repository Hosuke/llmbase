#!/bin/bash
set -u
TASK_ROOT=/root/llmbase-rust-migration-20260920
export CARGO_HOME="$TASK_ROOT/cargo"
export RUSTUP_HOME="$TASK_ROOT/rustup"
export PATH="$CARGO_HOME/bin:$TASK_ROOT/python/bin:$PATH"
export PYO3_PYTHON="$TASK_ROOT/python/bin/python"
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$TASK_ROOT/source/target"
cd "$TASK_ROOT/b1-core-source" || exit 97
python3 - <<'PY'
import hashlib,json
from pathlib import Path
files=sorted([*Path('crates').rglob('*'),Path('Cargo.toml'),Path('Cargo.lock'),Path('rust-toolchain.toml')])
rows={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}
Path('../logs/b1-accept-source-sha256.json').write_text(json.dumps(rows,indent=2)+'\n')
PY
nice -n 10 cargo check -p llmbase-core --lib > "$TASK_ROOT/logs/b1-accept-check.log" 2>&1
echo $? > "$TASK_ROOT/logs/b1-accept-check.exit"
nice -n 10 cargo test -p llmbase-core --test document_yaml_contracts > "$TASK_ROOT/logs/b1-accept-contracts.log" 2>&1
echo $? > "$TASK_ROOT/logs/b1-accept-contracts.exit"
nice -n 10 cargo test -p llmbase-core --lib > "$TASK_ROOT/logs/b1-accept-lib.log" 2>&1
echo $? > "$TASK_ROOT/logs/b1-accept-lib.exit"
rustfmt --check --edition 2024 --config skip_children=true crates/llmbase-core/src/lib.rs crates/llmbase-core/src/compat_yaml.rs crates/llmbase-core/tests/document_yaml_contracts.rs > "$TASK_ROOT/logs/b1-accept-fmt.log" 2>&1
echo $? > "$TASK_ROOT/logs/b1-accept-fmt.exit"
echo done > "$TASK_ROOT/logs/b1-accept.done"
