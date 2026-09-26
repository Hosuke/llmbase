"""Independent fixed-seed tests against the saved actual old taxonomy module."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import random
import sys

from llmwiki import _core, _native, taxonomy

root = Path(__file__).resolve().parent
source = root / 'legacy_taxonomy.py'
assert hashlib.sha256(source.read_bytes()).hexdigest() == '082c894d50cb1baa2a972f502a2a73f7121a8024cc8ca59e277c37bb72713aa7'
spec = importlib.util.spec_from_file_location('llmwiki._taxonomy_legacy_oracle', source)
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
assert Path(taxonomy.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
rng = random.Random(913721)
corpus = hashlib.sha256()
counts = {'fallback':0, 'localize_title':0, 'localize_tree':0}
before = dict(_native._configured_roots)

def compare(name, *args):
    copied = copy.deepcopy(args)
    helper = '_fallback_taxonomy' if name == 'fallback' else '_' + name
    expected = getattr(legacy, helper)(*copy.deepcopy(args))
    actual = getattr(taxonomy, helper)(*args)
    assert actual == expected, repr((name,args,expected,actual))
    assert args == copied, 'value helper mutated caller input'
    trace = _native.last_trace()
    assert trace['core']=='rust' and trace['operation']=='taxonomy_transform'
    assert trace['action']==name
    corpus.update(json.dumps([name,args,expected],ensure_ascii=False).encode())
    counts[name] += 1

tags=['topic', 'TOPIC', 'Mixed Words', 'Mixed-Words', "don't stop", 'category:經/史',
      '中a 國b', '𠮷é', 'ΟΣ', 'ǳabc', 'ßß', 'ᾀabc', 'ქართული', '', 'a  b', 'a_b', 'a\tB',
      'İ', 'ı', 'ﬄabc', '\u0345α', '漢ß', '中文', '日本語', '他']
for _ in range(180):
    articles=[{'slug':rng.choice(['a','b','c','d','𠮷','same','same']),
               'tags':[rng.choice(tags) for _ in range(rng.randrange(5))]}
              for _ in range(rng.randrange(18))]
    compare('fallback', articles)
titles=['Mencius / 孟子', '孟子 / Mencius', '𠮷 / 英', 'かな / English', ' / 甲 / 乙 / ',
        '甲/abc/かな', 'abc', '', None, 'a\x1f/\u3000漢', 'é / 經']
langs=['en','zh','ja','zh-en','文言','unknown','']
for title in titles:
    for lang in langs: compare('localize_title', title, lang)

def nodes(depth=0):
    result=[]
    for i in range(rng.randrange(1,4)):
        label=rng.choice(['固定', {}, {'en':'English','zh':'古義'}, {'en':None,'zh':'古義'}, {'zh':'古義'}])
        node={'id':f'n{depth}-{i}', 'label':copy.deepcopy(label),
              'article_slugs':[rng.choice(['a','b','𠮷','missing','a']) for _ in range(rng.randrange(5))]}
        if depth<3 and rng.randrange(2): node['children']=nodes(depth+1)
        result.append(node)
    return result

for _ in range(100):
    compare('localize_tree',nodes(),rng.choice(langs),{'a':'Alpha / 學','b':None,'𠮷':'𠮷 / English'})
assert _native._configured_roots==before
receipt={'passed':True,'counts':counts,'seed':913721,'corpus_sha256':corpus.hexdigest(),
         'legacy_source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
         'extension':_core.__file__,'extension_sha256':hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
         'no_library_registration':True,'input_values_unchanged':True}
(root/'heldout-result.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
