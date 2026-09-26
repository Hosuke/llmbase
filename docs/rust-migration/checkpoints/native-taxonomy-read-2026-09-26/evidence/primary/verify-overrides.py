"""Compare every live build helper boundary with the actual old implementation."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile

from llmwiki import _core, _native, taxonomy

root = Path(__file__).resolve().parents[1]
source = root / 'primary/legacy_taxonomy.py'
assert hashlib.sha256(source.read_bytes()).hexdigest() == '082c894d50cb1baa2a972f502a2a73f7121a8024cc8ca59e277c37bb72713aa7'
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
spec = importlib.util.spec_from_file_location('llmwiki._taxonomy_override_oracle', source)
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)
oracle = json.loads((root / 'source/tests/rust_migration/fixtures/taxonomy-read.json').read_text())
fixture = next(lib for lib in oracle['libraries'] if lib['name'] == 'uncached')
helpers = ['_load_articles', '_fallback_taxonomy', '_dedup_tree',
           '_build_title_map', '_localize_tree', '_localize_title']
checks = []

with tempfile.TemporaryDirectory(prefix='b13-primary-overrides-') as tmp:
    base = Path(tmp)
    for relative, data in fixture['files'].items():
        path = base / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data)
    for name in helpers:
        results, counts = [], []
        for module in (legacy, taxonomy):
            original = getattr(module, name)
            calls = []

            def replacement(*args, **kwargs):
                calls.append(tuple(type(a).__name__ for a in args))
                value = original(*args, **kwargs)
                if name == '_load_articles':
                    return value[:1]
                if name == '_fallback_taxonomy':
                    return [{'id': 'custom', 'label': 'custom', 'article_slugs': ['alpha']}]
                if name == '_dedup_tree':
                    for node in args[0]:
                        node['id'] = 'custom-' + node['id']
                if name == '_build_title_map':
                    return {slug: 'custom-' + str(title) for slug, title in value.items()}
                if name == '_localize_tree':
                    for node in value:
                        node['label'] = 'custom-' + str(node['label'])
                if name == '_localize_title':
                    return 'custom-' + str(value)
                return value

            try:
                setattr(module, name, replacement)
                results.append(copy.deepcopy(module.build_taxonomy(base, 'en')))
                counts.append(calls)
                if module is taxonomy:
                    trace = _native.last_trace()
                    assert trace['core'] == 'python' and trace['operation'] == 'build_taxonomy'
            finally:
                setattr(module, name, original)
        assert results[0] == results[1], name
        assert counts[0] and counts[0] == counts[1], (name, counts)
        checks.append({'helper': name, 'matching_callback_calls': len(counts[0])})

        # A fresh exception instance must survive the whole build boundary.
        original = getattr(taxonomy, name)
        failure = TypeError('primary live helper ' + name)
        calls = []

        def broken(*args, **kwargs):
            calls.append(1)
            raise failure

        taxonomy.build_taxonomy(base, 'en')
        try:
            setattr(taxonomy, name, broken)
            try:
                taxonomy.build_taxonomy(base, 'en')
            except TypeError as error:
                assert error is failure and len(calls) == 1
                assert _native.last_trace() is None
            else:
                raise AssertionError(name + ' swallowed callback exception')
        finally:
            setattr(taxonomy, name, original)
    inventory = {p.relative_to(base).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in base.rglob('*') if p.is_file()}
    assert inventory == fixture['inventory']

receipt = {'passed': True, 'helpers': checks, 'exception_identity_checks': len(helpers),
           'extension_sha256': hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest()}
(root / 'primary/overrides-result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
