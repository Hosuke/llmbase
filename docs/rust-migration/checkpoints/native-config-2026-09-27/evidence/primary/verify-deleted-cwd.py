"""Actual old/candidate reproducer, using temporary directories only."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile

from llmwiki import config

source = Path(__file__).parent / 'legacy_config.py'
spec = importlib.util.spec_from_file_location('llmwiki._legacy_config_oracle', source)
old = importlib.util.module_from_spec(spec)
spec.loader.exec_module(old)
results = {}
with tempfile.TemporaryDirectory() as temporary:
    root = Path(temporary).resolve()
    previous = Path.cwd()
    doomed = root / 'cwd'
    doomed.mkdir()
    os.chdir(doomed)
    doomed.rmdir()
    try:
        for name, module in [('old', old), ('candidate', config)]:
            try:
                cfg = module._read_config(root / 'library')
                assert cfg['base_dir'] == str(root / 'library')
                results[name] = {'success': True}
            except Exception as error:
                results[name] = {'success': False, 'error': type(error).__name__,
                                 'errno': getattr(error, 'errno', None)}
    finally:
        os.chdir(previous)
print(json.dumps(results))
assert results['old']['success']
if os.environ.get('B16_EXPECT_REPAIRED') == '1':
    assert results['candidate']['success']
else:
    assert results['candidate'] == {'success': False, 'error': 'FileNotFoundError', 'errno': 2}
