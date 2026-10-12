import hashlib
import json
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
probe = """
import json, pathlib, sys, llmwiki
from llmwiki import _native
backend = _native.backend_info()
prefix = pathlib.Path(sys.prefix).resolve()
for filename in (llmwiki.__file__, backend['extension_path']):
    if not pathlib.Path(filename).resolve().is_relative_to(prefix):
        raise SystemExit('not using the isolated installed wheel')
if not backend.get('native') or backend.get('core') != 'rust':
    raise SystemExit('Rust core is required')
extension = pathlib.Path(backend['extension_path'])
import hashlib
print(json.dumps({'python':sys.version,'package':llmwiki.__file__,'backend':backend,
                  'extension_sha256':hashlib.sha256(extension.read_bytes()).hexdigest()}))
"""
receipt = {}
for label in ("baseline", "candidate"):
    data = json.loads(subprocess.check_output([root / label / "bin/python", "-c", probe], cwd=root, text=True))
    wheel, = (root / f"{label}-wheels").glob("*.whl")
    data.update(wheel=str(wheel), wheel_sha256=hashlib.sha256(wheel.read_bytes()).hexdigest())
    receipt[label] = data
(root / "provenance.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt, indent=2))
