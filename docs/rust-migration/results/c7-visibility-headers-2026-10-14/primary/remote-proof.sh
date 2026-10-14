#!/usr/bin/env bash
set -eu
TASK=/root/llmbase-rust-migration-20260920/c7-visibility-headers-20261014
BASELINE=/root/llmbase-rust-migration-20260920/c6-public-projections-20261012/candidate/bin/python
export PYTHONDONTWRITEBYTECODE=1 LLMBASE_REQUIRE_NATIVE=1
unset PYTHONPATH OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY
unset LLMBASE_API_SECRET LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED PORT
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
step() {
    local name=$1 code=0
    shift
    timeout 300 nice -n 10 "$@" > "$TASK/$name.log" 2>&1 || code=$?
    printf '%s\n' "$code" > "$TASK/$name.exit"
    if [ "$code" -ne 0 ]; then
        printf '%s\n' "$code" > "$TASK/proof-runner.exit"
        exit "$code"
    fi
}
cd "$TASK"
if [ "${1:-}" != "--candidate-only" ]; then
    step corpus-copy "$BASELINE" primary/prepare_corpus.py
fi
step baseline-probe "$BASELINE" primary/probe.py --corpus "$TASK/corpus" --out "$TASK/baseline.json" --label 5cd2177
if [ "${1:-}" = "--baseline-only" ]; then exit 0; fi
step candidate-probe candidate/bin/python primary/probe.py --corpus "$TASK/corpus" --out "$TASK/candidate.json" --label c7
step corpus-growth candidate/bin/python primary/prepare_growth.py
step baseline-large "$BASELINE" primary/probe.py --corpus "$TASK/corpus-large" --out "$TASK/baseline-large.json" --label 5cd2177-synthetic-growth
step candidate-large candidate/bin/python primary/probe.py --corpus "$TASK/corpus-large" --out "$TASK/candidate-large.json" --label c7-synthetic-growth
step comparison candidate/bin/python primary/compare.py
printf '0\n' > "$TASK/proof-runner.exit"
