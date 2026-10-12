import hashlib
import json
from pathlib import Path
import sys

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / f"primary/{sys.argv[1]}-inputs.json").read_text())
mismatches = [name for name, expected in manifest.items()
              if hashlib.sha256((root / "source" / name).read_bytes()).hexdigest() != expected]
if mismatches:
    raise SystemExit(f"source hash mismatch: {mismatches}")
print(f"Verified {len(manifest)} {sys.argv[1]} source/build/test inputs")
