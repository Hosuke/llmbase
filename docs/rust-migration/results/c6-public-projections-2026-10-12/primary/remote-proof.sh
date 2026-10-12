#!/usr/bin/env bash
set -eu
TASK=/root/llmbase-rust-migration-20260920/c6-public-projections-20261012
BASELINE=/root/llmbase-rust-migration-20260920/c5-detail-visibility-20261012/candidate
export PYTHONDONTWRITEBYTECODE=1 LLMBASE_REQUIRE_NATIVE=1
unset PYTHONPATH LLMBASE_API_SECRET LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED PORT
step() {
    local name=$1 code=0
    shift
    timeout 600 nice -n 10 "$@" > "$TASK/$name.log" 2>&1 || code=$?
    printf '%s\n' "$code" > "$TASK/$name.exit"
    if [ "$code" -ne 0 ]; then
        printf '%s\n' "$code" > "$TASK/proof-runner.exit"
        exit "$code"
    fi
}
cd "$TASK"
step provenance candidate/bin/python primary/provenance.py
step corpus-config candidate/bin/python primary/prepare_corpus.py
step baseline-probe "$BASELINE/bin/python" primary/prove_public_read_projections.py probe --root corpus --siwen siwen-baseline --out baseline.json --label baseline
step candidate-probe candidate/bin/python primary/prove_public_read_projections.py probe --root corpus --siwen siwen --out candidate.json --label candidate
step compare candidate/bin/python primary/prove_public_read_projections.py compare --baseline baseline.json --candidate candidate.json --out comparison.json
step proof-source-after candidate/bin/python primary/verify_source.py
printf '0\n' > "$TASK/proof-runner.exit"
