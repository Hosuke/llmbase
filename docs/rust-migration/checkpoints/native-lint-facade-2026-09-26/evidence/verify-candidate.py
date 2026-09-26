"""Compare a fresh installed native facade with frozen actual old lint output."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

parser = argparse.ArgumentParser()
parser.add_argument('--library', type=Path, required=True)
parser.add_argument('--baseline', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--siwen-before', type=Path, required=True)
args = parser.parse_args()
import llmwiki
from llmwiki import _core, _native
from llmwiki.lint import checks
prefix = Path(sys.prefix).resolve()
for module in (llmwiki, _core, checks):
    assert Path(module.__file__).resolve().is_relative_to(prefix), module.__file__
assert _native.available()
baseline = json.loads(args.baseline.read_text())
root = args.library.resolve()

def inventory():
    return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(root.rglob('*')) if p.is_file()}

assert inventory() == baseline['files_sha256']
results, traces = {}, {}
for allow in (False, True):
    key = str(allow).lower()
    checks.ALLOW_CJK_SLUGS = allow
    results[key] = checks.lint(root)
    traces[key] = _native.last_trace()
    assert traces[key]['core'] == 'rust'
    assert traces[key]['operation'] == 'maintenance_run'
    assert traces[key]['action'] == 'lint'
siwen_before = json.loads(args.siwen_before.read_text())
namespace = {'_Path': Path}
exec(compile(siwen_before['callback_source'], siwen_before['source'], 'exec'), namespace)
callback = namespace['_check_stubs_siwen']
from llmwiki.config import load_config
expected_stubs = callback(load_config(root))
calls = []
def observe(frame, event, arg):
    if event == 'call' and frame.f_code is callback.__code__:
        calls.append(True)
original_callback = checks.check_stubs
previous_profile = sys.getprofile()
checks.check_stubs = callback
try:
    sys.setprofile(observe)
    siwen_result = checks.lint(root)
finally:
    sys.setprofile(previous_profile)
    checks.check_stubs = original_callback
assert len(calls) == 1, 'actual Siwen callback must be invoked once'
expected_siwen = dict(baseline['results']['true'], stubs=expected_stubs)
expected_siwen['total_issues'] = sum(len(v) for k, v in expected_siwen.items() if k != 'total_issues')
assert siwen_result == expected_siwen
siwen_trace = _native.last_trace()
assert siwen_trace['operation'] == 'maintenance_run' and siwen_trace['core'] == 'rust'
actual_inventory = inventory()
passed = results == baseline['results'] and actual_inventory == baseline['files_sha256']
args.output.write_text(json.dumps({
    'passed': passed, 'results': results, 'expected_results': baseline['results'],
    'traces': traces, 'inventory_after': actual_inventory,
    'package': llmwiki.__file__, 'extension': _core.__file__,
    'extension_sha256': hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
    'siwen_callback': {'source_sha256': siwen_before['source_sha256'],
                       'calls': len(calls), 'result': siwen_result, 'trace': siwen_trace,
                       'scope': 'actual extracted callback, not full Siwen integration'},
}, ensure_ascii=False, indent=2) + '\n')
assert passed, f'candidate differs from frozen baseline; see {args.output}'
print('PASS: eight lint categories agree with old implementation; Rust traces and original bytes verified')
