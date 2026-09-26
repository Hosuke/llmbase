"""Retain B14 native checks only for byte-identical inputs, with real provenance."""
import hashlib
import json
from pathlib import Path

task = Path(__file__).resolve().parents[1]
prior = task.parent / 'b14-heads'
manifest = {line.split('  ', 1)[1]: line.split('  ', 1)[0]
            for line in (task / 'source-before.sha256').read_text().splitlines()}
assert (task / 'source-before.sha256').read_bytes() == (prior / 'source-after.sha256').read_bytes()
changed = []
for name, expected in manifest.items():
    assert hashlib.sha256((prior / 'source' / name).read_bytes()).hexdigest() == expected
    if hashlib.sha256((task / 'source' / name).read_bytes()).hexdigest() != expected:
        changed.append(name)
assert changed == ['llmwiki/config.py'], changed
for phase in ['fmt', 'clippy', 'core-heads', 'runner']:
    assert (prior / (phase + '.exit')).read_text().strip() == '0'
provenance = json.loads((prior / 'candidate-provenance.json').read_text())
assert hashlib.sha256(Path(provenance['extension']).read_bytes()).hexdigest() == provenance['extension_sha256']
receipt = {'passed': True, 'unchanged_previous_inputs': len(manifest) - 1,
           'changed_previous_inputs': changed,
           'reused_native_checks': ['fmt', 'clippy', 'core-heads'],
           'previous_extension_sha256': provenance['extension_sha256']}
(task / 'primary/native-reuse-result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
