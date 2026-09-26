"""Freeze actual old lint outputs for a synthetic all-checks library."""
import hashlib
import json
from pathlib import Path
import sys

repo = Path('/Users/lanchair/Developer/LLM-infra/Github/llmbase')
task = Path(__file__).resolve().parent
before = task.parent / 'b11-native-lint/before/llmwiki/lint/checks.py'
sys.path.insert(0, str(repo))
from llmwiki.lint import checks
assert Path(checks.__file__).resolve() == repo / 'llmwiki/lint/checks.py'
assert Path(checks.__file__).read_bytes() == before.read_bytes()
root = task / 'baseline-library'
assert not root.exists(), 'preserve the frozen baseline; do not rerun in place'
for name in ('originals', 'knowledge/entries', 'knowledge/answers', 'knowledge/metadata'):
    (root / name).mkdir(parents=True, exist_ok=True)
(root / 'config.yaml').write_text('''paths:
  raw: originals
  wiki: knowledge
  concepts: knowledge/entries
  outputs: knowledge/answers
  meta: knowledge/metadata
languages:
  default: classical
  profiles:
    classical:
      sections:
        - {key: 文言, header: "", code: zh, label: 文言}
      default_lang: zh
      api_default_lang: zh
''')
body = '學而時習之，𠮷字及組合字 é 當保其原。' * 5
articles = {
    'alpha': ({'title': 'Study / 學', 'summary': '學之義',
               'tags': ['shared', 'we need output tags', 7],
               'editorial_extension': {'locked': True, 'source': '合成'}},
              body + '\n[[missing]] [[時習]] [[beta|習]]'),
    'beta': ({'title': 'Practice / 學', 'summary': '習之義', 'tags': ['shared']}, body),
    'empty': ({'title': '', 'summary': '', 'tags': []}, '短文'),
    '學': ({'title': '學', 'summary': '文言', 'tags': ['shared']}, body),
}
import yaml
for slug, (meta, content) in articles.items():
    (root / 'knowledge/entries' / (slug + '.md')).write_text(
        '---\n' + yaml.safe_dump(meta, allow_unicode=True, sort_keys=False) + '---\n' + content + '\n')
meta = root / 'knowledge/metadata'
(meta / 'aliases.json').write_text(json.dumps({'時習': 'alpha'}, ensure_ascii=False))
(meta / 'backlinks.json').write_text(json.dumps({'alpha': ['beta']}, ensure_ascii=False))
(meta / 'taxonomy.json').write_text(json.dumps({'categories': [
    {'id': 'other', 'label': {'en': 'Other', 'zh': '其他'},
     'children': [], 'article_slugs': ['beta', '學']},
]}, ensure_ascii=False))

def inventory():
    return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(root.rglob('*')) if p.is_file()}

original = inventory()
results = {}
for allow in (False, True):
    checks.ALLOW_CJK_SLUGS = allow
    results[str(allow).lower()] = checks.lint(root)
assert inventory() == original, 'old lint changed the synthetic source bytes'
assert 'llmwiki._core' not in sys.modules
for key in ('structural', 'broken_links', 'orphans', 'missing_metadata',
            'dirty_tags', 'duplicates', 'stubs', 'uncategorized'):
    assert results['false'][key], f'fixture does not exercise {key}'
receipt = {'status': 'frozen_before_native_facade', 'results': results,
           'files_sha256': original,
           'legacy_checks_sha256': hashlib.sha256(before.read_bytes()).hexdigest(),
           'loaded_modules': {name: str(Path(mod.__file__).resolve())
                              for name, mod in sys.modules.items()
                              if (name == 'llmwiki' or name.startswith('llmwiki.'))
                              and getattr(mod, '__file__', None)}}
assert all(Path(p).is_relative_to(repo) for p in receipt['loaded_modules'].values())
(task / 'baseline-lint.json').write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({'passed': True, 'files': len(original),
                  'issue_counts': {mode: value['total_issues'] for mode, value in results.items()}},
                 ensure_ascii=False))
