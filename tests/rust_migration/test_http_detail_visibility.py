"""Bounded detail reads must use live target metadata, without a library scan."""

import json
import os
import re

import frontmatter
import pytest

from llmwiki import http_auth, web


BODY = "## PUBLIC_HEADING\n\nPUBLIC_DETAIL_BODY"


@pytest.fixture(autouse=True)
def explicit_environment(monkeypatch):
    for name in ("LLMBASE_API_SECRET", "LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "PORT"):
        monkeypatch.delenv(name, raising=False)


@pytest.fixture
def no_snapshot(monkeypatch):
    def forbidden(*args, **kwargs):
        pytest.fail("a bounded detail request scanned the whole library")

    # The factory captures this import, so patch before creating the app.
    monkeypatch.setattr(http_auth, "visibility_snapshot", forbidden)


def write_document(path, metadata=None, body=BODY):
    path.parent.mkdir(parents=True, exist_ok=True)
    post = frontmatter.Post(body)
    post.metadata.update({"title": "Detail title", **(metadata or {})})
    path.write_text(frontmatter.dumps(post), encoding="utf-8")
    return path


def seed_targets(base, metadata=None):
    paths = (write_document(base / "wiki/concepts/detail.md", metadata),
             write_document(base / "raw/detail/index.md", metadata))
    meta = base / "wiki/_meta"
    meta.mkdir(parents=True, exist_ok=True)
    (meta / "aliases.json").write_text(json.dumps({"detail-alias": "detail"}))
    return paths


def article_urls(slug):
    return [f"/api/articles/{slug}", f"/api/articles/{slug}/sections"]


DETAIL_URLS = article_urls("detail") + article_urls("detail-alias") + ["/api/sources/detail"]


def assert_denied(response):
    assert 400 <= response.status_code < 600
    assert "PUBLIC_" not in response.get_data(as_text=True)
    assert "Detail title" not in response.get_data(as_text=True)


def test_public_detail_bodies_head_and_options_need_no_snapshot(tmp_kb, no_snapshot):
    seed_targets(tmp_kb)
    concepts = tmp_kb / "wiki/concepts"
    write_document(concepts / "safe-link.md", {"title": "Safe backlink"})
    write_document(concepts / "hidden-link.md", {"title": "PRIVATE_LINK_TITLE", "private": True})
    (tmp_kb / "wiki/_meta/backlinks.json").write_text(json.dumps({"detail": ["safe-link", "hidden-link"]}))
    client = web.create_web_app(tmp_kb).test_client()

    expected_article = {"status": "ok", "slug": "detail", "title": "Detail title",
                        "summary": "", "tags": [], "sources": [], "content": BODY,
                        "backlinks": [{"slug": "safe-link", "title": "Safe backlink"}]}
    for slug in ("detail", "detail-alias"):
        assert client.get(f"/api/articles/{slug}").json == expected_article
        response = client.get(f"/api/articles/{slug}/sections")
        assert response.status_code == 200
        assert {key: response.json[key] for key in ("status", "slug", "title")} == {
            "status": "ok", "slug": "detail", "title": "Detail title"}
        sections = response.json["sections"]
        assert len(sections) == 1
        assert re.fullmatch(r"h2-PUBLICHEADING-[0-9a-f]{6}", sections[0]["anchor"])
        assert {key: value for key, value in sections[0].items() if key != "anchor"} == {
            "level": 2, "title": "PUBLIC_HEADING", "start": 0, "end": len(BODY), "children": []}

    raw = client.get("/api/sources/detail")
    assert raw.status_code == 200
    assert raw.json == {"slug": "detail", "title": "Detail title", "type": "unknown",
                        "compiled": False, "content": BODY, "metadata": {"title": "Detail title"}}
    for url in DETAIL_URLS:
        assert client.get(url).status_code == 200
        head = client.head(url)
        assert head.status_code == 200 and head.data == b""
    # The article URL also has a DELETE rule; Flask may select that endpoint
    # for automatic OPTIONS, which must retain the mutation visibility gate.
    assert client.options("/api/articles/detail/sections").status_code == 200


def test_warmed_details_recheck_privacy_without_reindex_or_stat_change(tmp_kb, no_snapshot):
    paths = seed_targets(tmp_kb, {"private": False})
    app = web.create_web_app(tmp_kb)
    client = app.test_client()
    for url in DETAIL_URLS:
        assert client.get(url).status_code == 200

    for path in paths:
        before = path.stat()
        original = path.read_text()
        changed = original.replace("private: false", "private: true ")
        assert changed != original and len(changed.encode()) == before.st_size
        path.write_text(changed)
        os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
        assert path.stat().st_mtime_ns == before.st_mtime_ns
    for url in DETAIL_URLS:
        assert_denied(client.get(url))
        assert_denied(client.head(url))

    app.config["llmbase"]["token_scopes"] = {"private-fixture": ("read", "private")}
    for url in DETAIL_URLS:
        response = client.get(url, headers={"Authorization": "Bearer private-fixture"})
        assert response.status_code == 200
        assert "PUBLIC_" in response.get_data(as_text=True)
        assert response.headers["Cache-Control"] == "private, no-store"


def test_detail_insertions_and_deletions_are_live(tmp_kb, no_snapshot):
    app = web.create_web_app(tmp_kb)
    client = app.test_client()

    def assert_missing():
        for url in DETAIL_URLS:
            response = client.get(url)
            assert "PUBLIC_" not in response.get_data(as_text=True)
            # Raw previews historically return 200 with status=error when absent.
            assert response.status_code == 404 or response.json.get("status") == "error"

    assert_missing()
    paths = seed_targets(tmp_kb)
    for url in DETAIL_URLS:
        assert client.get(url).status_code == 200
        assert "PUBLIC_" in client.get(url).get_data(as_text=True)
    for path in paths:
        path.unlink()
    assert_missing()


@pytest.mark.parametrize("metadata, public", [
    ({}, True), ({"private": None}, True), ({"private": False}, True),
    ({"visibility": "public"}, True),
    *[({"private": value}, False) for value in (True, 0, "false", "", [], {})],
    *[({"visibility": value}, False) for value in (None, False, "", "PUBLIC", 0, [], {}, "internal")],
    ({"private": "false", "visibility": "public"}, False),
])
def test_detail_metadata_uses_fail_closed_core_policy(tmp_kb, no_snapshot, metadata, public):
    seed_targets(tmp_kb, metadata)
    client = web.create_web_app(tmp_kb).test_client()
    for url in DETAIL_URLS:
        response = client.get(url)
        if public:
            assert response.status_code == 200
            assert "PUBLIC_" in response.get_data(as_text=True)
        else:
            assert_denied(response)


def test_unsafe_targets_and_backlinks_cannot_disclose_documents(tmp_kb, no_snapshot):
    seed_targets(tmp_kb)
    concepts = tmp_kb / "wiki/concepts"
    raw = tmp_kb / "raw"
    outside = tmp_kb / "external"
    write_document(outside / "item.md")
    write_document(outside / "book/index.md")
    write_document(concepts / "nested/item.md")
    write_document(raw / "nested/book/index.md")
    write_document(concepts / "hidden.md", {"private": True})
    write_document(raw / "hidden/index.md", {"private": True})
    malformed = "---\nprivate: [unterminated\n---\n" + BODY
    (concepts / "malformed.md").write_text(malformed)
    (raw / "malformed").mkdir()
    (raw / "malformed/index.md").write_text(malformed)

    (concepts / "outside-link.md").symlink_to(outside / "item.md")
    (concepts / "inside-link.md").symlink_to(concepts / "detail.md")
    (concepts / "inside-dir").symlink_to(concepts / "nested", target_is_directory=True)
    (concepts / "outside-dir").symlink_to(outside, target_is_directory=True)
    for name, target in (("outside-link", outside / "item.md"),
                         ("inside-link", raw / "detail/index.md")):
        (raw / name).mkdir()
        (raw / name / "index.md").symlink_to(target)
    (raw / "inside-dir").symlink_to(raw / "nested", target_is_directory=True)
    (raw / "outside-dir").symlink_to(outside, target_is_directory=True)

    bad_articles = ["outside-link", "inside-link", "inside-dir/item", "outside-dir/item",
                    "../../external/item", "hidden", "malformed"]
    aliases = {"detail-alias": "detail", "escape-alias": "../../external/item",
               "link-alias": "inside-link", "malformed-alias": "malformed"}
    (tmp_kb / "wiki/_meta/aliases.json").write_text(json.dumps(aliases))
    write_document(concepts / "safe-link.md", {"title": "Safe backlink"})
    (tmp_kb / "wiki/_meta/backlinks.json").write_text(json.dumps({"detail": ["safe-link", *bad_articles]}))
    client = web.create_web_app(tmp_kb).test_client()

    urls = [url for slug in [*bad_articles, "escape-alias", "link-alias", "malformed-alias"]
            for url in article_urls(slug)]
    urls += [f"/api/sources/{slug}" for slug in (
        "outside-link", "inside-link", "inside-dir/book", "outside-dir/book",
        "../external/book", "hidden", "malformed")]
    for url in urls:
        assert_denied(client.get(url))
    response = client.get("/api/articles/detail")
    assert response.status_code == 200
    assert response.json["backlinks"] == [{"slug": "safe-link", "title": "Safe backlink"}]


def test_opaque_reads_and_mutations_keep_the_snapshot_gate(tmp_kb, monkeypatch):
    seed_targets(tmp_kb, {"private": True})
    original_snapshot = http_auth.visibility_snapshot
    calls = []

    def unavailable(*args, **kwargs):
        calls.append("unavailable")
        raise RuntimeError("snapshot unavailable")

    monkeypatch.setattr(http_auth, "visibility_snapshot", unavailable)
    client = web.create_web_app(tmp_kb).test_client()
    assert client.get("/api/stats").status_code == 503
    assert client.delete("/api/articles/kong").status_code == 503
    assert calls == ["unavailable", "unavailable"]

    def observed(*args, **kwargs):
        calls.append("live")
        return original_snapshot(*args, **kwargs)

    monkeypatch.setattr(http_auth, "visibility_snapshot", observed)
    app = web.create_web_app(tmp_kb)
    app.config["llmbase"]["token_scopes"] = {"writer-fixture": ("read", "write")}
    client = app.test_client()
    denied = client.get("/api/stats")
    assert denied.status_code == 401 and denied.json["required_scopes"] == ["private"]
    denied = client.delete("/api/articles/kong", headers={"Authorization": "Bearer writer-fixture"})
    assert denied.status_code == 403 and denied.json["required_scopes"] == ["private"]
    assert calls[-2:] == ["live", "live"]
    assert (tmp_kb / "wiki/concepts/kong.md").exists()


def test_replaced_detail_view_cannot_inherit_the_scan_exemption(tmp_kb, monkeypatch):
    def unavailable(*args, **kwargs):
        raise RuntimeError("snapshot unavailable")

    def replacement(slug):
        pytest.fail("an arbitrary replacement inherited bounded-read authority")

    monkeypatch.setattr(http_auth, "visibility_snapshot", unavailable)
    app = web.create_web_app(tmp_kb)
    app.view_functions["api_article"] = replacement
    assert app.test_client().get("/api/articles/kong").status_code == 503


def test_separate_apps_do_not_share_a_slug_visibility_decision(tmp_kb, no_snapshot):
    other = tmp_kb / "other-library"
    seed_targets(tmp_kb)
    seed_targets(other, {"private": True})
    public = web.create_web_app(tmp_kb).test_client()
    private = web.create_web_app(other).test_client()
    for url in DETAIL_URLS:
        assert public.get(url).status_code == 200
        assert_denied(private.get(url))
        assert public.get(url).status_code == 200
