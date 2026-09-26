"""Before-fix proof of the real Siwen lint callback signature incompatibility."""
import ast
import hashlib
import json
from pathlib import Path
import sys
import tempfile

repo = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(repo))
from llmwiki.lint import checks
assert Path(checks.__file__).resolve() == repo / 'llmwiki/lint/checks.py'

source_path = Path('/Users/lanchair/Developer/Buddha/Github/siwen/siwen/patches.py')
source = source_path.read_text()
tree = ast.parse(source)
node = next(n for n in tree.body if isinstance(n, ast.FunctionDef)
            and n.name == '_check_stubs_siwen')
namespace = {'_Path': Path}
# Extract only the unchanged function; do not execute the rest of patches.py.
exec(compile(ast.Module(body=[node], type_ignores=[]), str(source_path), 'exec'), namespace)
callback = namespace['_check_stubs_siwen']
original = checks.check_stubs
calls = []

def observe(frame, event, arg):
    if event == 'call' and frame.f_code is callback.__code__:
        calls.append(True)

with tempfile.TemporaryDirectory(prefix='llmbase-b11-before-') as temporary:
    base = Path(temporary)
    checks.check_stubs = callback
    previous_profile = sys.getprofile()
    sys.setprofile(observe)
    try:
        try:
            checks.lint(base)
        except TypeError as error:
            assert "unexpected keyword argument 'articles'" in str(error), str(error)
            assert not calls, 'callback body must not have been entered'
            print(json.dumps({
                'status': 'confirmed_before_fix',
                'failure': str(error),
                'callback_body_calls': len(calls),
                'source': str(source_path),
                'source_sha256': hashlib.sha256(source_path.read_bytes()).hexdigest(),
                'lint_module': checks.__file__,
                'lint_source_sha256': hashlib.sha256(Path(checks.__file__).read_bytes()).hexdigest(),
                'callback_source': ast.get_source_segment(source, node),
                'scope': 'isolated real callback function; not full Siwen integration',
            }, ensure_ascii=False, indent=2))
        else:
            raise AssertionError('expected signature mismatch did not occur')
    finally:
        sys.setprofile(previous_profile)
        checks.check_stubs = original
