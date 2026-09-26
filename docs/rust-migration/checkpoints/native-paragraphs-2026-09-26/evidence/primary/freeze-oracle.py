"""Freeze actual legacy normalization outputs before native implementation."""
import hashlib
import importlib.util
import inspect
import json
from pathlib import Path
import random
import subprocess
import sys

ROOT = Path('/Users/lanchair/Developer/LLM-infra/Github/llmbase-rust')
LEGACY = Path('/Users/lanchair/Developer/LLM-infra/Github/llmbase')
sys.path.insert(0, str(LEGACY))
import pytest
from llmwiki import normalize as legacy

source = Path(legacy.__file__)
assert source.resolve() == LEGACY / 'llmwiki/normalize.py'
before = ROOT / '.rust-migration-runs/b12-paragraphs/before/llmwiki/normalize.py'
assert source.read_bytes() == before.read_bytes()
baseline = subprocess.check_output(['git', 'show', 'a7c4d9d:llmwiki/normalize.py'], cwd=LEGACY)
assert baseline == source.read_bytes()
original = legacy.normalize_paragraphs
cases = []
label = 'existing'

def capture(body):
    output = original(body)
    cases.append({'name': f'{label}-{len(cases):04}', 'body': body,
                  'terminators': legacy.SENTENCE_TERMINATORS,
                  'wrappers': legacy.CLOSING_WRAPPERS, 'expected': output})
    return output

legacy.normalize_paragraphs = capture
test_path = ROOT / 'tests/test_normalize.py'
spec = importlib.util.spec_from_file_location('legacy_paragraph_tests', test_path)
tests = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tests)
checked = []
for name, function in vars(tests).items():
    if not name.startswith('test_paragraphs_'):
        continue
    label = name
    with pytest.MonkeyPatch.context() as monkeypatch:
        if inspect.signature(function).parameters:
            function(monkeypatch)
        else:
            function()
    checked.append(name)

defaults = (legacy.SENTENCE_TERMINATORS, legacy.CLOSING_WRAPPERS)
adversarial = [
    '', '\n', '\r', '\r\n\n', '甲\r乙\r', '甲\x1f乙\n丙',
    '甲𠮷é\n  乙\n\n', '「甲。」\n乙', '。）』\n乙',
    '```\n甲\n乙\n~~~\n```\n甲\n乙',
    '````\n甲\n```\n乙\n````\n甲\n乙',
    '```\n甲\n`~`\n乙\n甲\n', '~~~\n甲\n~`~\n乙\n甲',
    '- x\n\n\n  y\n z\n\n甲\n乙',
    '- x\n```\na\n```\ncontinuation\n\n甲\n乙',
    '> x\n\n  y\nz\n\n甲\n乙',
    '<div>\n</div>\n甲\n乙\n\n丙\n丁',
    '<script>\n</script>\n甲\n乙\n\n丙\n丁',
    '<scrİpt>\n甲\n乙', '<ſtyle>\n甲\n乙',
    '<span>甲</span>\n乙', '<!-- x -->\n甲\n乙',
    '甲\n[字]: url\n乙\n丙', '##\n    code\nnext\nline',
    '####### title\n甲\n乙', '###\u3000title\n甲\n乙',
    '١. item\n甲\n乙', '１２） item\n甲\n乙',
    '    code\n甲\n乙', '甲\n    code\n乙',
    '甲\n\t乙\n丙', '甲\n---\n乙\n丙',
    '甲\n- - -\n乙\n丙', '\u00a0甲\n\u2003乙\n',
]
for ending in ['\n', '\r\n', '\r', '\v', '\f', '\x1c', '\x1d', '\x1e', '\x85', '\u2028', '\u2029']:
    for tail in ['', ending, ending * 3]:
        adversarial.append(ending.join(['甲𠮷', '  乙。', '丙', '', '丁', '戊']) + tail)
for index, body in enumerate(adversarial):
    label = f'adversarial-{index}'
    capture(body)
for terminators, wrappers in [('', ''), ('，', ''), ('。', '』'), ('𠮷', '🙂'), defaults]:
    legacy.SENTENCE_TERMINATORS, legacy.CLOSING_WRAPPERS = terminators, wrappers
    for body in ['甲，\n乙', '甲。』\n乙', '甲𠮷🙂\n乙', '』\n乙', '甲。 \n乙']:
        label = 'live-overrides'
        capture(body)
legacy.SENTENCE_TERMINATORS, legacy.CLOSING_WRAPPERS = defaults
rng = random.Random(20260926)
atoms = ['甲，', '乙。', '𠮷é', '', ' ', '#', '## h', '- item', '> quote',
         '   ```x', '```', '~~~', '    code', '\tcode', '<div>', '</div>',
         '<span>x</span>', '[ref]: /x', '|a|', '---', '  cont', '١. item']
for index in range(96):
    label = f'fixed-seed-{index}'
    body = '\n'.join(rng.choice(atoms) for _ in range(rng.randrange(2, 12)))
    capture(body + rng.choice(['', '\n', '\n\n']))

receipt = {'schema': 1, 'baseline_commit': 'a7c4d9da764462e09c0769ecd8ba548a46d27e83',
           'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
           'existing_tests': checked, 'cases': cases}
output = ROOT / 'tests/rust_migration/fixtures/paragraphs.json'
output.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({'cases': len(cases), 'existing_tests': len(checked),
                  'source_sha256': receipt['source_sha256'],
                  'fixture_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}))
