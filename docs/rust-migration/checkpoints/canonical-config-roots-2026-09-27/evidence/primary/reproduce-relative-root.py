"""Bounded remote comparison: config defaults + atomic write with relative base."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile

from llmwiki import _core, _native, atomic, config

root = Path(__file__).resolve().parent
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())


def saved_module(name):
    path = root / ('legacy_' + name + '.py')
    spec = importlib.util.spec_from_file_location('old_' + name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


old_config, old_atomic = saved_module('config'), saved_module('atomic')
results = []
cwd = Path.cwd()
with tempfile.TemporaryDirectory(prefix='config-relative-root-') as tmp:
    try:
        os.chdir(tmp)
        for label, cfg_module, io_module in [('legacy', old_config, old_atomic), ('candidate', config, atomic)]:
            for mode in ('relative', 'absolute'):
                base = Path(label + '-' + mode)
                if mode == 'absolute':
                    base = Path(tmp) / base
                cfg = cfg_module.load_config(base)
                cfg_module.ensure_dirs(cfg)
                path = Path(cfg['paths']['concepts']) / 'a.md'
                record = {'implementation': label, 'base_mode': mode,
                          'concepts_absolute': Path(cfg['paths']['concepts']).is_absolute()}
                try:
                    io_module.atomic_write_text(path, '# 原文\n')
                except Exception as error:
                    record.update(ok=False, error_type=type(error).__name__, error=str(error))
                else:
                    record.update(ok=True, exact_bytes=path.read_bytes() == '# 原文\n'.encode())
                results.append(record)
    finally:
        os.chdir(cwd)
receipt = {'results': results,
           'extension_sha256': hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
           'candidate_config_sha256': hashlib.sha256(Path(config.__file__).read_bytes()).hexdigest(),
           'legacy_config_sha256': hashlib.sha256((root / 'legacy_config.py').read_bytes()).hexdigest(),
           'legacy_atomic_sha256': hashlib.sha256((root / 'legacy_atomic.py').read_bytes()).hexdigest()}
(root / 'relative-root-result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
