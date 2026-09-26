#!/bin/bash
set -u
ROOT=/root/llmbase-rust-migration-20260920/b10-primary
INPUT=/root/llmbase-rust-migration-20260920/b10-migration-old-reader
PY=/root/llmbase-rust-migration-20260920/b5-registration-fix/candidate/bin/python
cd "$ROOT" || exit 1
sha256sum verify-migration.py accepted-source.sha256 > inputs-actual.sha256
timeout --kill-after=10s 180s nice -n 10 "$PY" verify-migration.py \
  --binary "$INPUT/llmbase-cli-bin" --run-root "$ROOT/run-verified" \
  --python "$PY" --legacy-source "$INPUT/legacy-source" \
  --legacy-archive "$INPUT/legacy-a7c4d9d.tar.gz" \
  --legacy-hashes "$INPUT/legacy-source-sha256.json" \
  --source-manifest "$ROOT/accepted-source.sha256" \
  --fixtures "$INPUT/inputs/tests/rust_migration/fixtures/downstream/library" \
  > verify.log 2>&1
code=$?
echo "$code" > verify.exit
sha256sum verify-migration.py accepted-source.sha256 > inputs-after.sha256
exit "$code"
