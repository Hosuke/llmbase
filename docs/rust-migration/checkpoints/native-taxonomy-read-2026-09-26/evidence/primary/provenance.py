import hashlib
import json
from pathlib import Path
import sys

import llmwiki
from llmwiki import _core, _native, taxonomy

root = Path(__file__).resolve().parents[1]
prefix = Path(sys.prefix).resolve()
for module in (llmwiki, _core, _native, taxonomy):
    assert Path(module.__file__).resolve().is_relative_to(prefix), module.__file__
assert taxonomy._localize_title('English / 經', 'zh') == '經'
trace = _native.last_trace()
assert trace['core'] == 'rust' and trace['operation'] == 'taxonomy_transform'
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
