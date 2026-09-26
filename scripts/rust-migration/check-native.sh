#!/usr/bin/env bash
set -euo pipefail
# Run only in an isolated checkout/runner. No model credentials are needed.
unset OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export PYO3_PYTHON=${PYO3_PYTHON:-python3}
"$PYO3_PYTHON" -c 'import sys; assert sys.version_info >= (3, 11), "The optional Python bridge needs Python 3.11+; set PYO3_PYTHON to the test venv interpreter."'
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release -p llmbase-cli --locked
