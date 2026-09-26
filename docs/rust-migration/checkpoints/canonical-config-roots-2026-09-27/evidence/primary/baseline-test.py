"""Effective config roots agree with actual native writer scope and journaling."""
from copy import deepcopy
import json
from pathlib import Path
import sqlite3

import pytest

from llmwiki import _core, _native, atomic, config

pytestmark = pytest.mark.skipif(not _native.available(), reason='native candidate required')


@pytest.fixture(autouse=True)
def isolated_registry(monkeypatch):
    monkeypatch.setattr(_native, '_configured_roots', {})
    monkeypatch.setenv('LLMBASE_LIBRARY_ID', 'config-root-regression')


@pytest.mark.parametrize('mode', ['relative', 'absolute', 'dot', 'none', 'symlink', 'missing-dotdot'])
def test_missing_config_roots_remain_absolute_after_cwd_change(tmp_path, monkeypatch, mode):
    monkeypatch.chdir(tmp_path)
    base = Path('library')
    expected = tmp_path / 'library'
    if mode == 'absolute':
        base = expected
    elif mode in {'dot', 'none'}:
        base = Path('.') if mode == 'dot' else None
        expected = tmp_path
    elif mode == 'symlink':
        expected.mkdir()
        Path('alias').symlink_to(expected, target_is_directory=True)
        base = Path('alias')
    elif mode == 'missing-dotdot':
        base = Path('not-created/../library')

    before = sorted(str(p.relative_to(tmp_path)) for p in tmp_path.rglob('*'))
    read = config._read_config(base)
    assert _native._configured_roots == {}
    assert sorted(str(p.relative_to(tmp_path)) for p in tmp_path.rglob('*')) == before
    cfg = config.load_config(base)
    assert cfg == read
    assert set(_native._configured_roots) == {(str(expected.resolve()), 'config-root-regression')}
    for key, suffix in {'raw': 'raw', 'wiki': 'wiki', 'concepts': 'wiki/concepts',
                        'outputs': 'wiki/outputs', 'meta': 'wiki/_meta'}.items():
        assert Path(cfg['paths'][key]) == expected.resolve() / suffix, (mode, key)
        assert Path(cfg['paths'][key]).is_absolute()

    config.ensure_dirs(cfg)
    relocated = tmp_path / 'relocated'
    relocated.mkdir()
    monkeypatch.chdir(relocated)
    destination = Path(cfg['paths']['concepts']) / 'probe.md'
    content = '原文𠮷é\r\n第二行\n'
    atomic.atomic_write_text(destination, content)
    trace = _native.last_trace()
    assert trace['core'] == 'rust' and trace['journal'] is True
    assert trace['coordination'] == 'library_writer_and_file_lock'
    assert trace['library_id'] == 'config-root-regression'
    assert destination.read_bytes() == content.encode()
    replacement = '修訂𠮷\n'
    atomic.atomic_write_text(destination, replacement)
    database = Path(cfg['paths']['meta']) / 'llmbase.sqlite3'
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as db:
        rows = db.execute('SELECT library_id, root, relative_path, before_bytes, after_bytes, status '
                          'FROM write_journal ORDER BY rowid').fetchall()
    assert rows == [
        ('config-root-regression', 'concepts', 'probe.md', None, content.encode(), 'applied'),
        ('config-root-regression', 'concepts', 'probe.md', content.encode(), replacement.encode(), 'applied'),
    ]
    assert destination.read_bytes() == replacement.encode()
    assert not (expected / 'library').exists()
    assert not (tmp_path / 'not-created').exists()
    assert not (relocated / 'library').exists()


@pytest.mark.parametrize('absolute', [False, True])
def test_existing_yaml_custom_roots_unknown_fields_and_scope(tmp_path, monkeypatch, absolute):
    monkeypatch.chdir(tmp_path)
    base = Path('configured')
    base.mkdir()
    source = ('paths:\n  raw: originals\n  wiki: knowledge\n  concepts: knowledge/entries\n'
              '  outputs: ../external-output\n  meta: knowledge/state\n'
              'custom:\n  keep: [未知, 42]\n')
    file = base / 'config.yaml'
    file.write_bytes(source.encode())
    cfg = config.load_config(base.resolve() if absolute else base)
    assert cfg['custom'] == {'keep': ['未知', 42]}
    assert 'llm' not in cfg and 'languages' not in cfg  # No implicit defaults merge.
    assert cfg['paths']['concepts'] == str((base / 'knowledge/entries').resolve())
    assert cfg['paths']['outputs'] == str(tmp_path / 'external-output')
    assert file.read_bytes() == source.encode()
    config.ensure_dirs(cfg)
    destination = Path(cfg['paths']['outputs']) / 'external.md'
    atomic.atomic_write_text(destination, '允許的自定根\n')
    assert destination.read_bytes() == '允許的自定根\n'.encode()
    assert _native.last_trace()['journal'] is True

    outside = tmp_path / 'denied/probe.md'
    with pytest.raises(PermissionError):
        _core.atomic_write_at(str(outside), b'denied', cfg['base_dir'],
                              'config-root-regression', json.dumps(cfg['paths']))
    assert not outside.parent.exists()
    database = Path(cfg['paths']['meta']) / 'llmbase.sqlite3'
    database_before = database.read_bytes()
    with pytest.raises(PermissionError):
        atomic.atomic_write_text(database, 'must not replace database')
    assert database.read_bytes() == database_before
    assert file.read_bytes() == source.encode()


def test_defaults_are_live_deep_copied_and_partial_config_is_not_expanded(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    languages = {'default': 'custom', 'profiles': {'custom': {'sections': [{'code': 'wenyan'}]}}}
    monkeypatch.setattr(config, 'BUILTIN_LANGUAGES', languages)
    cfg = config._defaults(Path('relative'))
    assert all(Path(value).is_absolute() for value in cfg['paths'].values())
    assert cfg['languages'] == languages
    cfg['languages']['profiles']['custom']['sections'][0]['code'] = 'modified'
    assert languages['profiles']['custom']['sections'][0]['code'] == 'wenyan'
    assert config._defaults(Path('relative'))['languages'] == languages
    base = tmp_path / 'partial'
    base.mkdir()
    (base / 'config.yaml').write_text('paths:\n  concepts: entries\ncustom: true\n')
    partial = config._read_config(base)
    assert partial == {'paths': {'concepts': str(base / 'entries')}, 'custom': True, 'base_dir': str(base)}
    assert _native._configured_roots == {}
    assert not (base / 'entries').exists()


def test_live_defaults_override_remains_called(tmp_path, monkeypatch):
    expected = {'paths': {'concepts': str(tmp_path / 'custom')}, 'custom': [1, 2]}
    calls = []

    def defaults(base):
        calls.append(base)
        return deepcopy(expected)

    monkeypatch.setattr(config, '_defaults', defaults)
    cfg = config._read_config(tmp_path)
    assert calls == [tmp_path]
    assert cfg == dict(expected, base_dir=str(tmp_path))
    assert not (tmp_path / 'custom').exists()
    assert _native._configured_roots == {}
