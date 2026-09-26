"""Rerun the actual-old/B13 regression matrix against the repaired candidate."""
import hashlib
import json
from pathlib import Path
import runpy

root = Path(__file__).resolve().parent
expected = {
    'legacy_config.py': '5fb59b498dd049f80e4da476a94053c6bb850d807f2c55587cf0a7c6431359ce',
    'legacy_atomic.py': '3aef32211d48841d867d6afca1472ca21bcf046203f13ef64399697ecb116067',
}
for name, sha in expected.items():
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == sha
runpy.run_path(str(root / 'reproduce-relative-root.py'), run_name='__main__')
receipt = json.loads((root / 'relative-root-result.json').read_text())
assert len(receipt['results']) == 4
assert all(row['ok'] and row['exact_bytes'] for row in receipt['results'])
assert all(row['concepts_absolute'] for row in receipt['results'] if row['implementation'] == 'candidate')
receipt['passed'] = True
(root / 'original-comparison-result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('Original/candidate relative/absolute write matrix: 4 exact-byte successes')
