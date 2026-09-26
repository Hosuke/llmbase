#!/bin/bash
# B10 finite copy-first migration + legacy reader proof runner.
set -u
ROOT=/root/llmbase-rust-migration-20260920/b10-migration-old-reader
PY=/root/llmbase-rust-migration-20260920/b5-registration-fix/candidate/bin/python
BIN=$ROOT/llmbase-cli-bin
EXPECT=f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8
AEXPECT=fda78e50bcb93aa0992aeabe9cd45ae1e39b9600fa24b8814ea240e792e1e222
ATTEMPT=$(date +%Y%m%d-%H%M%S)
LOG=$ROOT/verify-$ATTEMPT.log

fail() { echo "$1"; echo 1 > "$ROOT/verify-$ATTEMPT.exit"; exit 1; }

{
actual=$(sha256sum "$BIN" | cut -d' ' -f1)
[ "$actual" = "$EXPECT" ] || fail "binary hash mismatch: $actual"
echo "binary sha256: $actual (pinned B8 release CLI)"
aarch=$(sha256sum "$ROOT/legacy-a7c4d9d.tar.gz" | cut -d' ' -f1)
[ "$aarch" = "$AEXPECT" ] || fail "legacy archive hash mismatch: $aarch"
echo "legacy archive sha256: $aarch (baseline a7c4d9da764462e09c0769ecd8ba548a46d27e83)"
cd "$ROOT/inputs" || fail "inputs missing"
sha256sum -c "$ROOT/inputs.sha256" || fail "inputs hash failure"
sha256sum "$ROOT/verify-migration.py"

SRC=$ROOT/legacy-source
[ -e "$SRC" ] && fail "legacy-source already exists; refusing to reuse"
mkdir "$SRC"
tar -xzf "$ROOT/legacy-a7c4d9d.tar.gz" -C "$SRC" || fail "archive extraction failed"
"$PY" - <<'EOF' || fail "legacy source hash verification failed"
import hashlib, json, sys
from pathlib import Path
root = Path("/root/llmbase-rust-migration-20260920/b10-migration-old-reader/legacy-source")
manifest = json.loads(Path("/root/llmbase-rust-migration-20260920/b10-migration-old-reader/legacy-source-sha256.json").read_text())
bad = [rel for rel, expect in sorted(manifest.items())
       if not (root / rel).is_file()
       or hashlib.sha256((root / rel).read_bytes()).hexdigest() != expect]
extra = [str(p.relative_to(root)) for p in root.rglob("*")
         if p.is_file() and str(p.relative_to(root)) not in manifest]
if bad or extra:
    print("MISMATCH", bad, extra)
    sys.exit(1)
print(f"legacy source verified: {len(manifest)} files, no extras")
EOF

RUN=$ROOT/run-$ATTEMPT
echo "=== verify-migration run-$ATTEMPT ==="
nice -n10 "$PY" "$ROOT/verify-migration.py" --binary "$BIN" --run-root "$RUN" \
    --python "$PY" --legacy-source "$SRC" \
    --legacy-hashes "$ROOT/legacy-source-sha256.json" \
    --fixtures "$ROOT/inputs/tests/rust_migration/fixtures/downstream/library"
code=$?
echo "$code" > "$ROOT/verify-$ATTEMPT.exit"
echo "verify exit: $code"
exit "$code"
} > "$LOG" 2>&1
