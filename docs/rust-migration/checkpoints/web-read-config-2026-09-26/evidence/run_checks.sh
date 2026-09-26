#!/bin/bash
set -u
export CARGO_HOME=/root/llmbase-rust-migration-20260920/cargo
export RUSTUP_HOME=/root/llmbase-rust-migration-20260920/rustup
export PATH=$CARGO_HOME/bin:/root/llmbase-rust-migration-20260920/python/bin:$PATH
export PYO3_PYTHON=/root/llmbase-rust-migration-20260920/python/bin/python
export CARGO_TARGET_DIR=/root/llmbase-rust-migration-20260920/source/target
export CARGO_BUILD_JOBS=2
R=/root/llmbase-rust-migration-20260920/b8-web-read-config
cd "$R/source" || exit 97

step() {
  local name="$1"; shift
  nice -n10 "$@" > "$R/$name.log" 2>&1
  echo $? > "$R/$name.exit"
}

# BEFORE: pristine snapshot (frozen failing test at web_read.rs:181)
step before-web_read cargo test -p llmbase-core --test web_read --locked

# overlay the repaired test (only owned file)
cp "$R/fixed/web_read.rs" crates/llmbase-core/tests/web_read.rs
sha256sum crates/llmbase-core/tests/web_read.rs > "$R/fixed-overlay.sha256"

step fmt cargo fmt --all -- --check
step fixed-web_read cargo test -p llmbase-core --test web_read --locked
step document-contract cargo test -p llmbase-core --test document_yaml_contracts --locked
step clippy cargo clippy --workspace --all-targets --locked -- -D warnings
step workspace-tests cargo test --workspace --locked --no-fail-fast

ok=0
for n in fmt fixed-web_read document-contract clippy workspace-tests; do
  [ "$(cat "$R/$n.exit")" = "0" ] || ok=1
done
if [ "$ok" = "0" ]; then
  step release-cli cargo build --release -p llmbase-cli --locked
  if [ "$(cat "$R/release-cli.exit")" = "0" ]; then
    cp "$CARGO_TARGET_DIR/release/llmbase" "$R/llmbase-cli-bin"
    sha256sum "$R/llmbase-cli-bin" > "$R/llmbase-cli-bin.sha256"
    "$R/llmbase-cli-bin" --version > "$R/llmbase-cli-bin.version" 2>&1
  fi
else
  echo 99 > "$R/release-cli.exit"
  echo "skipped: one or more checks failed" > "$R/release-cli.log"
fi
echo DONE > "$R/done"
