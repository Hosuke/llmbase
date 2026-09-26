import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1]


def manifest(path):
    return {line.split(None, 1)[1].strip(): line.split(None, 1)[0]
            for line in path.read_text().splitlines()}


before = manifest(root / 'source-before-facade.sha256')
after = manifest(root / 'source-after.sha256')
assert before.keys() == after.keys()
changed = {name for name in before if before[name] != after[name]}
assert changed == {'llmwiki/taxonomy.py', 'tests/rust_migration/test_native_taxonomy_read.py'}, changed
for name, digest in after.items():
    assert hashlib.sha256((root / 'source' / name).read_bytes()).hexdigest() == digest, name
for step in ('fmt', 'clippy', 'core-taxonomy'):
    assert (root / (step + '.exit')).read_text().strip() == '0', step
receipt = {'passed': True, 'changed_inputs': sorted(changed),
           'unchanged_inputs': len(after) - len(changed),
           'source_before_sha256': hashlib.sha256((root / 'source-before-facade.sha256').read_bytes()).hexdigest(),
           'source_after_sha256': hashlib.sha256((root / 'source-after.sha256').read_bytes()).hexdigest()}
(root / 'native-reuse.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
