"""Tests for config-derived language profiles."""

import pytest

import llmwiki.compile as compile_mod
from llmwiki.config import BUILTIN_LANGUAGES, get_language_profile, get_section_headers


def test_builtin_trilingual_profile_without_cfg():
    expected_headers = [
        ("english", "## English"),
        ("中文", "## 中文"),
        ("日本語", "## 日本語"),
    ]

    assert get_section_headers(None) == expected_headers
    assert get_section_headers({}) == expected_headers

    profile = get_language_profile(None)
    assert profile["codes"] == ["en", "zh", "ja", "zh-en"]
    assert profile["default_lang"] == "zh-en"
    assert profile["bare_body"] is False
    assert get_language_profile({})["codes"] == ["en", "zh", "ja", "zh-en"]


def test_config_wenyan_profile_bare_body():
    cfg = {
        "languages": {
            "default": "wenyan",
            "profiles": {
                "wenyan": {
                    "sections": [{"key": "文言", "header": ""}],
                },
            },
        },
    }

    assert get_section_headers(cfg) == [("文言", "")]
    profile = get_language_profile(cfg)
    assert profile["single_section"] is True
    assert profile["bare_body"] is True
    assert profile["codes"] == ["文言"]


def test_import_time_section_headers_patch_wins_over_config():
    cfg = {"languages": BUILTIN_LANGUAGES}
    try:
        compile_mod.SECTION_HEADERS = [("文言", "")]
        assert get_section_headers(cfg) == [("文言", "")]
        profile = get_language_profile(cfg)
        assert profile["name"] == "_patched"
        assert profile["default_lang"] == "zh"
        assert profile["api_default_lang"] == "zh"
        assert profile["codes"] == ["文言"]
    finally:
        compile_mod.SECTION_HEADERS = compile_mod.DEFAULT_SECTION_HEADERS


def test_validation_rejects_empty_header_with_two_sections():
    cfg = {
        "languages": {
            "default": "bad",
            "profiles": {
                "bad": {
                    "sections": [
                        {"key": "文言", "header": ""},
                        {"key": "english", "header": "## English"},
                    ],
                },
            },
        },
    }

    with pytest.raises(ValueError, match="empty section header"):
        get_language_profile(cfg)


def test_validation_rejects_missing_default_profile():
    cfg = {
        "languages": {
            "default": "missing",
            "profiles": {
                "trilingual": {
                    "sections": [{"key": "english", "header": "## English"}],
                },
            },
        },
    }

    with pytest.raises(ValueError, match="languages.default"):
        get_language_profile(cfg)


def test_validation_rejects_view_show_unknown_code():
    cfg = {
        "languages": {
            "default": "bad",
            "profiles": {
                "bad": {
                    "sections": [{"code": "zh", "key": "中文", "header": "## 中文"}],
                    "views": [{"code": "zh-en", "show": ["zh", "en"]}],
                },
            },
        },
    }

    with pytest.raises(ValueError, match="show must be a non-empty subset"):
        get_language_profile(cfg)
