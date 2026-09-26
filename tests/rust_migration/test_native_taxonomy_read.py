"""Actual installed native taxonomy read/value helpers and live facade contracts."""
import hashlib
import json
from pathlib import Path

import pytest

from llmwiki import _native, taxonomy

pytestmark = pytest.mark.skipif(not _native.available(), reason="native candidate required")

ORACLE = json.loads((Path(__file__).parent / 'fixtures/taxonomy-read.json').read_text())


def write_library(base, files):
    for relative, data in files.items():
        path = base / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data, encoding='utf-8')


def inventory(base):
    return {p.relative_to(base).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in base.rglob('*') if p.is_file()}


def test_frozen_pure_cases_execute_in_rust_without_python_algorithm():
    assert len(ORACLE['pure']) == 76
    for case in ORACLE['pure']:
        args = case['args']
        if case['action'] == 'fallback':
            result = taxonomy._fallback_taxonomy(args['articles'])
        elif case['action'] == 'localize_title':
            result = taxonomy._localize_title(args['title'], args['lang'])
        else:
            result = taxonomy._localize_tree(args['tree'], args['lang'], args['titles'])
        assert result == case['expected'], case
        trace = _native.last_trace()
        assert trace == {'core': 'rust', 'operation': 'taxonomy_transform',
                         'action': case['action'], 'storage': 'none'}


def test_frozen_libraries_and_poisoned_python_algorithms(monkeypatch, tmp_path):
    def forbidden(*args, **kwargs):
        raise AssertionError('legacy Python taxonomy algorithm executed')
    for name in ['_legacy_load_articles', '_legacy_fallback_taxonomy', '_legacy_build_title_map',
                 '_legacy_localize_tree', '_legacy_localize_title', '_legacy_build_taxonomy',
                 '_legacy_load_taxonomy']:
        monkeypatch.setattr(taxonomy, name, forbidden)
    monkeypatch.setattr(taxonomy.frontmatter, 'load', forbidden)
    before = {key: dict(record) for key, record in _native._configured_roots.items()}
    for library in ORACLE['libraries']:
        base = tmp_path / library['name']
        write_library(base, library['files'])
        snapshot = inventory(base)
        assert taxonomy.load_taxonomy(base) == library['results']['load']
        assert _native.last_trace()['action'] == 'load'
        concepts = base / 'knowledge/entries'
        assert taxonomy._load_articles(concepts) == library['results']['articles']
        assert taxonomy._build_title_map(concepts) == library['results']['titles']
        for lang, expected in library['results']['build'].items():
            assert taxonomy.build_taxonomy(base, lang) == expected
            trace = _native.last_trace()
            assert trace == {'core': 'rust', 'operation': 'taxonomy_transform',
                             'action': 'build', 'storage': 'read_only_filesystem'}
        assert inventory(base) == snapshot == library['inventory']
    # Only the explicit config loads registered roots; earlier entries are unchanged.
    for key, record in before.items():
        assert key in _native._configured_roots
        assert _native._configured_roots[key] == record


def test_libraries_with_same_slugs_remain_separate(tmp_path):
    cached, uncached = ORACLE['libraries'][0], ORACLE['libraries'][1]
    bases = []
    for library in (cached, uncached):
        base = tmp_path / library['name']
        write_library(base, library['files'])
        bases.append(base)
    assert cached['files']['knowledge/entries/alpha.md'] != ''
    # Both libraries contain the same slugs with different cached state.
    for library, base in zip((cached, uncached), bases):
        for lang, expected in library['results']['build'].items():
            assert taxonomy.build_taxonomy(base, lang) == expected
    assert taxonomy.build_taxonomy(bases[0], 'en') == cached['results']['build']['en']
    assert taxonomy.build_taxonomy(bases[1], 'en') == uncached['results']['build']['en']
    assert taxonomy.build_taxonomy(bases[0], 'en') == cached['results']['build']['en']


def test_live_helper_replacement_receives_real_path_and_dicts(monkeypatch, tmp_path):
    library = ORACLE['libraries'][1]  # uncached
    base = tmp_path / 'uncached'
    write_library(base, library['files'])
    assert taxonomy.build_taxonomy(base, 'en') == library['results']['build']['en']
    assert _native.last_trace()['action'] == 'build'
    real_articles = taxonomy._legacy_load_articles(base / 'knowledge/entries')
    calls = []

    def replacement(concepts_dir):
        calls.append(concepts_dir)
        assert isinstance(concepts_dir, Path)
        return [dict(article) for article in real_articles]

    monkeypatch.setattr(taxonomy, '_load_articles', replacement)
    result = taxonomy.build_taxonomy(base, 'en')
    assert calls == [base / 'knowledge/entries']  # exactly one actual callback invocation
    assert result == library['results']['build']['en']
    trace = _native.last_trace()
    assert trace['core'] == 'python' and trace['operation'] == 'build_taxonomy'
    # Restoring returns to the native default branch.
    monkeypatch.undo()
    assert taxonomy.build_taxonomy(base, 'en') == library['results']['build']['en']
    assert _native.last_trace()['action'] == 'build'


def test_callback_typeerror_identity_no_retry_no_stale_trace(monkeypatch, tmp_path):
    library = ORACLE['libraries'][1]
    base = tmp_path / 'uncached'
    write_library(base, library['files'])
    assert taxonomy.build_taxonomy(base, 'en') == library['results']['build']['en']
    assert _native.last_trace()['core'] == 'rust'  # a real prior success receipt
    expected = TypeError('callback failure identity')
    calls = []

    def broken(concepts_dir):
        calls.append(concepts_dir)
        raise expected

    monkeypatch.setattr(taxonomy, '_load_articles', broken)
    with pytest.raises(TypeError) as caught:
        taxonomy.build_taxonomy(base, 'en')
    assert caught.value is expected
    assert len(calls) == 1  # no retry
    assert _native.last_trace() is None  # no stale success trace


def test_replaced_localize_title_is_honored_through_tree_and_build(monkeypatch, tmp_path):
    library = ORACLE['libraries'][0]  # cached
    base = tmp_path / 'cached'
    write_library(base, library['files'])
    calls = []

    def replacement(title, lang):
        calls.append((title, lang))
        return f'X{title}X' if isinstance(title, str) else title

    monkeypatch.setattr(taxonomy, '_localize_title', replacement)
    case = next(c for c in ORACLE['pure']
                if c['action'] == 'localize_tree' and c['args']['lang'] == 'en')
    result = taxonomy._localize_tree(case['args']['tree'], 'en', case['args']['titles'])
    assert calls  # the real replacement was actually invoked
    assert result[0]['articles'][0] == {'slug': 'alpha', 'title': 'XAlpha / 學X'}
    trace = _native.last_trace()
    assert trace['core'] == 'python' and trace['operation'] == 'localize_tree'
    calls.clear()
    built = taxonomy.build_taxonomy(base, 'en')
    assert calls  # honored through the build orchestration as well
    titles = library['results']['titles']

    def wrap(node):
        node = dict(node)
        node['articles'] = [dict(a, title=f"X{titles.get(a['slug'], a['slug'])}X")
                            for a in node['articles']]
        node['children'] = [wrap(c) for c in node['children']]
        return node

    assert built == [wrap(n) for n in library['results']['build']['en']]


def test_explicit_unbuilt_development_fallback(monkeypatch, tmp_path):
    monkeypatch.setattr(_native, 'available', lambda: False)

    def forbidden(*args, **kwargs):
        raise AssertionError('development fallback entered native transport')

    monkeypatch.setattr(_native, 'taxonomy_transform', forbidden)
    library = ORACLE['libraries'][1]
    base = tmp_path / 'uncached'
    write_library(base, library['files'])
    assert taxonomy.build_taxonomy(base, 'zh') == library['results']['build']['zh']
    assert taxonomy.load_taxonomy(base) == library['results']['load']
    case = next(c for c in ORACLE['pure']
                if c['action'] == 'fallback' and len(c['args']['articles']) == 3)
    assert taxonomy._fallback_taxonomy(case['args']['articles']) == case['expected']
    title_case = next(c for c in ORACLE['pure']
                      if c['action'] == 'localize_title' and c['args']['title'] == 'Mencius / 孟子'
                      and c['args']['lang'] == 'zh')
    assert taxonomy._localize_title('Mencius / 孟子', 'zh') == title_case['expected']


def test_missing_required_native_failure_is_not_swallowed(monkeypatch, tmp_path):
    expected = _native.NativeCapabilityError('required extension absent')

    def unavailable():
        raise expected

    monkeypatch.setattr(_native, 'available', unavailable)
    with pytest.raises(_native.NativeCapabilityError) as caught:
        taxonomy.build_taxonomy(tmp_path)
    assert caught.value is expected
    with pytest.raises(_native.NativeCapabilityError) as caught:
        taxonomy.load_taxonomy(tmp_path)
    assert caught.value is expected
    with pytest.raises(_native.NativeCapabilityError) as caught:
        taxonomy._fallback_taxonomy([])
    assert caught.value is expected


def test_wrapped_localize_tree_keeps_live_recursive_calls(monkeypatch):
    original = taxonomy._localize_tree
    calls = []

    def wrapper(tree, lang, titles):
        calls.append(len(tree))
        return original(tree, lang, titles)

    monkeypatch.setattr(taxonomy, '_localize_tree', wrapper)
    tree = [{'id': 'root', 'article_slugs': ['a'], 'children': [{'id': 'child'}]}]
    result = wrapper(tree, 'en', {'a': 'Alpha / 學'})
    assert calls == [1, 1, 0]
    assert result[0]['articles'] == [{'slug': 'a', 'title': 'Alpha'}]
    assert result[0]['total'] == 1
    assert result[0]['children'][0]['id'] == 'child'
    assert _native.last_trace()['core'] == 'python'
