"""Actual installed native paragraph normalization and live profile contracts."""
import json
from pathlib import Path

import pytest

from llmwiki import _native, normalize

pytestmark = pytest.mark.skipif(not _native.available(), reason="native candidate required")


def test_frozen_legacy_oracle_executes_in_rust_without_python_algorithm(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("legacy Python paragraph algorithm executed")
    for name in ["_line_ending", "_is_structural", "_ends_with_terminator", "_preserve_trailing_newline"]:
        monkeypatch.setattr(normalize, name, forbidden)
    before = dict(_native._configured_roots)
    oracle = json.loads((Path(__file__).parent / 'fixtures/paragraphs.json').read_text())
    assert len(oracle['cases']) == 229
    for case in oracle['cases']:
        monkeypatch.setattr(normalize, 'SENTENCE_TERMINATORS', case['terminators'])
        monkeypatch.setattr(normalize, 'CLOSING_WRAPPERS', case['wrappers'])
        assert normalize.normalize_paragraphs(case['body']) == case['expected'], case['name']
        trace = _native.last_trace()
        assert trace['core'] == 'rust'
        assert trace['operation'] == 'utility_normalize_paragraphs'
        assert trace['adapter'] == 'llmbase_core::compat_util::transform'
        assert trace['sequence'] > 0
    assert _native._configured_roots == before


def test_live_terminators_and_wrappers_are_read_on_every_call(monkeypatch):
    monkeypatch.setattr(normalize, 'SENTENCE_TERMINATORS', '。')
    monkeypatch.setattr(normalize, 'CLOSING_WRAPPERS', '')
    assert normalize.normalize_paragraphs('甲，\n乙') == '甲，乙'
    assert normalize.normalize_paragraphs('甲。』\n乙') == '甲。』乙'
    monkeypatch.setattr(normalize, 'SENTENCE_TERMINATORS', '。，')
    monkeypatch.setattr(normalize, 'CLOSING_WRAPPERS', '』')
    assert normalize.normalize_paragraphs('甲，\n乙') == '甲，\n乙'
    assert normalize.normalize_paragraphs('甲。』\n乙') == '甲。』\n乙'


def test_native_invalid_dto_does_not_fall_back(monkeypatch):
    normalize.normalize_paragraphs('甲\n乙')
    monkeypatch.setattr(normalize, 'CLOSING_WRAPPERS', 42)
    with pytest.raises(ValueError, match='wrappers must be a string'):
        normalize.normalize_paragraphs('甲\n乙')
    assert _native.last_trace() is None


def test_explicit_unbuilt_development_fallback(monkeypatch):
    monkeypatch.setattr(_native, 'available', lambda: False)
    def forbidden(*args, **kwargs):
        raise AssertionError('development fallback entered native transport')
    monkeypatch.setattr(_native, 'utility_transform', forbidden)
    assert normalize.normalize_paragraphs('甲，\n乙。\n\n') == '甲，乙。\n\n'


def test_required_native_capability_error_is_not_swallowed(monkeypatch):
    expected = _native.NativeCapabilityError('required extension absent')
    def unavailable():
        raise expected
    monkeypatch.setattr(_native, 'available', unavailable)
    with pytest.raises(_native.NativeCapabilityError) as caught:
        normalize.normalize_paragraphs('甲\n乙')
    assert caught.value is expected
