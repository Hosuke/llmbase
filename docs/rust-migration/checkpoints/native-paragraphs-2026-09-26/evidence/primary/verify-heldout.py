"""Independent fixed-seed differential check against the untouched legacy body."""
import hashlib
import importlib.util
import json
from pathlib import Path
import random
import sys

from llmwiki import _core, _native, normalize

root = Path(__file__).resolve().parent
source = root / 'legacy_normalize.py'
assert hashlib.sha256(source.read_bytes()).hexdigest() == 'b0411b409377b6c19fc0ecca29b07dd8481fe74b859f066029f00a92cd25c4e1'
spec = importlib.util.spec_from_file_location('llmwiki._paragraph_legacy_oracle', source)
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
assert legacy._FENCE_OPEN_RE.pattern == r'^ {0,3}(`{3,}|~{3,})'
assert legacy._FENCE_CLOSE_RE.pattern == r'^ {0,3}([`~]{3,})\s*$'
configured_before = dict(_native._configured_roots)
rng = random.Random(385012)
atoms = ['甲𠮷，', '乙。』', 'a é', '', '\u00a0', '\x1f', '##', '### heading',
         '- item', '١٢. item', '１２) item', '  >quote', '>quote', '||||',
         '---', '_ _ _ ', '\x1f>quote', '````x', '```', '~`~', '`~~`', '~~~',
         '  cont', '    code', '\tcode', '<scrİpt>', '<ſTYLE>', '<div>', '</div>',
         '<textarea>', '<span>x</span>', '<!--', '<!DOCTYPE', '<?pi?>',
         '[ref]:\x1furl', '[ref]:\u3000url', '<h1/>', '無終止', '。</span>']
endings = ['\n', '\r\n', '\r', '\v', '\f', '\x1c', '\x1d', '\x1e', '\x85', '\u2028', '\u2029']
settings = [('。！？；.!?;', '）」』】》〉)]}\"\''), ('', ''), ('，', '』'), ('𠮷', '🙂'), ('。', ' \t』')]
count = 0
corpus = hashlib.sha256()
for index in range(1200):
    body = ''.join(rng.choice(atoms) + rng.choice(endings) for _ in range(rng.randrange(1, 15)))
    body += rng.choice(['', '\n', '\r\n\n', '末', '末\r'])
    terminators, wrappers = rng.choice(settings)
    legacy.SENTENCE_TERMINATORS = normalize.SENTENCE_TERMINATORS = terminators
    legacy.CLOSING_WRAPPERS = normalize.CLOSING_WRAPPERS = wrappers
    expected = legacy.normalize_paragraphs(body)
    actual = normalize.normalize_paragraphs(body)
    assert actual == expected, json.dumps({'index': index, 'body': body, 'expected': expected, 'actual': actual}, ensure_ascii=False)
    trace = _native.last_trace()
    assert trace['core'] == 'rust' and trace['operation'] == 'utility_normalize_paragraphs'
    corpus.update(json.dumps([body, terminators, wrappers, expected], ensure_ascii=False).encode())
    count += 1
assert _native._configured_roots == configured_before
receipt = {'passed': True, 'cases': count, 'seed': 385012,
           'legacy_source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
           'corpus_sha256': corpus.hexdigest(), 'extension': _core.__file__,
           'extension_sha256': hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
           'no_library_registration': True, 'last_trace': trace}
(root / 'heldout-result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
