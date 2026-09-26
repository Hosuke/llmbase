"""Independent fixed-seed comparisons against the saved original normalizer."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import random
import re
import sys

from llmwiki import _core, _native, normalize

root = Path(__file__).resolve().parent
source = root / 'legacy_normalize.py'
assert hashlib.sha256(source.read_bytes()).hexdigest() == 'b0411b409377b6c19fc0ecca29b07dd8481fe74b859f066029f00a92cd25c4e1'
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
assert Path(normalize.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
spec = importlib.util.spec_from_file_location('llmwiki._legacy_heads_oracle', source)
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)
rng = random.Random(184723)
before_roots = dict(_native._configured_roots)
digest = hashlib.sha256()
count = 0


def compare(body, rules):
    global count
    old_rules = copy.deepcopy(rules)
    expected = legacy.normalize_heads(body, old_rules)
    actual = normalize.normalize_heads(body, rules)
    assert actual == expected, repr((body, rules, expected, actual))
    assert rules == old_rules, 'caller rules mutated'
    trace = _native.last_trace()
    assert trace['core'] == 'rust' and trace['operation'] == 'utility_normalize_heads', trace
    assert trace['matcher'] == ('python_regex' if rules else 'none'), trace
    digest.update(repr((body, rules, expected)).encode())
    count += 1


rulesets = [[], [{'pattern': '', 'level': 2}],
            [{'pattern': r'^第.*章', 'level': 2}, {'pattern': r'甲', 'level': 5}],
            [{'pattern': r'(?<=第)一章', 'level': 3}],
            [{'pattern': r'^(?P<w>word)\s+(?P=w)$', 'level': 6}],
            [{'pattern': re.compile(r'^CHAPTER$', re.I | re.ASCII), 'level': 4}],
            [{'pattern': r'\b\w+\b', 'level': 1}],
            [{'pattern': r'\s', 'level': 5}, {'pattern': r'.', 'level': 6}]]
lines = ['# 第一章', '   #### 第三章  ##  ', '#  ', '# ', '##\t\t',
         '##\x1f第一章\x1f### ', '#### 𠮷é  ##', '# chapter', '# word word',
         '# 甲、', '####### no heading', '    # indented', '\t# tab', '正文',
         '```', '```python', '````', '~~~', '~~~~x', '   ``` ',
         '<div>', '</div>', '<ſCRIPT>', '</ſCRIPT>', '<span>', '<!--', '', '  ']
for _ in range(500):
    nl = rng.choice(['\n', '\r\n', '\r', '\u2028', '\x85'])
    body = nl.join(rng.choices(lines, k=rng.randrange(24)))
    body += rng.choice(['', '\n', '\n\n', '\r\n', '\r\n\n', '\r'])
    compare(body, copy.deepcopy(rng.choice(rulesets)))

for indent in ['', ' ', '   ', '    ', '\t']:
    for suffix in ['', ' ', '  ', '\t', '\x1f', ' ### ', '###', ' ##\t']:
        compare(indent + '### ' + suffix + '\n', [{'pattern': '', 'level': 2}])
for level in [True, 1, 6, 2.0]:
    rules = [{'pattern': 'absent', 'level': level}]
    compare('# present\n', rules)
    if type(level) is not float:
        compare('# absent\n', rules)

errors = []
for rules in [[{'pattern': 'present', 'level': 2.0}],
              [{'pattern': 'x', 'level': 0}], [{'pattern': 'x', 'level': 7}],
              [{'pattern': '[', 'level': 2}], [{'pattern': '[', 'level': 0}]]:
    for body in ['', '# present\n']:
        old_error = None
        try:
            legacy.normalize_heads(body, rules)
        except Exception as exc:
            old_error = type(exc)
        if old_error is None:
            continue
        _native.utility_transform('sha1', {'text': 'seed success'})
        try:
            normalize.normalize_heads(body, rules)
        except Exception as exc:
            assert type(exc) is old_error, (rules, old_error, type(exc))
            assert _native.last_trace() is None, 'stale trace after validation/matcher failure'
            errors.append(old_error.__name__)
        else:
            raise AssertionError((rules, 'legacy failure became success'))

assert _native._configured_roots == before_roots
receipt = {'passed': True, 'compared_cases': count, 'error_cases': len(errors),
           'seed': 184723, 'corpus_sha256': digest.hexdigest(),
           'extension_sha256': hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
           'no_library_registration': True}
(root / 'heldout-result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
