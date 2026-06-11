"""Tests for the web language profile contract."""

import pytest
import yaml

import llmwiki.compile as compile_mod
from llmwiki.web import create_web_app


@pytest.fixture
def client(tmp_kb):
    app = create_web_app(tmp_kb)
    app.config["TESTING"] = True
    return app.test_client()


def test_api_languages_factory_profile(client):
    r = client.get("/api/languages")

    assert r.status_code == 200
    body = r.get_json()
    assert body["name"] == "trilingual"
    assert body["codes"] == ["en", "zh", "ja", "zh-en"]
    assert body["bare_body"] is False
    assert body["sections"][0]["header"] == "## English"


def test_taxonomy_lang_defaults_and_rejects_invalid(client):
    r = client.get("/api/taxonomy")
    assert r.status_code == 200

    bad = client.get("/api/taxonomy", query_string={"lang": "bogus"})
    assert bad.status_code == 400
    body = bad.get_json()
    assert body["error"] == "invalid lang"
    assert body["valid"] == ["en", "zh", "ja", "zh-en"]


def test_invalid_languages_config_returns_json_500(tmp_kb):
    config_path = tmp_kb / "config.yaml"
    cfg = yaml.safe_load(config_path.read_text())
    cfg["languages"] = {"default": "missing", "profiles": {}}
    config_path.write_text(yaml.dump(cfg))

    app = create_web_app(tmp_kb)
    app.config["TESTING"] = True
    client = app.test_client()

    languages = client.get("/api/languages")
    assert languages.status_code == 500
    assert languages.is_json
    assert "languages" in languages.get_json()["error"]

    taxonomy = client.get("/api/taxonomy")
    assert taxonomy.status_code == 500
    assert taxonomy.is_json
    assert "languages" in taxonomy.get_json()["error"]


def test_xici_accepts_profile_view_lang(client):
    r = client.get("/api/xici", query_string={"lang": "zh-en"})

    assert r.status_code == 200
    body = r.get_json()
    assert body["lang"] == "zh-en"


def test_xici_generate_rejects_profile_lang_without_xici_style(tmp_kb, monkeypatch):
    config_path = tmp_kb / "config.yaml"
    cfg = yaml.safe_load(config_path.read_text())
    cfg["languages"] = {
        "default": "wenyan",
        "profiles": {
            "wenyan": {
                "sections": [
                    {"code": "wenyan", "key": "文言", "header": ""},
                ],
            },
        },
    }
    config_path.write_text(yaml.dump(cfg))

    def fail_generate(*args, **kwargs):
        raise AssertionError("generate_xici should not be called for unsupported xici lang")

    monkeypatch.setattr("llmwiki.xici.generate_xici", fail_generate)
    app = create_web_app(tmp_kb)
    app.config["TESTING"] = True
    client = app.test_client()

    r = client.post("/api/xici/generate", json={"lang": "wenyan"})

    assert r.status_code == 400
    body = r.get_json()
    assert body["error"] == "lang not supported by xici (no LANG_STYLES entry)"
    assert body["valid"] == []


def test_xici_generate_accepts_factory_xici_lang(tmp_kb, monkeypatch):
    def fake_generate(base_dir=None, lang="zh"):
        return {
            "text": "ok",
            "themes": [],
            "lang": lang,
            "generated_at": "2026-06-11T00:00:00+00:00",
            "article_count": 3,
        }

    monkeypatch.setattr("llmwiki.xici.generate_xici", fake_generate)
    app = create_web_app(tmp_kb)
    app.config["TESTING"] = True
    client = app.test_client()

    r = client.post("/api/xici/generate", json={"lang": "zh"})

    assert r.status_code == 200
    body = r.get_json()
    assert body["lang"] == "zh"


def test_import_time_patch_reflected_over_http(tmp_kb):
    try:
        compile_mod.SECTION_HEADERS = [("文言", "")]
        app = create_web_app(tmp_kb)
        app.config["TESTING"] = True
        client = app.test_client()

        r = client.get("/api/languages")
        assert r.status_code == 200
        body = r.get_json()
        assert body["name"] == "_patched"
        assert body["bare_body"] is True
        assert body["default_lang"] == "zh"
        assert body["api_default_lang"] == "zh"
        assert body["codes"] == ["文言"]

        zh = client.get("/api/taxonomy", query_string={"lang": "zh"})
        assert zh.status_code != 400

        other = client.get("/api/taxonomy", query_string={"lang": "whatever"})
        assert other.status_code != 400

        default = client.get("/api/taxonomy")
        assert default.status_code != 400
    finally:
        compile_mod.SECTION_HEADERS = compile_mod.DEFAULT_SECTION_HEADERS
