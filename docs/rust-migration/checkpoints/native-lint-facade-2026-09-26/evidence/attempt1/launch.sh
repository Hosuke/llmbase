#!/bin/bash
TASK=/root/llmbase-rust-migration-20260920/b11-native-lint
timeout --kill-after=15s 1500s bash "$TASK/primary/verify.sh" > "$TASK/runner.log" 2>&1
code=$?
echo "$code" > "$TASK/runner.exit"
exit "$code"
