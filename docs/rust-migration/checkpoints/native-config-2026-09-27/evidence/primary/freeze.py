import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[3]
task = root / '.rust-migration-runs/b16-native-config'
before = {}
for line in (task / 'source-before.sha256').read_text().splitlines():
    digest, path = line.split('  ', 1)
    before[path] = digest
owned = {
    'crates/llmbase-core/src/config.rs', 'crates/llmbase-core/src/lib.rs',
    'crates/llmbase-python/src/lib.rs', 'llmwiki/config.py', 'llmwiki/_native.py',
}
new = {
    'crates/llmbase-core/src/compat_config.rs', 'crates/llmbase-core/tests/compat_config.rs',
    'crates/llmbase-python/src/config.rs', 'tests/rust_migration/test_native_config.py',
}
assert not (new & before.keys())
after = {path: hashlib.sha256((root / path).read_bytes()).hexdigest()
         for path in sorted(before.keys() | new)}
changes = {path for path in before if before[path] != after[path]}
assert changes == owned, changes
for path in owned:
    assert hashlib.sha256((task / 'before' / path).read_bytes()).hexdigest() == before[path]
(task / 'source-after.sha256').write_text(''.join(f'{digest}  {path}\n' for path, digest in after.items()))
(task / 'primary/source-files.txt').write_text(''.join(f'{path}\n' for path in after))
report = {'source_files': len(after), 'changed': sorted(changes), 'new': sorted(new),
          'unchanged_predecessor_inputs': len(before) - len(changes)}
(task / 'primary/source-delta.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
