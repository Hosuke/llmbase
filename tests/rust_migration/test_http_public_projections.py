"""Explicit public projection registrations bind a reviewed Flask pipeline."""

from functools import wraps
import json
import os

import frontmatter
from flask import Flask, request
import pytest

from llmwiki import http_auth, operations, web


PUBLIC_BODY = "## PUBLIC_HEADING\n\nPUBLIC_PROJECTION_BODY"
PRIVATE_MARKER = "PRIVATE_PROJECTION_SENTINEL"


@pytest.fixture(autouse=True)
def explicit_environment(monkeypatch):
    for name in ("LLMBASE_API_SECRET", "LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "PORT"):
        monkeypatch.delenv(name, raising=False)


@pytest.fixture
def no_snapshot(monkeypatch):
    def forbidden(*args, **kwargs):
        pytest.fail("an explicitly registered projection reached the library snapshot")

    # create_web_app captures this import, so patch before constructing any app.
    monkeypatch.setattr(http_auth, "visibility_snapshot", forbidden)


def write_document(path, *, private=False, title="Public projection", body=PUBLIC_BODY):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(frontmatter.dumps(frontmatter.Post(body, title=title, private=private)))
    return path


def seed(base, *, all_private=False):
    public = write_document(base / "wiki/concepts/projected.md", private=all_private)
    write_document(base / "wiki/concepts/hidden.md", private=True,
                   title=PRIVATE_MARKER, body=PRIVATE_MARKER)
    raw = write_document(base / "raw/projected/index.md", private=all_private)
    meta = base / "wiki/_meta"
    meta.mkdir(parents=True, exist_ok=True)
    (meta / "aliases.json").write_text(json.dumps({"projection-alias": "projected"}))
    return public, raw


def filtered_rows(base):
    # This tiny host-owned projection only covers two known fixture documents.
    # Its permission decision uses the same post that it serializes.
    rows = []
    for slug in ("projected", "hidden"):
        post = frontmatter.load(base / f"wiki/concepts/{slug}.md")
        if post.metadata.get("private") is False:
            rows.append({"slug": slug, "title": post["title"], "content": post.content})
    return rows


def passthrough(view):
    @wraps(view)
    def wrapped(*args, **kwargs):
        return view(*args, **kwargs)
    return wrapped


def make_private_preserving_stat(path):
    before = path.stat()
    original = path.read_text()
    changed = original.replace("private: false", "private: true ")
    assert changed != original and len(changed.encode()) == before.st_size
    path.write_text(changed)
    os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
    assert path.stat().st_size == before.st_size
    assert path.stat().st_mtime_ns == before.st_mtime_ns


def test_filtered_stats_entities_and_export_are_explicit_public_projections(tmp_kb, no_snapshot):
    seed(tmp_kb)
    app = web.create_web_app(tmp_kb)
    observed_principals = []

    def stats():
        observed_principals.append(http_auth.principal())
        return {"articles": len(filtered_rows(tmp_kb))}

    def entities():
        return {"entities": [{"name": row["title"]} for row in filtered_rows(tmp_kb)]}

    def export(slug):
        for row in filtered_rows(tmp_kb):
            if row["slug"] == slug:
                return row["content"], 200, {"Content-Type": "text/markdown"}
        return {"status": "error", "message": "Not found"}, 404

    for endpoint, view in (("api_stats", stats), ("api_entities", entities),
                           ("api_export_article", export)):
        web.register_public_read_projection(app, endpoint, view)
    client = app.test_client()
    expected = {"/api/stats": {"articles": 1},
                "/api/entities": {"entities": [{"name": "Public projection"}]}}
    for url, body in expected.items():
        response = client.get(url)
        assert response.status_code == 200 and response.json == body
        assert "no-store" in response.headers["Cache-Control"]
        assert PRIVATE_MARKER not in response.get_data(as_text=True)
        assert client.head(url).status_code == 200
    response = client.get("/api/export/article/projected")
    assert response.status_code == 200 and response.get_data(as_text=True) == PUBLIC_BODY
    assert "no-store" in response.headers["Cache-Control"]
    hidden = client.get("/api/export/article/hidden")
    assert hidden.status_code == 404 and PRIVATE_MARKER not in hidden.get_data(as_text=True)
    assert all(p["kind"] == "visitor" and p["scopes"] == {"read"} for p in observed_principals)


def test_unregistered_opaque_routes_and_mutations_keep_existing_authority(tmp_kb, monkeypatch):
    seed(tmp_kb)

    def forbidden(*args, **kwargs):
        pytest.fail("public projection registration granted model or write authority")

    monkeypatch.setattr(web, "query", forbidden)
    monkeypatch.setattr(operations, "dispatch", forbidden)
    app = web.create_web_app(tmp_kb)
    web.register_public_read_projection(app, "api_stats", lambda: {"articles": 1})
    client = app.test_client()
    for url in ("/api/entities", "/api/export/article/projected"):
        denied = client.get(url)
        assert denied.status_code == 401 and denied.json["required_scopes"] == ["private"]
        assert PRIVATE_MARKER not in denied.get_data(as_text=True)
    for path, body in (("/api/ask", {"question": "q", "file_back": False}), ("/api/compile", {})):
        assert client.post(path, json=body).status_code == 401
    assert client.delete("/api/articles/projected").status_code == 401
    assert (tmp_kb / "wiki/concepts/projected.md").exists()


def test_wraps_passthrough_requires_explicit_registration(tmp_kb, monkeypatch, no_snapshot):
    seed(tmp_kb)
    scans = []

    def unavailable(*args, **kwargs):
        scans.append(True)
        raise RuntimeError("snapshot unavailable")

    with monkeypatch.context() as scoped:
        scoped.setattr(http_auth, "visibility_snapshot", unavailable)
        unregistered = web.create_web_app(tmp_kb)
    unregistered.view_functions["api_article"] = passthrough(unregistered.view_functions["api_article"])
    assert unregistered.test_client().get("/api/articles/projected").status_code == 503
    assert scans == [True]

    registered = web.create_web_app(tmp_kb)
    wrapped = passthrough(registered.view_functions["api_article"])
    web.register_public_read_projection(registered, "api_article", wrapped)
    client = registered.test_client()
    assert client.get("/api/articles/projected").json["content"] == PUBLIC_BODY
    denied = client.get("/api/articles/hidden")
    assert denied.status_code == 404 and PRIVATE_MARKER not in denied.get_data(as_text=True)


@pytest.mark.parametrize("endpoint", ["api_ask", "api_compile", "api_delete_article", "custom_read"])
def test_registration_rejects_model_write_and_arbitrary_endpoints(tmp_kb, endpoint):
    app = web.create_web_app(tmp_kb)
    app.add_url_rule("/api/custom-read", endpoint="custom_read", view_func=lambda: {"ok": True})
    original = app.view_functions[endpoint]
    with pytest.raises(ValueError):
        web.register_public_read_projection(app, endpoint, lambda: {"ok": True})
    assert app.view_functions[endpoint] is original


def test_registration_rejects_noncallable_duplicate_and_late_setup(tmp_kb):
    app = web.create_web_app(tmp_kb)
    original = app.view_functions["api_stats"]
    with pytest.raises((TypeError, ValueError)):
        web.register_public_read_projection(app, "api_stats", None)
    assert app.view_functions["api_stats"] is original
    view = lambda: {"articles": 0}
    web.register_public_read_projection(app, "api_stats", view)
    with pytest.raises(ValueError):
        web.register_public_read_projection(app, "api_stats", view)
    assert app.view_functions["api_stats"] is view
    assert app.test_client().get("/api/healthz").status_code == 200
    with pytest.raises(ValueError):
        web.register_public_read_projection(app, "api_entities", lambda: {"entities": []})


@pytest.mark.parametrize("changed", ["view", "before", "after"])
def test_changed_registered_pipeline_fails_closed(tmp_kb, no_snapshot, changed):
    seed(tmp_kb)
    app = web.create_web_app(tmp_kb)

    def projection():
        return {"articles": len(filtered_rows(tmp_kb))}

    web.register_public_read_projection(app, "api_stats", projection)
    if changed == "view":
        # Even __wrapped__ pointing at the registered callable carries no grant.
        app.view_functions["api_stats"] = passthrough(projection)
    elif changed == "before":
        app.before_request(lambda: {"leak": PRIVATE_MARKER})
    else:
        def changed_after(response):
            response.set_data(PRIVATE_MARKER)
            response.status_code = 200
            return response
        app.after_request(changed_after)
    response = app.test_client().get("/api/stats")
    assert response.status_code == 503
    assert PRIVATE_MARKER not in response.get_data(as_text=True)


def test_conditional_reads_recheck_live_builtin_targets_aliases_and_backlinks(tmp_kb, no_snapshot):
    targets = seed(tmp_kb)
    backlink = write_document(tmp_kb / "wiki/concepts/backlink.md", title="Public backlink")
    (tmp_kb / "wiki/_meta/backlinks.json").write_text(json.dumps({"projected": ["backlink", "hidden"]}))
    app = web.create_web_app(tmp_kb)
    seen_headers = []

    @app.before_request
    def conditional_hook():
        pair = (request.headers.get("If-None-Match"), request.headers.get("If-Modified-Since"))
        seen_headers.append(pair)
        if any(pair):
            return "", 304

    def cached_passthrough(original):
        @wraps(original)
        def projection(*args, **kwargs):
            response = app.make_response(original(*args, **kwargs))
            response.set_etag("same-stat")
            response.headers["Last-Modified"] = "Sun, 11 Oct 2026 00:00:00 GMT"
            response.headers["Cache-Control"] = "public, max-age=3600"
            return response
        return projection

    for endpoint in ("api_article", "api_article_sections", "api_source_detail"):
        web.register_public_read_projection(app, endpoint, cached_passthrough(app.view_functions[endpoint]))
    client = app.test_client()
    urls = ["/api/articles/projected", "/api/articles/projection-alias",
            "/api/articles/projected/sections", "/api/articles/projection-alias/sections",
            "/api/sources/projected"]
    for url in urls:
        response = client.get(url)
        assert response.status_code == 200 and "PUBLIC_" in response.get_data(as_text=True)
        assert "no-store" in response.headers["Cache-Control"]
    assert client.get(urls[0]).json["backlinks"] == [{"slug": "backlink", "title": "Public backlink"}]

    headers = {"If-None-Match": '"same-stat"', "If-Modified-Since": "Sun, 11 Oct 2026 00:00:00 GMT"}
    make_private_preserving_stat(backlink)
    updated = client.get(urls[0], headers=headers)
    assert updated.status_code == 200 and updated.json["backlinks"] == []
    for path in targets:
        make_private_preserving_stat(path)
    for url in urls:
        denied = client.get(url, headers=headers)
        assert denied.status_code == 404
        assert "PUBLIC_" not in denied.get_data(as_text=True)
        assert "no-store" in denied.headers["Cache-Control"]
    assert seen_headers and all(pair == (None, None) for pair in seen_headers)


def test_registrations_and_views_are_isolated_between_apps(tmp_kb, no_snapshot):
    other = tmp_kb / "other-library"
    seed(tmp_kb)
    seed(other, all_private=True)
    first, second = web.create_web_app(tmp_kb), web.create_web_app(other)
    web.register_public_read_projection(first, "api_stats", lambda: {"articles": len(filtered_rows(tmp_kb))})
    web.register_public_read_projection(second, "api_stats", lambda: {"articles": len(filtered_rows(other))})
    public, hidden = first.test_client(), second.test_client()
    assert public.get("/api/stats").json == {"articles": 1}
    assert hidden.get("/api/stats").json == {"articles": 0}
    assert public.get("/api/stats").json == {"articles": 1}


def test_registration_rejects_an_endpoint_with_a_second_mutating_rule_atomically(tmp_kb):
    seed(tmp_kb)
    app = web.create_web_app(tmp_kb)
    original = app.view_functions["api_stats"]
    app.add_url_rule("/api/stats/submit", endpoint="api_stats", view_func=original, methods=["POST"])
    with pytest.raises(ValueError):
        web.register_public_read_projection(app, "api_stats", lambda: {"articles": 1})
    assert app.view_functions["api_stats"] is original
    # A rejected registration must leave the original mixed-library gate active.
    response = app.test_client().get("/api/stats")
    assert response.status_code == 401 and response.json["required_scopes"] == ["private"]


@pytest.mark.parametrize("change", ["additional_rule", "methods"])
def test_registered_route_rule_changes_fail_closed(tmp_kb, no_snapshot, change):
    app = web.create_web_app(tmp_kb)
    view = lambda: {"articles": 0}
    web.register_public_read_projection(app, "api_stats", view)
    if change == "additional_rule":
        app.add_url_rule("/api/another-stats", endpoint="api_stats", view_func=view, methods=["GET"])
    else:
        next(app.url_map.iter_rules("api_stats")).methods.add("POST")
    response = app.test_client().get("/api/stats")
    assert response.status_code == 503
    assert "no-store" in response.headers["Cache-Control"]


@pytest.mark.parametrize("kind", ["before", "after"])
@pytest.mark.parametrize("change", ["replacement", "removal", "reorder"])
def test_existing_hook_identity_and_order_changes_fail_closed(tmp_kb, no_snapshot, kind, change):
    app = web.create_web_app(tmp_kb)
    if kind == "before":
        first = lambda: None
        second = lambda: None
        replacement = lambda: {"leak": PRIVATE_MARKER}
        app.before_request(first)
        app.before_request(second)
        chain = app.before_request_funcs[None]
    else:
        def first(response):
            # This existing callback tries to turn a gate denial into success.
            if response.status_code != 200:
                response.set_data(PRIVATE_MARKER)
                response.status_code = 200
            return response
        second = lambda response: response
        replacement = lambda response: response
        app.after_request(first)
        app.after_request(second)
        chain = app.after_request_funcs[None]
    web.register_public_read_projection(app, "api_stats", lambda: {"articles": 0})
    if change == "replacement":
        chain[-1] = replacement
    elif change == "removal":
        chain.pop()
    else:
        chain[-2:] = [second, first]
    response = app.test_client().get("/api/stats")
    assert response.status_code == 503
    assert PRIVATE_MARKER not in response.get_data(as_text=True)
    assert "no-store" in response.headers["Cache-Control"]


def test_valid_after_hook_cannot_rewrite_a_missing_read_scope_denial(tmp_kb, no_snapshot):
    app = web.create_web_app(tmp_kb)
    app.config["llmbase"]["token_scopes"] = {"no-read-fixture": ("write",)}
    hook_statuses = []

    @app.after_request
    def rewrite_denial(response):
        hook_statuses.append(response.status_code)
        response.set_data(PRIVATE_MARKER)
        response.status_code = 200
        return response

    def forbidden():
        pytest.fail("a token lacking read scope reached the projection")

    web.register_public_read_projection(app, "api_stats", forbidden)
    response = app.test_client().get("/api/stats", headers={"Authorization": "Bearer no-read-fixture"})
    assert hook_statuses == [403]
    assert response.status_code == 403 and response.json["required_scopes"] == ["read"]
    assert PRIVATE_MARKER not in response.get_data(as_text=True)
    assert "no-store" in response.headers["Cache-Control"]


@pytest.mark.parametrize("origin", ["view", "after_hook"])
def test_valid_pipeline_unsolicited_304_fails_closed(tmp_kb, no_snapshot, origin):
    app = web.create_web_app(tmp_kb)
    if origin == "after_hook":
        @app.after_request
        def cached_response(response):
            response.status_code = 304
            return response
    status = 304 if origin == "view" else 200
    web.register_public_read_projection(app, "api_stats", lambda: ({"articles": 0}, status))
    response = app.test_client().get("/api/stats")
    assert response.status_code == 503
    assert "no-store" in response.headers["Cache-Control"]


def test_bare_flask_app_cannot_register_a_public_projection():
    app = Flask(__name__)
    original = lambda: {"original": True}
    app.add_url_rule("/api/stats", endpoint="api_stats", view_func=original)
    with pytest.raises(ValueError):
        web.register_public_read_projection(app, "api_stats", lambda: {"original": False})
    assert app.view_functions["api_stats"] is original
    assert app.test_client().get("/api/stats").json == {"original": True}
