#!/usr/bin/env bash
set -u
cd /root/llmbase-rust-migration-20260920/c1-dual-identity-20261010
PYTHONDONTWRITEBYTECODE=1 timeout 180 /root/llmbase-rust-migration-20260920/b16-native-config/candidate-v2/bin/python primary/run_probe.py > runner.log 2>&1
code=$?
printf '%s\n' "$code" > runner.exit
exit "$code"
