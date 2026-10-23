"""Installed-native visibility reads are bounded by frontmatter, not body size."""

import json
import os

import pytest
import yaml

from llmwiki import _native, http_auth, web
from llmwiki.config import BUILTIN_LANGUAGES, load_config


LARGE_SIZE = 65 * 1024 * 1024
HEADER_LIMIT = 1024 * 1024
PUBLIC_ROUTES = ("/api/trails", "/api/taxonomy?lang=zh", "/api/xici?lang=zh")
PUBLIC_HEADER = b"---\ntitle: Public raw\nprivate: false\n---\n"


@pytest.fixture(autouse=True)
def native_anonymous_environment(monkeypatch):
    if not _native.available():
        pytest.skip("installed native candidate required")
    assert _native.backend_info()["core"] == "rust"
    for name in ("LLMBASE_API_SECRET", "LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "PORT"):
        monkeypatch.delenv(name, raising=False)


@pytest.fixture
def public_library(tmp_kb):
    config_path = tmp_kb / "config.yaml"
    cfg = yaml.safe_load(config_path.read_text())
    # Use the official profile schema; these endpoints validate their language.
    cfg["languages"] = BUILTIN_LANGUAGES
    config_path.write_text(yaml.safe_dump(cfg))
    meta = tmp_kb / "wiki/_meta"
    artifacts = {
        "index.json": [{"slug": "kong", "title": "Public indexed title"}],
        "trails.json": {"trails": []},
        "taxonomy.json": {"categories": [{"id": "public", "name": "Public category",
                                            "article_slugs": ["kong"], "children": []}]},
        "xici-zh.json": {"text": "PUBLIC_XICI_BODY", "themes": [], "lang": "zh",
                         "generated_at": None, "article_count": 3},
    }
    for filename, value in artifacts.items():
        (meta / filename).write_text(json.dumps(value))
    return tmp_kb


def write_sparse_raw(base, prefix):
    path = base / "raw/large/index.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as stream:
        stream.write(prefix)
        stream.truncate(LARGE_SIZE)
    assert path.stat().st_size > 64 * 1024 * 1024
    return path


def snapshot(base):
    return http_auth.visibility_snapshot(base, load_config(base))


def public_responses(client):
    result = {}
    for route in PUBLIC_ROUTES:
        response = client.get(route)
        assert response.status_code == 200, (route, response.get_data(as_text=True))
        result[route] = response.json
    assert result["/api/trails"] == {"trails": []}
    assert result["/api/taxonomy?lang=zh"]["categories"]
    assert result["/api/xici?lang=zh"]["text"] == "PUBLIC_XICI_BODY"
    return result


def rewrite_preserving_size_and_mtime(path, old, new):
    before = path.stat()
    assert before.st_size < 4096, "this helper must never read the large body"
    original = path.read_bytes()
    changed = original.replace(old, new, 1)
    assert changed != original and len(changed) == before.st_size
    path.write_bytes(changed)
    os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
    assert path.stat().st_size == before.st_size
    assert path.stat().st_mtime_ns == before.st_mtime_ns


@pytest.mark.parametrize("prefix", [
    PUBLIC_HEADER + b"Small body.\n",
    b"---\r\ntitle: Public raw\r\nprivate: false\r\n...\r\nSmall body.\r\n",
    b"Plain Markdown without frontmatter.\n",
], ids=["lf", "crlf-yaml-end", "no-frontmatter"])
def test_large_raw_body_preserves_snapshot_and_public_aggregate_responses(public_library, prefix):
    base = public_library
    raw = base / "raw/large/index.md"
    raw.parent.mkdir(parents=True)
    raw.write_bytes(prefix)
    client = web.create_web_app(base).test_client()
    small_snapshot = snapshot(base)
    assert small_snapshot["raw"]["large"]["public"] is True
    assert small_snapshot["has_nonpublic"] is False
    small_responses = public_responses(client)

    write_sparse_raw(base, prefix)
    # Body growth and sparse NUL bytes are irrelevant once the header ends.
    assert snapshot(base) == small_snapshot
    assert public_responses(client) == small_responses


def test_lite_etag_tracks_live_metadata_instead_of_body_bytes(public_library):
    base = public_library
    write_sparse_raw(base, PUBLIC_HEADER)
    article = base / "wiki/concepts/kong.md"
    article.write_bytes(b"---\ntitle: First title\nprivate: false\n---\nBody A\n")
    client = web.create_web_app(base).test_client()
    before = client.get("/api/articles/lite")
    assert before.status_code == 200 and before.json["total"] == 1
    original_snapshot = snapshot(base)

    rewrite_preserving_size_and_mtime(article, b"Body A", b"Body B")
    assert snapshot(base) == original_snapshot
    body_edit = client.get("/api/articles/lite", headers={"If-None-Match": before.headers["ETag"]})
    assert body_edit.status_code == 304

    rewrite_preserving_size_and_mtime(article, b"First title", b"Other title")
    metadata_snapshot = snapshot(base)
    assert metadata_snapshot["compiled"]["kong"]["public"] is True
    assert metadata_snapshot["compiled"]["kong"] != original_snapshot["compiled"]["kong"]
    metadata_edit = client.get("/api/articles/lite", headers={"If-None-Match": before.headers["ETag"]})
    assert metadata_edit.status_code == 200 and metadata_edit.json == before.json
    assert metadata_edit.headers["ETag"] != before.headers["ETag"]

    rewrite_preserving_size_and_mtime(article, b"private: false", b"private: true ")
    private_snapshot = snapshot(base)
    assert private_snapshot["compiled"]["kong"]["public"] is False
    assert private_snapshot["has_nonpublic"] is True
    private_edit = client.get("/api/articles/lite", headers={"If-None-Match": metadata_edit.headers["ETag"]})
    assert private_edit.status_code == 200
    assert private_edit.json == {"articles": [], "total": 0}
    assert private_edit.headers["ETag"] != metadata_edit.headers["ETag"]
    assert client.get("/api/articles/kong").status_code == 404
    for route in PUBLIC_ROUTES[1:]:
        denied = client.get(route)
        assert denied.status_code == 401 and denied.json["required_scopes"] == ["private"]


@pytest.mark.parametrize("header", [
    b"---\nprivate: true\n---\n",
    b"---\nprivate: [unterminated\n---\n",
    b"---\nprivate: false\npadding: " + b"a" * HEADER_LIMIT + b"\n---\n",
    b"---\nprivate: false\n",
], ids=["private", "malformed", "over-header-limit", "unterminated"])
def test_large_body_does_not_hide_private_or_invalid_frontmatter(public_library, header):
    base = public_library
    write_sparse_raw(base, header)
    exceeds_header = len(header) > HEADER_LIMIT or header == b"---\nprivate: false\n"
    if exceeds_header:
        # The sparse body's NUL bytes form an oversized unterminated header.
        # Capacity exhaustion now has an explicit diagnostic, still fail-closed.
        with pytest.raises(RuntimeError) as captured:
            snapshot(base)
        assert captured.value.code == "visibility_snapshot_limit_exceeded"
        assert captured.value.resource == "max_header_bytes"
        client = web.create_web_app(base).test_client()
        for route in PUBLIC_ROUTES[1:]:
            response = client.get(route)
            assert response.status_code == 503
            assert response.json["code"] == "visibility_snapshot_limit_exceeded"
            assert "PUBLIC_XICI_BODY" not in response.get_data(as_text=True)
        return
    visibility = snapshot(base)
    assert visibility["raw"]["large"]["public"] is False
    assert visibility["has_nonpublic"] is True
    client = web.create_web_app(base).test_client()
    for route in PUBLIC_ROUTES[1:]:
        response = client.get(route)
        assert response.status_code == 401 and response.json["required_scopes"] == ["private"]
        assert "PUBLIC_XICI_BODY" not in response.get_data(as_text=True)
