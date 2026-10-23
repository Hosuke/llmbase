"""Native visibility capacity has actionable, non-disclosing HTTP failures."""

import json

import pytest

from llmwiki import _native, agent_api, http_auth, web
from llmwiki.config import BUILTIN_LANGUAGES, load_config


CODE = "visibility_snapshot_limit_exceeded"
CONFIG_CODE = "visibility_snapshot_config_invalid"
SECRET = "PRIVATE_CONTENT_SENTINEL"
CONFIG_SECRET = "PRIVATE_CONFIG_SENTINEL"
PUBLIC_HEADER = f"---\ntitle: {SECRET}\nprivate: false\n---\n"


@pytest.fixture(autouse=True)
def native_anonymous_environment(monkeypatch):
    if not _native.available():
        pytest.skip("installed native candidate required")
    assert _native.backend_info()["core"] == "rust"
    for name in ("LLMBASE_API_SECRET", "LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "PORT"):
        monkeypatch.delenv(name, raising=False)


@pytest.fixture
def library(tmp_path):
    base = tmp_path / "PRIVATE_LIBRARY_SENTINEL"
    paths = {key: base / relative for key, relative in {
        "raw": "raw", "wiki": "wiki", "concepts": "wiki/concepts",
        "outputs": "wiki/outputs", "meta": "wiki/_meta"}.items()}
    for path in paths.values():
        path.mkdir(parents=True, exist_ok=True)
    config = {"paths": {key: str(path) for key, path in paths.items()},
              "languages": BUILTIN_LANGUAGES, "entities": {"enabled": False}}
    (base / "config.yaml").write_text(json.dumps(config))
    (paths["meta"] / "trails.json").write_text('{"trails": []}')
    (paths["meta"] / "xici-zh.json").write_text('{"text": "PUBLIC_GUIDE", "lang": "zh"}')
    return base


def configure(base, limits):
    path = base / "config.yaml"
    config = json.loads(path.read_text())
    config["visibility_snapshot"] = limits
    path.write_text(json.dumps(config))


def assert_no_disclosure(value, base):
    text = json.dumps(value) if isinstance(value, dict) else str(value)
    for forbidden in (str(base), "PRIVATE_LIBRARY_SENTINEL", SECRET, CONFIG_SECRET):
        assert forbidden not in text


def prepare_limit(base, resource):
    article = base / "wiki/concepts/public.md"
    if resource == "max_files":
        article.write_text("")
        (base / "raw/other.md").write_text("")
        return {"max_files": 1}, {"max_files": 2}
    if resource == "max_read_bytes":
        article.write_text(PUBLIC_HEADER + SECRET * 256)
        return ({"max_read_bytes": 128, "max_header_bytes": 128},
                {"max_read_bytes": 8192, "max_header_bytes": 128})
    article.write_text(f"---\nprivate: false\npadding: {SECRET * 20}\n---\nbody")
    return ({"max_read_bytes": 8192, "max_header_bytes": 64},
            {"max_read_bytes": 8192, "max_header_bytes": 1024})


@pytest.mark.parametrize("resource", ["max_files", "max_read_bytes", "max_header_bytes"])
def test_capacity_error_is_typed_actionable_and_recovers_on_live_config_change(library, resource):
    from llmwiki import _core

    low, high = prepare_limit(library, resource)
    configure(library, low)
    exception_type = _core.VisibilitySnapshotLimitError
    with pytest.raises(exception_type) as caught:
        http_auth.visibility_snapshot(library, load_config(library))
    error = caught.value
    assert isinstance(error, RuntimeError)
    assert error.code == CODE and error.resource == resource
    assert error.limit == low[resource] and error.observed > error.limit
    assert_no_disclosure(error, library)

    # Neither flat nor nested operation arguments can lift configured limits.
    overrides = {"max_files": 100_000, "max_read_bytes": 100_000, "max_header_bytes": 100_000}
    with pytest.raises(exception_type) as bypass:
        _native.dispatch("compat_kb_visibility", {**overrides, "visibility_snapshot": overrides},
                         library, paths=load_config(library)["paths"])
    assert bypass.value.resource == resource and bypass.value.limit == low[resource]

    client = web.create_web_app(library).test_client()
    agent_client = agent_api.create_agent_server(library).test_client()
    agent_response = agent_client.get("/api/articles")
    assert agent_response.status_code == 503
    assert agent_response.json["code"] == CODE
    assert agent_response.json["config_key"] == "visibility_snapshot." + resource
    assert_no_disclosure(agent_response.json, library)
    for route in ("/api/trails", "/api/xici?lang=zh"):
        response = client.get(route)
        assert response.status_code == 503
        data = response.json
        assert set(data) == {"status", "code", "message", "limit", "config_key"}
        assert data["status"] == "error" and data["code"] == CODE
        assert data["message"] and data["message"].isascii()
        assert data["limit"]["resource"] == resource
        assert data["limit"]["configured"] == low[resource]
        assert data["limit"]["observed"] > low[resource]
        assert data["config_key"] == "visibility_snapshot." + resource
        assert_no_disclosure(data, library)

    configure(library, high)
    assert agent_client.get("/api/articles").status_code == 200
    assert client.get("/api/trails").json == {"trails": []}
    recovered = client.get("/api/xici?lang=zh")
    assert recovered.status_code == 200 and recovered.json["text"] == "PUBLIC_GUIDE"
    configure(library, low)
    assert client.get("/api/xici?lang=zh").json["code"] == CODE
    configure(library, high)
    (library / "wiki/concepts/public.md").write_text(f"---\nprivate: true\n---\n{SECRET}")
    private = client.get("/api/xici?lang=zh")
    assert private.status_code == 401 and private.json["required_scopes"] == ["private"]
    assert_no_disclosure(private.json, library)


@pytest.mark.parametrize("limits", [
    None, False, [], 7, CONFIG_SECRET,
    {"max_files": 0}, {"max_files": True}, {"max_files": CONFIG_SECRET},
    {"max_files": -1}, {"max_read_bytes": 1.5}, {"max_files": 2**64 - 1},
    {"unexpected": CONFIG_SECRET}, {"max_read_bytes": 64, "max_header_bytes": 65},
])
def test_invalid_visibility_configuration_is_typed_and_redacted(library, limits):
    from llmwiki import _core

    configure(library, limits)
    with pytest.raises(_core.VisibilitySnapshotConfigError) as caught:
        http_auth.visibility_snapshot(library, load_config(library))
    assert isinstance(caught.value, ValueError)
    assert caught.value.code == CONFIG_CODE
    assert_no_disclosure(caught.value, library)
    agent_response = agent_api.create_agent_server(library).test_client().get("/api/articles")
    assert agent_response.status_code == 503 and agent_response.json["code"] == CONFIG_CODE
    assert_no_disclosure(agent_response.json, library)
    client = web.create_web_app(library).test_client()
    response = client.get("/api/trails")
    assert response.status_code == 503
    assert response.json["status"] == "error" and response.json["code"] == CONFIG_CODE
    assert response.json["message"] and response.json["message"].isascii()
    assert_no_disclosure(response.json, library)
    configure(library, {})
    restored = client.get("/api/trails")
    assert restored.status_code == 200 and restored.json == {"trails": []}
