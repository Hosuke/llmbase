import hashlib
import json
from pathlib import Path
import llmwiki
from llmwiki import _core

root = Path(__file__).resolve().parents[1]
prior = root.parent / 'b16-native-config'
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
runtime = json.loads((root / 'primary/engine-runtime.json').read_text())
mismatches = [name for name, expected in runtime.items()
              if not (prior / 'source' / name).is_file()
              or digest(prior / 'source' / name) != expected]
assert not mismatches, mismatches
record = json.loads((prior / 'candidate-provenance.json').read_text())
package = Path(llmwiki.__file__).parent
assert package.is_relative_to(prior / 'candidate-v2')
assert digest(Path(_core.__file__)) == record['extension_sha256']
assert all(digest(package / name) == expected for name, expected in record['python_files'].items())
wheel, = (prior / 'wheels-v2').glob('*.whl')
assert digest(wheel) == record['wheel_sha256']
downstream = json.loads((root / 'primary/downstream-before.json').read_text())
snapshot = root / 'siwen-snapshot'
assert all(digest(snapshot / name) == expected for name, expected in downstream['files'].items())
receipt = {'engine_commit':downstream['engine_commit'], 'matching_runtime_inputs':len(runtime),
           'matching_installed_python_files':len(record['python_files']),
           'extension_sha256':record['extension_sha256'], 'wheel_sha256':record['wheel_sha256'],
           'wheel':str(wheel), 'downstream_head':downstream['head'],
           'downstream_snapshot_files':len(downstream['files']),
           'downstream_has_uncommitted_changes':bool(downstream['git_status']),
           'assets':'historical B5 static assets copied within remote runner; not a current frontend build',
           'runtime_reused':'B16 candidate-v2, justified by exact source and installed-file hashes'}
(root / 'candidate-identity.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(receipt,ensure_ascii=False))
