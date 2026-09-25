#!/bin/bash
set -u
TASK_ROOT=/root/llmbase-rust-migration-20260920
export CARGO_HOME="$TASK_ROOT/cargo"
export RUSTUP_HOME="$TASK_ROOT/rustup"
export PATH="$CARGO_HOME/bin:$TASK_ROOT/python/bin:$PATH"
export PYO3_PYTHON="$TASK_ROOT/python/bin/python"
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$TASK_ROOT/source/target"
cd "$TASK_ROOT/b2-privacy-source" || exit 97
python3 - <<'PY'
import hashlib,json
from pathlib import Path
files=sorted([*Path('crates').rglob('*'),Path('Cargo.toml'),Path('Cargo.lock'),Path('rust-toolchain.toml')])
rows={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file()}
Path('../logs/b2-accept-source-sha256.json').write_text(json.dumps(rows,indent=2)+'\n')
PY
nice -n 10 cargo test -p llmbase-core --test compat_query > "$TASK_ROOT/logs/b2-accept-query.log" 2>&1
echo $? > "$TASK_ROOT/logs/b2-accept-query.exit"
nice -n 10 cargo test -p llmbase-core --test compat_compile > "$TASK_ROOT/logs/b2-accept-compile.log" 2>&1
echo $? > "$TASK_ROOT/logs/b2-accept-compile.exit"
rustfmt --check --edition 2024 --config skip_children=true crates/llmbase-core/src/compat_query.rs crates/llmbase-core/src/compat_compile.rs crates/llmbase-core/tests/compat_query.rs crates/llmbase-core/tests/compat_compile.rs > "$TASK_ROOT/logs/b2-accept-fmt.log" 2>&1
echo $? > "$TASK_ROOT/logs/b2-accept-fmt.exit"
echo done > "$TASK_ROOT/logs/b2-accept.done"
