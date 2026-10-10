import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import traceback

root = Path(__file__).resolve().parents[1]
for key in list(os.environ):
    if key.endswith(('API_KEY', 'API_SECRET')) or key.startswith('LLMBASE_') or key in {'LLM_MODEL', 'LLM_BASE_URL', 'OPENAI_BASE_URL'}:
        os.environ.pop(key, None)
os.environ['LLMBASE_REQUIRE_NATIVE'] = '1'
os.environ['PYTHONDONTWRITEBYTECODE'] = '1'
runpy.run_path(str(root / 'primary/verify_candidate.py'), run_name='__main__')
identity = json.loads((root / 'candidate-identity.json').read_text())
manifest = json.loads((root / 'primary/downstream-before.json').read_text())
sys.argv = ['prove_dual_site_candidate.py', '--snapshot', str(root / 'siwen-snapshot'),
            '--run-root', str(root / 'proof'), '--wheel', identity['wheel']]
report = {'candidate': identity, 'status':'failed', 'receipts':{}, 'exception':None,
          'boundary':'Sequential diagnostic on synthetic libraries; not final dual-site acceptance.'}
code = 1
try:
    runpy.run_path(str(root / 'primary/prove_dual_site_candidate.py'), run_name='__main__')
    report.update(status='passed', proof=json.loads((root / 'proof/receipt.json').read_text()))
    code = 0
except BaseException as exc:
    report['exception'] = {'type':type(exc).__name__, 'message':str(exc)}
    tb = exc.__traceback__
    while tb:
        if tb.tb_frame.f_code.co_name == 'main':
            report['receipts'] = tb.tb_frame.f_locals.get('receipts', {})
        tb = tb.tb_next
    traceback.print_exc()
finally:
    changed = [name for name, expected in manifest['files'].items()
               if hashlib.sha256((root / 'siwen-snapshot' / name).read_bytes()).hexdigest() != expected]
    report['downstream_snapshot_unchanged'] = not changed
    report['changed_snapshot_files'] = changed
    if changed:
        code = 1
        report['status'] = 'failed'
    (root / 'diagnostic.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({'status':report['status'], 'exception':report['exception'],
                      'snapshot_unchanged':not changed},ensure_ascii=False))
sys.exit(code)
