"""Candidate fetch policy + actual Rust persistence; network pinning is tested in Rust."""
import hashlib
from pathlib import Path

import frontmatter
import pytest
import requests.sessions

from llmwiki import _native, ingest


@pytest.fixture(autouse=True)
def native_candidate():
    if not _native.available():
        pytest.skip("native candidate extension is not built")


def test_native_fetch_blocks_url_spellings_and_cannot_reuse_receipt(monkeypatch):
    def python_http(*args, **kwargs):
        raise AssertionError("default fetch used Python HTTP")
    monkeypatch.setattr(requests.sessions.Session, "request", python_http)
    for url in ["http://127.1/", "http://2130706433/", "http://0x7f000001/", "http://169.254.169.254/",
                "http://[::1]/", "http://[::ffff:127.0.0.1]/", "http://[fc00::1]/", "file:///tmp/secret",
                "http://u:secret@8.8.8.8/", "http://192.0.2.1/"]:
        _native.python_boundary("previous", "fixture")
        with pytest.raises(ValueError) as error:
            _native.fetch_url(url)
        assert "secret" not in str(error.value)
        assert _native.last_trace() is None


def test_default_ingest_validation_override_cannot_authorize_private_socket(tmp_path, monkeypatch):
    validations = []
    monkeypatch.setattr(ingest, "_validate_url", validations.append)
    with pytest.raises(ValueError, match="not public"):
        ingest.ingest_url("http://127.0.0.1:9/", tmp_path)
    assert validations == ["http://127.0.0.1:9/"]
    assert not list(tmp_path.rglob("*.md"))


def test_precancelled_native_fetch_and_ingest_do_no_work(tmp_path):
    cancellation = _native.fetch_cancellation()
    cancellation.cancel()
    assert cancellation.is_cancelled()
    with pytest.raises(ValueError, match="cancelled"):
        _native.fetch_url("https://unresolved.invalid/", cancellation=cancellation)
    with pytest.raises(ValueError, match="cancelled"):
        ingest.ingest_url("https://unresolved.invalid/", tmp_path, cancellation=cancellation)
    assert list(tmp_path.iterdir()) == []


class Response:
    status_code = 200
    def __init__(self, body, content_type="text/html", url=""):
        self.content = body
        self.text = body.decode("utf-8", errors="replace")
        self.headers = {"content-type": content_type}
        self.url = url
    def raise_for_status(self):
        pass


def test_live_python_fetch_hook_replaced_midprocess_and_trace_is_explicit(monkeypatch):
    seen = []
    monkeypatch.setattr(ingest, "_validate_url", seen.append)
    for body in [b"first", b"second"]:
        def callback(url, **kwargs):
            assert kwargs["allow_redirects"] is False
            return Response(body)
        monkeypatch.setattr(ingest.requests, "get", callback)
        assert ingest._fetch_response("https://fixture.invalid/", timeout=1, max_bytes=100).content == body
        assert _native.last_trace() == {"core": "python", "operation": "fetch_url", "callback": "requests.get"}
    assert seen == ["https://fixture.invalid/", "https://fixture.invalid/"]


def test_images_use_same_validation_and_native_atomic_persistence(tmp_path, monkeypatch):
    calls, validations, hooks = [], [], []
    original_validator = ingest._DEFAULT_VALIDATE_URL
    def validate(url):
        validations.append(url)
        if "fixture.invalid" not in url:
            original_validator(url)
    monkeypatch.setattr(ingest, "_validate_url", validate)
    image = b"\x89PNG\r\n\x1a\n\x00\xfforiginal"
    def callback(url, **kwargs):
        calls.append(url)
        assert kwargs["allow_redirects"] is False
        if url.endswith("/source"):
            return Response(b'<html><title>Test Article</title><article><p>Body</p>'
                            b'<img src="images/a.png"><img src="http://127.0.0.1:9/secret"></article></html>',
                            url="https://fixture.invalid/final/source")
        assert url == "https://fixture.invalid/final/images/a.png"
        return Response(image, "image/png", url)
    monkeypatch.setattr(ingest.requests, "get", callback)
    from llmwiki import hooks as hook_module
    monkeypatch.setattr(hook_module, "emit", lambda *a, **kw: hooks.append((a, kw)))
    def forbidden_python_write(*args, **kwargs):
        raise AssertionError("accepted web raw bytes were written by Python")
    monkeypatch.setattr(Path, "write_bytes", forbidden_python_write)
    monkeypatch.setattr(Path, "write_text", forbidden_python_write)
    path = ingest.ingest_url("https://fixture.invalid/source", tmp_path)
    trace = _native.last_trace()
    assert trace["core"] == "rust" and trace["operation"] == "compat_kb_raw_write"
    assert path == tmp_path / "raw/test-article/index.md"
    post = frontmatter.load(path)
    assert post["source"] == "https://fixture.invalid/source"
    assert post["compiled"] is False
    assert post["type"] == "web_article"
    original = (path.parent / "source.original").read_bytes()
    assert original.startswith(b"<html><title>Test Article")
    assert post["original_file"] == "source.original"
    assert post["original_sha256"] == hashlib.sha256(original).hexdigest()
    assert post["original_media_type"] == "text/html"
    name = hashlib.md5(b"https://fixture.invalid/final/images/a.png").hexdigest()[:8] + ".png"
    assert (path.parent / "images" / name).read_bytes() == image
    assert "images/" + name in post.content
    assert "http://127.0.0.1:9/secret" in validations
    assert "http://127.0.0.1:9/secret" not in calls
    assert hooks[0][0] == ("ingested",)
    assert hooks[0][1]["path"] == str(path)


def test_hook_redirect_is_revalidated_before_second_callback(tmp_path, monkeypatch):
    calls = []
    original = ingest._DEFAULT_VALIDATE_URL
    monkeypatch.setattr(ingest, "_validate_url", lambda url: None if url.startswith("https://fixture.invalid/") else original(url))
    def callback(url, **kwargs):
        calls.append(url)
        response = Response(b"")
        response.status_code = 302
        response.headers["location"] = "http://169.254.169.254/latest/meta-data/"
        return response
    monkeypatch.setattr(ingest.requests, "get", callback)
    with pytest.raises(ValueError, match="not public"):
        ingest.ingest_url("https://fixture.invalid/source", tmp_path)
    assert calls == ["https://fixture.invalid/source"]
    assert not list(tmp_path.rglob("*.md"))


def test_url_refresh_retains_exact_original_history_without_indexing_unrelated_files(tmp_path, monkeypatch):
    import sqlite3
    raw = tmp_path / "raw"
    raw.mkdir()
    malformed = raw / "malformed.md"
    malformed.write_bytes(b"---\ntitle: [broken\n---\nunchanged")
    monkeypatch.setattr(ingest, "_validate_url", lambda url: None)
    versions = [b"<html><title>Stable</title><body>version one</body></html>",
                b"<html><title>Stable</title><body>version two</body></html>"]
    for original in versions:
        monkeypatch.setattr(ingest.requests, "get", lambda *a, **kw: Response(original))
        path = ingest.ingest_url("https://fixture.invalid/stable", tmp_path)
        assert (path.parent / "source.original").read_bytes() == original
        assert frontmatter.load(path)["original_sha256"] == hashlib.sha256(original).hexdigest()
    assert malformed.read_bytes() == b"---\ntitle: [broken\n---\nunchanged"
    database = tmp_path / "wiki/_meta/llmbase.sqlite3"
    with sqlite3.connect(f"file:{database}?mode=ro", uri=True) as connection:
        intents = connection.execute("SELECT content, status FROM write_intents WHERE relative_path = ?",
                                     ("stable/source.original",)).fetchall()
        assert sorted(row[0] for row in intents) == sorted(versions)
        assert {row[1] for row in intents} == {"applied"}
        assert connection.execute("SELECT count(*) FROM sources").fetchone()[0] == 0
