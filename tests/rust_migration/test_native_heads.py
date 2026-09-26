"""Installed native scanner, actual legacy outputs and live matcher boundaries."""
import json
from pathlib import Path
import re

import pytest

from llmwiki import _native, normalize

pytestmark = pytest.mark.skipif(not _native.available(), reason="native candidate required")


def test_frozen_outputs_without_python_scanner_or_rewrite(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("Python document algorithm executed")

    class Body(str):
        def splitlines(self, *args, **kwargs):
            return forbidden()

    monkeypatch.setattr(normalize, '_normalize_heads_python', forbidden)
    # Keep identity anchors intact while poisoning the actual helper bodies.
    for name in ['_line_ending', '_preserve_trailing_newline', '_rewrite_heading_level']:
        monkeypatch.setattr(getattr(normalize, name), '__code__', forbidden.__code__)
    before = dict(_native._configured_roots)
    oracle = json.loads((Path(__file__).parent / 'fixtures/heads.json').read_text())
    assert len(oracle['cases']) == 60
    for case in oracle['cases']:
        assert normalize.normalize_heads(Body(case['body']), case['rules']) == case['expected'], case['name']
        trace = _native.last_trace()
        assert trace['core'] == 'rust' and trace['operation'] == 'utility_normalize_heads'
        assert trace['storage'] == 'none'
        assert trace['matcher'] == ('python_regex' if case['rules'] else 'none')
    assert before == _native._configured_roots


def test_live_rules_compiled_flags_and_python_regex_features():
    rules = [{'pattern': re.compile('^CHAPTER$', re.I | re.ASCII), 'level': 2}]
    assert normalize.normalize_heads('# chapter', rules) == '## chapter'
    rules[0] = {'pattern': r'(?<=word )word$', 'level': 5}
    assert normalize.normalize_heads('# word word', rules) == '##### word word'
    rules[0] = {'pattern': r'^(word) \1$', 'level': 6}
    assert normalize.normalize_heads('# word word', rules) == '###### word word'
    assert normalize.normalize_heads('# chapter', rules) == '# chapter'


def test_callback_order_skip_blocks_and_exception_identity():
    calls = []
    body = '# first\n```\n# skip\n```\n<div>\n# skip2\n\n## second\n'

    def classify(title):
        calls.append(title)
        return 3

    assert _native.normalize_heads_run(body, classify) == body.replace('# first', '### first').replace('## second', '### second')
    assert calls == ['first', 'second']
    assert _native.last_trace()['callback_count'] == 2
    error = TypeError('callback identity')
    calls.clear()

    def fail(title):
        calls.append(title)
        _native.utility_transform('sha1', {'text': 'nested success'})
        raise error

    with pytest.raises(TypeError) as caught:
        _native.normalize_heads_run(body, fail)
    assert caught.value is error and calls == ['first']
    assert _native.last_trace() is None


def test_eager_validation_and_legacy_float_bool_levels():
    assert normalize.normalize_heads('# title', [{'pattern': '.', 'level': True}]) == '# title'
    assert normalize.normalize_heads('# title', [{'pattern': 'absent', 'level': 2.0}]) == '# title'
    for body, rule, error in [
        ('# title', {'pattern': '.', 'level': 2.0}, TypeError),
        ('', {'pattern': '[', 'level': 0}, ValueError),
        ('', {'pattern': '[', 'level': 2}, re.error),
    ]:
        _native.utility_transform('sha1', {'text': 'seed'})
        with pytest.raises(error):
            normalize.normalize_heads(body, [rule])
        assert _native.last_trace() is None
    body = '# title\r\ntext\u2028\r'
    assert normalize.normalize_heads(body, []) == body
    assert _native.last_trace()['matcher'] == 'none'
    assert _native.last_trace()['callback_count'] == 0


@pytest.mark.parametrize('name', list(normalize._HEAD_DEFAULTS))
def test_live_parser_override_then_native_restoration(monkeypatch, name):
    original = getattr(normalize, name)
    calls = []

    def call(*args):
        calls.append(args)
        return original(*args)

    class Pattern:
        def match(self, *args):
            calls.append(args)
            return original.match(*args)

    replacement = Pattern() if name.endswith('_RE') else call
    body = '```\n```\n# title\n'
    with monkeypatch.context() as patch:
        patch.setattr(normalize, name, replacement)
        assert normalize.normalize_heads(body, [{'pattern': '.', 'level': 2}]) == body.replace('# title', '## title')
        assert calls and _native.last_trace()['core'] == 'python'
    assert normalize.normalize_heads(body, [{'pattern': '.', 'level': 2}]) == body.replace('# title', '## title')
    assert _native.last_trace()['core'] == 'rust'


def test_override_failure_clears_nested_trace_and_unbuilt_fallback(monkeypatch):
    error = TypeError('override failure')

    def fail(*args):
        _native.utility_transform('sha1', {'text': 'nested'})
        raise error

    with monkeypatch.context() as patch:
        patch.setattr(normalize, '_rewrite_heading_level', fail)
        with pytest.raises(TypeError) as caught:
            normalize.normalize_heads('# title', [{'pattern': '.', 'level': 2}])
        assert caught.value is error and _native.last_trace() is None
    monkeypatch.setattr(_native, 'available', lambda: False)
    assert normalize.normalize_heads('# title', [{'pattern': '.', 'level': 2}]) == '## title'
    assert _native.last_trace()['core'] == 'python'


def test_required_native_failure_is_not_swallowed(monkeypatch):
    error = _native.NativeCapabilityError('required extension unavailable')

    def fail():
        raise error

    _native.utility_transform('sha1', {'text': 'seed'})
    monkeypatch.setattr(_native, 'available', fail)
    with pytest.raises(_native.NativeCapabilityError) as caught:
        normalize.normalize_heads('# title', [])
    assert caught.value is error and _native.last_trace() is None
