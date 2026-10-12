import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / "primary/source-inputs.json").read_text())
mismatches = [name for name, expected in manifest.items()
              if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected]
if mismatches:
    raise SystemExit(f"source hash mismatch: {mismatches}")
print(f"Verified {len(manifest)} engine/Siwen source and test inputs")
