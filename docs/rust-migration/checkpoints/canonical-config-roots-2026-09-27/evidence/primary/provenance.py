import hashlib
import json
from pathlib import Path
import sys
import tempfile

import llmwiki
from llmwiki import _core, _native, atomic, config

root = Path(__file__).resolve().parents[1]
prefix = Path(sys.prefix).resolve()
for module in (llmwiki, _core, _native, config):
    assert Path(module.__file__).resolve().is_relative_to(prefix), module.__file__
with tempfile.TemporaryDirectory(prefix='b15-provenance-') as tmp:
    cfg = config.load_config(Path(tmp))
    config.ensure_dirs(cfg)
    path = Path(cfg['paths']['concepts']) / 'provenance.md'
    atomic.atomic_write_text(path, '原文𠮷\r\n')
    trace = _native.last_trace()
    assert trace['core'] == 'rust' and trace['journal'] is True
    assert path.read_bytes() == '原文𠮷\r\n'.encode()
python_files = {}
for source in sorted((root / 'source/llmwiki').rglob('*.py')):
    relative = source.relative_to(root / 'source/llmwiki')
    installed = Path(llmwiki.__file__).parent / relative
    assert installed.read_bytes() == source.read_bytes(), str(relative)
    python_files[str(relative)] = hashlib.sha256(installed.read_bytes()).hexdigest()
wheels = list((root / 'wheels').glob('*.whl'))
assert len(wheels) == 1
receipt = {'package': llmwiki.__file__, 'extension': _core.__file__,
           'extension_sha256': hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
           'wheel_sha256': hashlib.sha256(wheels[0].read_bytes()).hexdigest(),
           'python_files': python_files, 'backend': _core.backend_info()}
(root / 'candidate-provenance.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps({'passed': True, 'python_files': len(python_files),
                  'wheel_sha256': receipt['wheel_sha256'], 'extension_sha256': receipt['extension_sha256']}))
