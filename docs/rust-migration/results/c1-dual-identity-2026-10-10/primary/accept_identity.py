"""Independent installed-candidate identity check; synthetic libraries only."""
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import tempfile

from llmwiki import _native, atomic
from llmwiki import compile as compiler
from llmwiki.config import ensure_dirs, load_config

root = Path(__file__).resolve().parents[1]
receipt = {'sites':{}, 'wrong_identity':{}}
previous = os.environ.get('LLMBASE_LIBRARY_ID')
try:
    with tempfile.TemporaryDirectory(prefix='identity-', dir=root) as tmp:
        for name in ('siwen', 'taixu'):
            os.environ['LLMBASE_LIBRARY_ID'] = name
            base = Path(tmp) / name
            cfg = load_config(base)
            ensure_dirs(cfg)
            source = Path(cfg['paths']['raw']) / 'source.md'
            source.write_text('原文𠮷：' + name, encoding='utf-8')
            original = hashlib.sha256(source.read_bytes()).hexdigest()
            compiler.rebuild_index(base)
            rebuild_trace = _native.last_trace()
            assert rebuild_trace['core'] == 'rust'
            doctor = _native.dispatch('doctor', base_dir=base, library_id=name, paths=cfg['paths'])
            assert doctor['library_id'] == name
            target = Path(cfg['paths']['meta']) / 'identity-probe.txt'
            _native.with_writer(base, lambda: atomic.atomic_write_text(target, '已驗 ' + name),
                                library_id=name, paths=cfg['paths'])
            dbpath, = base.rglob('*.sqlite3')
            with sqlite3.connect(dbpath) as db:
                rows = db.execute("SELECT library_id, relative_path, status FROM write_journal WHERE relative_path LIKE '%identity-probe.txt'").fetchall()
            assert len(rows) == 1 and rows[0][0] == name and rows[0][2] == 'applied', rows
            assert hashlib.sha256(source.read_bytes()).hexdigest() == original
            receipt['sites'][name] = {'doctor':doctor, 'journal':rows, 'source_unchanged':True,
                                      'rebuild_trace':rebuild_trace}
            wrong = 'taixu' if name == 'siwen' else 'siwen'
            try:
                _native.dispatch('doctor', base_dir=base, library_id=wrong, paths=cfg['paths'])
            except PermissionError as exc:
                receipt['wrong_identity'][name] = {'rejected':True, 'error':str(exc)}
            else:
                raise AssertionError('wrong library ID unexpectedly opened the existing database')
finally:
    if previous is None:
        os.environ.pop('LLMBASE_LIBRARY_ID', None)
    else:
        os.environ['LLMBASE_LIBRARY_ID'] = previous
assert os.environ.get('LLMBASE_LIBRARY_ID') == previous
receipt['environment_restored'] = True
(root / 'identity-acceptance.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(receipt,ensure_ascii=False))
