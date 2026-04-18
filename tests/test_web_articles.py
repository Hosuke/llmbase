"""Tests for /api/articles pagination/filter/lite/etag (v0.6.4)."""

import json
from pathlib import Path

import frontmatter
import pytest

from tools.web import create_web_app


@pytest.fixture
def client(tmp_kb):
    app = create_web_app(tmp_kb)
    app.config["TESTING"] = True
    return app.test_client()


@pytest.fixture
def big_kb(tmp_kb):
    """tmp_kb + 50 extra synthetic concepts for pagination tests."""
    concepts_dir = tmp_kb / "wiki" / "concepts"
    for i in range(50):
        slug = f"zsynthetic-{i:03d}"
        post = frontmatter.Post(f"## English\n\nSynthetic article {i}.\n")
        post.metadata.update({
            "title": f"Synthetic {i}",
            "summary": f"Synthetic summary {i}",
            "tags": ["synthetic", "even" if i % 2 == 0 else "odd"],
            "created": "2026-04-01T00:00:00+00:00",
            "updated": "2026-04-01T00:00:00+00:00",
        })
        (concepts_dir / f"{slug}.md").write_text(frontmatter.dumps(post), encoding="utf-8")
    return tmp_kb


def _seed_index(base: Path):
    """Mimic compile.rebuild_index: write wiki/_meta/index.json from concepts/."""
    meta_dir = base / "wiki" / "_meta"
    concepts_dir = base / "wiki" / "concepts"
    meta_dir.mkdir(parents=True, exist_ok=True)
    entries = []
    for md in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md))
        entries.append({
            "slug": md.stem,
            "title": post.metadata.get("title", md.stem),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []),
        })
    (meta_dir / "index.json").write_text(json.dumps(entries), encoding="utf-8")
    return entries


def test_backward_compat_no_query(client):
    r = client.get("/api/articles")
    assert r.status_code == 200
    body = r.get_json()
    assert set(body.keys()) == {"articles"}
    assert isinstance(body["articles"], list)
    assert len(body["articles"]) == 3


def test_articles_pagination(big_kb):
    app = create_web_app(big_kb)
    c = app.test_client()
    r1 = c.get("/api/articles?limit=10")
    b1 = r1.get_json()
    assert b1["count"] == 10
    assert b1["total"] == 53  # 3 from tmp_kb + 50 synthetic
    assert b1["next_cursor"] is not None

    r2 = c.get(f"/api/articles?limit=10&cursor={b1['next_cursor']}")
    b2 = r2.get_json()
    s1 = {a["slug"] for a in b1["articles"]}
    s2 = {a["slug"] for a in b2["articles"]}
    assert not (s1 & s2)
    assert b2["count"] == 10


def test_next_cursor_null_on_exact_last_page(client):
    """limit == remaining → next_cursor must be null, not point past the end."""
    # tmp_kb has exactly 3 articles.
    r = client.get("/api/articles?limit=3")
    body = r.get_json()
    assert body["count"] == 3
    assert body["next_cursor"] is None


def test_articles_tag_filter(client):
    r = client.get("/api/articles?tag=buddhism")
    body = r.get_json()
    assert body["count"] == 2
    for a in body["articles"]:
        assert "buddhism" in a["tags"]
    assert body["filters"]["tag"] == "buddhism"


def test_articles_q_filter(client):
    r = client.get("/api/articles?q=emptiness")
    body = r.get_json()
    assert body["count"] == 1
    assert body["articles"][0]["slug"] == "kong"


def test_articles_fields_selector(client):
    r = client.get("/api/articles?limit=10&fields=slug,title")
    body = r.get_json()
    for a in body["articles"]:
        assert set(a.keys()) == {"slug", "title"}


def test_articles_limit_bounds(client):
    assert client.get("/api/articles?limit=0").status_code == 400
    assert client.get("/api/articles?limit=1001").status_code == 400
    assert client.get("/api/articles?limit=abc").status_code == 400


def test_bad_limit_beats_etag(client, tmp_kb):
    """Invalid limit must 400 even when If-None-Match would otherwise 304."""
    # '*' always matches — if ordering were wrong, this would 304.
    r = client.get("/api/articles?limit=abc", headers={"If-None-Match": "*"})
    assert r.status_code == 400


def test_articles_lite_requires_index(client, tmp_kb):
    # No index.json yet → empty + 200.
    r = client.get("/api/articles/lite")
    assert r.status_code == 200
    assert r.get_json() == {"articles": [], "total": 0}

    _seed_index(tmp_kb)
    r2 = client.get("/api/articles/lite")
    body = r2.get_json()
    assert body["total"] == 3
    for a in body["articles"]:
        assert set(a.keys()) == {"slug", "title"}


def test_articles_etag_304(client, tmp_kb):
    _seed_index(tmp_kb)
    r1 = client.get("/api/articles/lite")
    etag = r1.headers.get("ETag")
    assert etag
    r2 = client.get("/api/articles/lite", headers={"If-None-Match": etag})
    assert r2.status_code == 304
    assert r2.data == b""
    # 304 must echo validators (RFC 7232 §4.1)
    assert r2.headers.get("ETag") == etag
    assert r2.headers.get("Cache-Control") == "no-cache"


def test_articles_etag_invalidates_on_rename(client, tmp_kb):
    """Renaming a file (same mtime/size) must change the ETag for /api/articles."""
    import os, time
    concepts_dir = tmp_kb / "wiki" / "concepts"
    e_before = client.get("/api/articles").headers.get("ETag")
    assert e_before
    src = concepts_dir / "ren.md"
    dst = concepts_dir / "ren-renamed.md"
    st = src.stat()
    dst.write_bytes(src.read_bytes())
    src.unlink()
    # Restore mtime so the test exercises the rename-with-same-mtime case.
    os.utime(dst, (st.st_atime, st.st_mtime))
    e_after = client.get("/api/articles").headers.get("ETag")
    assert e_after and e_after != e_before


def test_articles_etag_distinct_for_query(client, tmp_kb):
    _seed_index(tmp_kb)
    e_full = client.get("/api/articles").headers.get("ETag")
    e_tag = client.get("/api/articles?tag=buddhism").headers.get("ETag")
    assert e_full and e_tag and e_full != e_tag


def test_tag_filter_no_substring_match(tmp_kb):
    """Single-string tags must match exactly, not substring."""
    concepts_dir = tmp_kb / "wiki" / "concepts"
    post = frontmatter.Post("body")
    post.metadata.update({
        "title": "X", "summary": "x", "tags": "buddhism",  # string, not list
        "created": "2026-04-01T00:00:00+00:00",
        "updated": "2026-04-01T00:00:00+00:00",
    })
    (concepts_dir / "string-tag.md").write_text(frontmatter.dumps(post), encoding="utf-8")
    app = create_web_app(tmp_kb); c = app.test_client()
    # 'bud' is a substring of 'buddhism' but must NOT match
    r = c.get("/api/articles?tag=bud")
    assert r.get_json()["count"] == 0
    # exact match still works
    r2 = c.get("/api/articles?tag=buddhism")
    slugs = {a["slug"] for a in r2.get_json()["articles"]}
    assert "string-tag" in slugs


def test_etag_wildcard_and_weak_strong(client, tmp_kb):
    _seed_index(tmp_kb)
    r1 = client.get("/api/articles/lite")
    etag = r1.headers.get("ETag")
    assert etag and etag.startswith('W/"')
    # '*' must always 304
    assert client.get("/api/articles/lite", headers={"If-None-Match": "*"}).status_code == 304
    # comma-separated list including the etag
    multi = f'W/"deadbeef00000000", {etag}'
    assert client.get("/api/articles/lite", headers={"If-None-Match": multi}).status_code == 304
    # strong form of a weak tag still matches
    strong = etag[2:]  # drop W/
    assert client.get("/api/articles/lite", headers={"If-None-Match": strong}).status_code == 304


def test_lite_route_not_shadowed_by_path_slug(client, tmp_kb):
    """Static /api/articles/lite must beat /api/articles/<path:slug>."""
    _seed_index(tmp_kb)
    r = client.get("/api/articles/lite")
    body = r.get_json()
    assert "articles" in body and "total" in body
    # If shadowed, would 404 (no article slug "lite") or return article shape.
    assert "content" not in body


# ─── v0.7.2: lite ?tag= filter + LLMBASE_LITE_CACHE_MAX_AGE ──────────


def test_lite_tag_filter_narrows(client, tmp_kb):
    _seed_index(tmp_kb)
    r = client.get("/api/articles/lite?tag=buddhism")
    body = r.get_json()
    assert body["total"] == 2
    slugs = {a["slug"] for a in body["articles"]}
    assert slugs == {"kong", "si-di"}
    # Lite shape unchanged: only slug + title.
    for a in body["articles"]:
        assert set(a.keys()) == {"slug", "title"}


def test_lite_tag_unknown_returns_empty_200(client, tmp_kb):
    """Parity with /api/articles?tag=nonexistent — empty list, not 404.

    Validating tag existence would require loading taxonomy, which lite avoids
    on principle. The downstream caller can detect empty and decide what to do.
    """
    _seed_index(tmp_kb)
    r = client.get("/api/articles/lite?tag=nonexistent")
    assert r.status_code == 200
    assert r.get_json() == {"articles": [], "total": 0}


def test_lite_no_tag_returns_full_list(client, tmp_kb):
    """Backwards compat: omitting ?tag must match v0.6.4-0.7.1 shape exactly."""
    _seed_index(tmp_kb)
    r = client.get("/api/articles/lite")
    body = r.get_json()
    assert body["total"] == 3
    assert {a["slug"] for a in body["articles"]} == {"kong", "si-di", "ren"}


def test_lite_etag_distinct_per_tag(client, tmp_kb):
    """Different tag slices must get distinct ETags — otherwise a 304 would
    hand back stale partial data to the wrong caller."""
    _seed_index(tmp_kb)
    e_full = client.get("/api/articles/lite").headers.get("ETag")
    e_buddhism = client.get("/api/articles/lite?tag=buddhism").headers.get("ETag")
    e_confucianism = client.get("/api/articles/lite?tag=confucianism").headers.get("ETag")
    assert e_full and e_buddhism and e_confucianism
    assert len({e_full, e_buddhism, e_confucianism}) == 3


def test_lite_etag_304_respects_tag(client, tmp_kb):
    _seed_index(tmp_kb)
    r1 = client.get("/api/articles/lite?tag=buddhism")
    etag = r1.headers.get("ETag")
    # Same tag → 304.
    r2 = client.get("/api/articles/lite?tag=buddhism", headers={"If-None-Match": etag})
    assert r2.status_code == 304
    # Different tag → must NOT 304 with the buddhism etag (would serve stale slice).
    r3 = client.get("/api/articles/lite?tag=confucianism", headers={"If-None-Match": etag})
    assert r3.status_code == 200


def test_lite_cache_default_no_cache(client, tmp_kb, monkeypatch):
    monkeypatch.delenv("LLMBASE_LITE_CACHE_MAX_AGE", raising=False)
    _seed_index(tmp_kb)
    r = client.get("/api/articles/lite")
    assert r.headers.get("Cache-Control") == "no-cache"


def test_lite_cache_max_age_env_opts_in(tmp_kb, monkeypatch):
    """LLMBASE_LITE_CACHE_MAX_AGE switches Cache-Control to public, max-age=N."""
    monkeypatch.setenv("LLMBASE_LITE_CACHE_MAX_AGE", "600")
    app = create_web_app(tmp_kb)
    c = app.test_client()
    _seed_index(tmp_kb)
    r = c.get("/api/articles/lite")
    assert r.headers.get("Cache-Control") == "public, max-age=600"
    # ETag still present — max-age handles fast path, ETag handles freshness past TTL.
    assert r.headers.get("ETag")


def test_lite_cache_max_age_applies_to_304(tmp_kb, monkeypatch):
    monkeypatch.setenv("LLMBASE_LITE_CACHE_MAX_AGE", "600")
    app = create_web_app(tmp_kb)
    c = app.test_client()
    _seed_index(tmp_kb)
    etag = c.get("/api/articles/lite").headers.get("ETag")
    r304 = c.get("/api/articles/lite", headers={"If-None-Match": etag})
    assert r304.status_code == 304
    assert r304.headers.get("Cache-Control") == "public, max-age=600"


def test_lite_cache_invalid_env_falls_back(tmp_kb, monkeypatch):
    """Garbage env value must NOT crash — falls back to no-cache."""
    monkeypatch.setenv("LLMBASE_LITE_CACHE_MAX_AGE", "not-a-number")
    app = create_web_app(tmp_kb)
    c = app.test_client()
    _seed_index(tmp_kb)
    r = c.get("/api/articles/lite")
    assert r.status_code == 200
    assert r.headers.get("Cache-Control") == "no-cache"


def test_lite_cache_zero_env_means_no_cache(tmp_kb, monkeypatch):
    monkeypatch.setenv("LLMBASE_LITE_CACHE_MAX_AGE", "0")
    app = create_web_app(tmp_kb)
    c = app.test_client()
    _seed_index(tmp_kb)
    r = c.get("/api/articles/lite")
    assert r.headers.get("Cache-Control") == "no-cache"


def test_lite_tag_normalizes_string_tags(tmp_kb):
    """A frontmatter tag stored as a single string (not a list) must still match."""
    concepts_dir = tmp_kb / "wiki" / "concepts"
    post = frontmatter.Post("body")
    post.metadata.update({
        "title": "Single-tag", "summary": "s",
        "tags": "buddhism",  # plain string, not a list
        "created": "2026-04-01T00:00:00+00:00",
        "updated": "2026-04-01T00:00:00+00:00",
    })
    (concepts_dir / "lone.md").write_text(frontmatter.dumps(post), encoding="utf-8")
    _seed_index(tmp_kb)
    app = create_web_app(tmp_kb); c = app.test_client()
    slugs = {a["slug"] for a in c.get("/api/articles/lite?tag=buddhism").get_json()["articles"]}
    assert "lone" in slugs
