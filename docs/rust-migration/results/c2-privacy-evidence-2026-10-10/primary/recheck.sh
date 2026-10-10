#!/usr/bin/env bash
set -u
TASK=/root/llmbase-rust-migration-20260920/c2-privacy-evidence-20261010
cd "$TASK" || exit 1
export PYTHONDONTWRITEBYTECODE=1
timeout 180 /root/llmbase-rust-migration-20260920/b16-native-config/candidate-v2/bin/python primary/run_probe.py proof-v2 > diagnostic-v2.log 2>&1
code=$?
printf '%s\n' "$code" > diagnostic-v2.exit
exit "$code"
