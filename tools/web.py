"""Web UI server — serves React frontend + API endpoints."""

import hashlib
import hmac
import json
from email.utils import formatdate
from functools import wraps
from pathlib import Path

import frontmatter
from flask import Flask, current_app, request, jsonify, send_from_directory


def _kb_etag(meta_dir: Path, extra: str = "") -> tuple[str | None, str | None]:
    """Return (ETag, Last-Modified) derived from index.json mtime.

    *extra* mixes a stable per-request key (e.g. query string) into the ETag
    so distinct query shapes get distinct cache entries while still sharing
    the underlying KB-version signal.
    """
    idx = meta_dir / "index.json"
    try:
        st = idx.stat()
    except OSError:
        return None, None
    raw = f"{st.st_mtime}:{st.st_size}:{extra}".encode()
    etag = hashlib.md5(raw).hexdigest()[:16]
    return f'W/"{etag}"', formatdate(st.st_mtime, usegmt=True)


def _concepts_fingerprint(concepts_dir: Path) -> str:
    """Short hash of concepts/*.md (name+mtime+size) — for etags that depend
    on article content, not just the index."""
    h = hashlib.md5()
    try:
        entries = sorted(concepts_dir.glob("*.md"))
    except OSError:
        return ""
    for p in entries:
        try:
            st = p.stat()
        except OSError:
            continue
        h.update(f"{p.name}:{st.st_mtime}:{st.st_size}\n".encode())
    return h.hexdigest()[:16]


def _apply_kb_cache_headers(resp, etag: str | None, last_mod: str | None):
    if etag:
        resp.headers["ETag"] = etag
        if last_mod:
            resp.headers["Last-Modified"] = last_mod
        resp.headers["Cache-Control"] = "no-cache"
    return resp


def _not_modified(etag: str, last_mod: str | None):
    """Return a 304 carrying the validators (RFC 7232 §4.1)."""
    from flask import make_response
    resp = make_response("", 304)
    resp.headers["ETag"] = etag
    if last_mod:
        resp.headers["Last-Modified"] = last_mod
    resp.headers["Cache-Control"] = "no-cache"
    return resp


def _if_none_match_hits(header: str | None, etag: str) -> bool:
    """RFC 7232-aware match: handles '*', comma lists, and W/ vs strong tags."""
    if not header or not etag:
        return False
    h = header.strip()
    if h == "*":
        return True
    target = etag[2:] if etag.startswith("W/") else etag
    for raw in h.split(","):
        cand = raw.strip()
        if not cand:
            continue
        if cand.startswith("W/"):
            cand = cand[2:]
        if cand == target:
            return True
    return False


def _normalize_tags(value) -> list[str]:
    """Coerce frontmatter tags to a list[str]; tolerate string/None."""
    if value is None:
        return []
    if isinstance(value, str):
        return [value]
    if isinstance(value, (list, tuple)):
        return [str(t) for t in value]
    return [str(value)]

from .config import load_config, ensure_dirs
from .search import search
from .query import query, query_with_search
from .ingest import ingest_url, list_raw
from .compile import compile_new, rebuild_index
from .lint import lint


def derive_session_token(secret: str) -> str:
    """Derive a session cookie token from the API secret.

    This is a public interface so downstream projects that customise
    serve_spa or add their own auth middleware can generate the same
    cookie value without reverse-engineering the algorithm.

    Returns an empty string when *secret* is falsy.
    """
    if not secret:
        return ""
    import hashlib
    return hashlib.sha256(f"session:{secret}".encode()).hexdigest()[:48]


# ─── Customizable constants ──────────────────────────────────────
# Downstream can extend the web layer without forking.
#
# Two approaches (can be mixed):
#
# 1. EXTRA_ROUTES / hooks — fill these BEFORE calling create_web_app():
#
#     import tools.web as web
#     web.EXTRA_ROUTES.append(("/api/my-endpoint", my_handler, {"methods": ["GET"]}))
#     web.BEFORE_REQUEST_HOOKS.append(my_auth_middleware)
#     app = web.create_web_app(base_dir)
#
# 2. Flask Blueprints — register AFTER create_web_app() returns:
#
#     app = web.create_web_app(base_dir)
#     app.register_blueprint(my_blueprint, url_prefix="/api")
#
# Approach 1 is simpler; approach 2 is more powerful (middleware, error
# handlers, nested blueprints).  Both work.

# List of (rule, view_func, options_dict) tuples.
# Must be populated BEFORE create_web_app() is called.
EXTRA_ROUTES: list[tuple] = []

# Callables invoked as Flask before_request / after_request hooks.
# Must be populated BEFORE create_web_app() is called.
BEFORE_REQUEST_HOOKS: list = []
AFTER_REQUEST_HOOKS: list = []


def require_auth(f):
    """Decorator: protect a route when LLMBASE_API_SECRET is set.

    Module-level so EXTRA_ROUTES handlers and downstream blueprints can
    wrap their own views with the same auth as built-in write endpoints.
    Reads the secret/session-token from ``current_app.config["llmbase"]``
    populated by :func:`create_web_app`; when the secret is empty the
    decorator is a no-op (local/dev mode).

    Usage::

        from tools.web import require_auth

        @require_auth
        def my_handler():
            ...
    """
    @wraps(f)
    def decorated(*args, **kwargs):
        llm_cfg = current_app.config.get("llmbase", {})
        api_secret = llm_cfg.get("api_secret", "")
        session_token = llm_cfg.get("session_token", "")
        if not api_secret:
            return f(*args, **kwargs)
        auth = request.headers.get("Authorization", "").replace("Bearer ", "")
        cookie = request.cookies.get("llmbase_auth", "")
        if (hmac.compare_digest(auth, api_secret)
                or hmac.compare_digest(cookie, session_token)):
            return f(*args, **kwargs)
        return jsonify({"status": "error", "message": "Unauthorized"}), 401
    return decorated


def create_web_app(base_dir: Path | None = None):
    """Create the full web application."""
    import os

    base = Path(base_dir) if base_dir else Path.cwd()
    cfg = load_config(base)
    # static_dir: configurable via config.yaml web.static_dir
    # Must resolve under base_dir to prevent serving arbitrary filesystem paths.
    static_dir_cfg = cfg.get("web", {}).get("static_dir")
    if static_dir_cfg:
        static_dir = Path(static_dir_cfg)
        if not static_dir.is_absolute():
            static_dir = (base / static_dir_cfg).resolve()
        else:
            static_dir = static_dir.resolve()
        # Guard: must be under base (path-aware check)
        try:
            static_dir.relative_to(base.resolve())
        except ValueError:
            import logging as _log
            _log.getLogger("llmbase.web").warning(
                f"web.static_dir '{static_dir}' is outside project root, ignoring")
            static_dir = base / "static" / "dist"
    else:
        static_dir = base / "static" / "dist"

    app = Flask(__name__, static_folder=None)

    # ─── Auth middleware for write endpoints ───────────────────
    # Auto-generate a secret if not set and running in production (PORT env = cloud deploy)
    API_SECRET = os.getenv("LLMBASE_API_SECRET", "")
    if not API_SECRET and os.getenv("PORT"):
        import secrets
        API_SECRET = secrets.token_urlsafe(32)
        os.environ["LLMBASE_API_SECRET"] = API_SECRET
        import logging
        logging.getLogger("llmbase.auth").info(f"Auto-generated API secret: {API_SECRET[:8]}...")

    # Generate a session token derived from the secret (never expose the secret itself)
    SESSION_TOKEN = derive_session_token(API_SECRET)

    # Publish runtime state for extension handlers (EXTRA_ROUTES, blueprints,
    # middleware) and for the module-level ``require_auth`` decorator. Reach
    # these via ``flask.current_app.config["llmbase"]``.
    app.config["llmbase"] = {
        "base_dir": base,
        "cfg": cfg,
        "api_secret": API_SECRET,
        "session_token": SESSION_TOKEN,
    }

    # ─── API Routes ────────────────────────────────────────────

    @app.route("/api/healthz")
    def api_healthz():
        """Liveness probe — must return instantly with no I/O.

        Used by Railway healthchecks (and any monitoring) to detect when
        the web layer wedges. Deliberately does NOT touch the filesystem,
        the LLM, the worker daemon, or any external service. If gunicorn
        can route a request to a worker thread at all, this returns 200.
        """
        return jsonify({"status": "ok"}), 200

    @app.route("/api/branding")
    def api_branding():
        cfg = load_config(base)
        branding = cfg.get("branding", {})
        return jsonify({
            "name": branding.get("name", "LLMBase"),
            "nameShort": branding.get("name_short", "L"),
            "tagline": branding.get("tagline", "Knowledge Base"),
            "poweredBy": {
                "label": branding.get("powered_by_label", "Powered by LLMBase"),
                "url": branding.get("powered_by_url", "https://github.com/Hosuke/llmbase"),
            },
        })

    @app.route("/api/stats")
    def api_stats():
        cfg = load_config(base)
        raw_dir = Path(cfg["paths"]["raw"])
        concepts_dir = Path(cfg["paths"]["concepts"])
        outputs_dir = Path(cfg["paths"]["outputs"])

        raw_count = len(list(raw_dir.glob("*"))) if raw_dir.exists() else 0
        article_count = len(list(concepts_dir.glob("*.md"))) if concepts_dir.exists() else 0
        output_count = len(list(outputs_dir.glob("*.md"))) if outputs_dir.exists() else 0

        total_words = 0
        if concepts_dir.exists():
            for f in concepts_dir.glob("*.md"):
                total_words += len(f.read_text().split())

        # Count wiki-links
        import re
        link_count = 0
        if concepts_dir.exists():
            link_re = re.compile(r'\[\[[^\]]+\]\]')
            for f in concepts_dir.glob("*.md"):
                link_count += len(link_re.findall(f.read_text()))

        # Health score
        try:
            health_path = Path(cfg["paths"]["meta"]) / "health.json"
            if health_path.exists():
                health = json.loads(health_path.read_text())
                total_issues = health.get("results", {}).get("total_issues", 0)
                health_score = max(0, 100 - total_issues) if article_count > 0 else 0
            else:
                health_score = 100 if article_count > 0 else 0
        except Exception:
            health_score = 0

        return jsonify({
            "raw_count": raw_count,
            "article_count": article_count,
            "output_count": output_count,
            "total_words": total_words,
            "link_count": link_count,
            "health_score": health_score,
        })

    @app.route("/api/taxonomy")
    def api_taxonomy():
        """Get hierarchical category taxonomy. ?lang=zh|en|ja"""
        from .taxonomy import build_taxonomy
        cfg = load_config(base)
        meta_dir = Path(cfg["paths"]["meta"])
        lang = request.args.get("lang", "zh")
        # Taxonomy depends on both KB version (index.json) and the on-disk
        # taxonomy.json. Mix taxonomy.json mtime + lang into the etag.
        tx_path = meta_dir / "taxonomy.json"
        tx_sig = ""
        try:
            tst = tx_path.stat()
            tx_sig = f"{tst.st_mtime}:{tst.st_size}"
        except OSError:
            pass
        # build_taxonomy resolves titles from concepts/*.md at request time,
        # so concept edits (with no taxonomy.json rewrite) still change the
        # response — fold concept fingerprint into the etag.
        concepts_dir = Path(cfg["paths"]["concepts"])
        cx_sig = _concepts_fingerprint(concepts_dir)
        etag, last_mod = _kb_etag(meta_dir, f"taxonomy:{lang}:{tx_sig}:{cx_sig}")
        if etag and _if_none_match_hits(request.headers.get("If-None-Match"), etag):
            return _not_modified(etag, last_mod)
        categories = build_taxonomy(base, lang)
        resp = jsonify({"categories": categories})
        return _apply_kb_cache_headers(resp, etag, last_mod)

    @app.route("/api/collections")
    def api_collections():
        """Group articles into collections by tags."""
        cfg = load_config(base)
        concepts_dir = Path(cfg["paths"]["concepts"])
        collections: dict[str, list] = {}

        if concepts_dir.exists():
            for md_file in sorted(concepts_dir.glob("*.md")):
                post = frontmatter.load(str(md_file))
                tags = post.metadata.get("tags", [])
                entry = {
                    "slug": md_file.stem,
                    "title": post.metadata.get("title", md_file.stem),
                    "summary": post.metadata.get("summary", ""),
                }
                if not tags:
                    tags = ["uncategorized"]
                for tag in tags:
                    collections.setdefault(tag, []).append(entry)

        # Also build from config if defined
        configured = cfg.get("collections", {})
        result = []
        for tag in sorted(collections.keys()):
            label = configured.get(tag, {}).get("label", tag.title()) if isinstance(configured.get(tag), dict) else tag.title()
            result.append({
                "id": tag,
                "label": label,
                "count": len(collections[tag]),
                "articles": collections[tag],
            })

        return jsonify({"collections": result})

    @app.route("/api/articles")
    def api_articles():
        cfg = load_config(base)
        concepts_dir = Path(cfg["paths"]["concepts"])
        meta_dir = Path(cfg["paths"]["meta"])

        limit_raw = request.args.get("limit")
        cursor = request.args.get("cursor")
        tag = request.args.get("tag")
        q = request.args.get("q")
        fields_raw = request.args.get("fields")
        new_mode = any(v is not None for v in (limit_raw, cursor, tag, q, fields_raw))

        # Validate query params BEFORE evaluating conditional cache — a bad
        # request must surface as 400 even if its ETag happens to match.
        limit = None
        if limit_raw is not None:
            try:
                limit = int(limit_raw)
            except (TypeError, ValueError):
                return jsonify({"status": "error", "message": "limit must be int"}), 400
            if limit < 1 or limit > 1000:
                return jsonify({"status": "error", "message": "limit must be 1..1000"}), 400

        # ETag derived from concepts/*.md directly (not index.json) — articles
        # are served from disk, so direct edits between rebuilds must invalidate
        # the cache. Signature mixes slug + mtime + size for each file, catching
        # renames and same-size edits that share an mtime profile. stat() is
        # microseconds; for 12k files this is well under 100ms.
        all_md = sorted(concepts_dir.glob("*.md")) if concepts_dir.exists() else []
        total = len(all_md)
        max_mtime = 0.0
        sig_hash = hashlib.md5()
        for p in all_md:
            try:
                st = p.stat()
            except OSError:
                continue
            if st.st_mtime > max_mtime:
                max_mtime = st.st_mtime
            sig_hash.update(f"{p.name}:{st.st_mtime}:{st.st_size}\n".encode())
        etag_extra = request.query_string.decode("utf-8", errors="replace")
        sig_hash.update(f"|q={etag_extra}".encode())
        etag = f'W/"{sig_hash.hexdigest()[:16]}"' if all_md else None
        last_mod = formatdate(max_mtime, usegmt=True) if max_mtime else None
        if etag and _if_none_match_hits(request.headers.get("If-None-Match"), etag):
            return _not_modified(etag, last_mod)

        fields = None
        if fields_raw:
            fields = {f.strip() for f in fields_raw.split(",") if f.strip()}

        if not new_mode:
            arts = []
            for md_file in all_md:
                post = frontmatter.load(str(md_file))
                arts.append({
                    "slug": md_file.stem,
                    "title": post.metadata.get("title", md_file.stem),
                    "summary": post.metadata.get("summary", ""),
                    "tags": post.metadata.get("tags", []),
                })
            resp = jsonify({"articles": arts})
            return _apply_kb_cache_headers(resp, etag, last_mod)

        candidates = all_md
        if cursor:
            candidates = [p for p in candidates if p.stem > cursor]

        # Collect limit+1 to detect whether a next page exists; this avoids
        # emitting a phantom next_cursor when the last page lands exactly on
        # `limit` (which would force clients into an empty extra fetch).
        cap = (limit + 1) if limit is not None else None
        collected: list[tuple[Path, frontmatter.Post]] = []
        for p in candidates:
            post = frontmatter.load(str(p))
            if tag:
                if tag not in _normalize_tags(post.metadata.get("tags")):
                    continue
            if q:
                blob = (
                    str(post.metadata.get("title", "")) + " "
                    + str(post.metadata.get("summary", ""))
                ).lower()
                if q.lower() not in blob:
                    continue
            collected.append((p, post))
            if cap is not None and len(collected) >= cap:
                break

        has_more = limit is not None and len(collected) > limit
        if has_more:
            collected = collected[:limit]

        articles = []
        for p, post in collected:
            entry = {
                "slug": p.stem,
                "title": post.metadata.get("title", p.stem),
                "summary": post.metadata.get("summary", ""),
                "tags": post.metadata.get("tags", []),
            }
            if fields:
                entry = {k: v for k, v in entry.items() if k in fields}
            articles.append(entry)

        next_cursor = collected[-1][0].stem if has_more else None

        resp = jsonify({
            "articles": articles,
            "total": total,
            "count": len(articles),
            "next_cursor": next_cursor,
            "filters": {"tag": tag, "q": q},
        })
        return _apply_kb_cache_headers(resp, etag, last_mod)

    @app.route("/api/articles/lite")
    def api_articles_lite():
        """Slim {slug, title} list backed by index.json — 1 file read, no frontmatter parse."""
        cfg = load_config(base)
        meta_dir = Path(cfg["paths"]["meta"])
        idx_path = meta_dir / "index.json"

        etag, last_mod = _kb_etag(meta_dir, "lite")
        if etag and _if_none_match_hits(request.headers.get("If-None-Match"), etag):
            return _not_modified(etag, last_mod)

        if not idx_path.exists():
            resp = jsonify({"articles": [], "total": 0})
            return _apply_kb_cache_headers(resp, etag, last_mod)
        try:
            idx = json.loads(idx_path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return jsonify({"articles": [], "total": 0}), 500
        if not isinstance(idx, list):
            return jsonify({"articles": [], "total": 0}), 500
        lite = [
            {"slug": e.get("slug", ""), "title": e.get("title", e.get("slug", ""))}
            for e in idx if isinstance(e, dict)
        ]
        resp = jsonify({"articles": lite, "total": len(lite)})
        return _apply_kb_cache_headers(resp, etag, last_mod)

    @app.route("/api/articles/<path:slug>")
    def api_article(slug):
        from .resolve import load_aliases, resolve_link
        cfg = load_config(base)
        concepts_dir = Path(cfg["paths"]["concepts"])
        meta_dir = Path(cfg["paths"]["meta"])
        article_path = (concepts_dir / f"{slug}.md").resolve()
        # Path traversal guard
        if not str(article_path).startswith(str(concepts_dir.resolve())):
            return jsonify({"status": "error", "message": "Invalid slug"}), 400
        # If not found by slug, try alias resolution
        if not article_path.exists():
            aliases = load_aliases(meta_dir)
            resolved = resolve_link(slug, aliases)
            if resolved:
                article_path = (concepts_dir / f"{resolved}.md").resolve()
                if not str(article_path).startswith(str(concepts_dir.resolve())):
                    return jsonify({"status": "error", "message": "Invalid slug"}), 400
                slug = resolved
        if not article_path.exists():
            return jsonify({"status": "error", "message": f"Article not found: {slug}"}), 404
        post = frontmatter.load(str(article_path))
        # Sanitize source URLs (only allow http/https)
        sources = post.metadata.get("sources", [])
        safe_sources = []
        for src in sources:
            url = src.get("url", "")
            if url and not url.startswith(("http://", "https://")):
                src = {**src, "url": ""}
            safe_sources.append(src)

        return jsonify({
            "status": "ok",
            "slug": slug,
            "title": post.metadata.get("title", slug),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []),
            "sources": safe_sources,
            "content": post.content,
            "backlinks": _get_backlinks(cfg, slug),
        })

    def _get_backlinks(cfg, slug):
        """Get articles that link to this slug, with titles."""
        meta_dir = Path(cfg["paths"]["meta"])
        concepts_dir = Path(cfg["paths"]["concepts"])
        bl_path = meta_dir / "backlinks.json"
        if not bl_path.exists():
            return []
        try:
            data = json.loads(bl_path.read_text())
        except (json.JSONDecodeError, OSError):
            return []
        slugs = data.get(slug, [])
        result = []
        concepts_resolved = str(concepts_dir.resolve()) + "/"
        for s in slugs:
            p = (concepts_dir / f"{s}.md").resolve()
            if not (str(p) + "/").startswith(concepts_resolved):
                continue  # Path traversal guard
            if p.exists():
                post = frontmatter.load(str(p))
                result.append({"slug": s, "title": post.metadata.get("title", s)})
        return result

    @app.route("/api/aliases")
    def api_aliases():
        from .resolve import load_aliases
        cfg = load_config(base)
        aliases = load_aliases(Path(cfg["paths"]["meta"]))
        return jsonify({"aliases": aliases})

    # ─── Structured Export ─────────────────────────────────

    @app.route("/api/export/article/<path:slug>")
    def api_export_article(slug):
        from .export import export_article
        result = export_article(slug, base)
        if not result:
            return jsonify({"status": "error", "message": "Not found"}), 404
        return jsonify(result)

    @app.route("/api/export/tag/<tag>")
    def api_export_tag(tag):
        from .export import export_by_tag
        return jsonify(export_by_tag(tag, base))

    @app.route("/api/export/graph/<path:slug>")
    def api_export_graph(slug):
        from .export import export_graph
        try:
            depth = max(0, min(int(request.args.get("depth", 2)), 5))
        except (ValueError, TypeError):
            return jsonify({"status": "error", "message": "depth must be integer 0-5"}), 400
        return jsonify(export_graph(slug, depth, base))

    @app.route("/api/entities")
    def api_entities():
        """Return extracted entities (people, events, places)."""
        from .entities import get_entities
        return jsonify(get_entities(base))

    @app.route("/api/entities/extract", methods=["POST"])
    @require_auth
    def api_extract_entities():
        """Trigger entity extraction."""
        from .entities import extract_entities
        result = extract_entities(base)
        return jsonify(result)

    @app.route("/api/refs/plugins")
    def api_ref_plugins():
        """List available reference source plugins."""
        from .refs import list_plugins
        return jsonify({"plugins": list_plugins()})

    # ─── Research Trails ──────────────────────────────────────

    def _load_trails():
        cfg = load_config(base)
        path = Path(cfg["paths"]["meta"]) / "trails.json"
        if path.exists():
            try:
                return json.loads(path.read_text())
            except (json.JSONDecodeError, OSError):
                pass
        return {"trails": []}

    def _save_trails(data):
        from .atomic import atomic_write_json
        cfg = load_config(base)
        path = Path(cfg["paths"]["meta"]) / "trails.json"
        atomic_write_json(path, data)

    @app.route("/api/trails")
    @require_auth
    def api_trails():
        """List all research trails."""
        return jsonify(_load_trails())

    import threading as _threading
    _trail_lock = _threading.Lock()

    @app.route("/api/trails", methods=["POST"])
    @require_auth
    def api_trails_save():
        """Add a step to a trail (or create a new trail)."""
        import uuid
        from datetime import datetime, timezone
        data = request.get_json(silent=True) or {}
        step = data.get("step")
        if step and not isinstance(step, dict):
            return jsonify({"status": "error", "message": "step must be a dict"}), 400
        step = step or {}
        trail_id = data.get("trail_id")
        name = data.get("name", "")

        with _trail_lock:
            trails_data = _load_trails()
            trails = trails_data.get("trails", [])
            now = datetime.now(timezone.utc).isoformat()

            if trail_id:
                trail = next((t for t in trails if t["id"] == trail_id), None)
                if trail:
                    step["ts"] = now  # Server timestamp always wins
                    trail["steps"].append(step)
                    trail["updated"] = now
                else:
                    return jsonify({"status": "error", "message": "Trail not found"}), 404
            else:
                # Create new trail
                trail = {
                    "id": uuid.uuid4().hex[:12],
                    "name": name or f"Trail {len(trails) + 1}",
                    "created": now,
                    "updated": now,
                    "steps": [{**step, "ts": now}] if step.get("type") else [],
                }
                trails.append(trail)

            trails_data["trails"] = trails
            _save_trails(trails_data)
            return jsonify({"trail": trail})

    @app.route("/api/trails/<trail_id>/delete", methods=["POST"])
    @require_auth
    def api_trail_delete(trail_id):
        """Delete a research trail."""
        with _trail_lock:
            trails_data = _load_trails()
            trails_data["trails"] = [t for t in trails_data.get("trails", []) if t["id"] != trail_id]
            _save_trails(trails_data)
        return jsonify({"status": "ok"})

    @app.route("/api/xici")
    def api_xici():
        """Get the cached Xi Ci (guided introduction). ?lang=zh|en|ja|zh-en"""
        from .xici import get_xici
        lang = request.args.get("lang", "zh")
        return jsonify(get_xici(base, lang))

    @app.route("/api/xici/generate", methods=["POST"])
    @require_auth
    def api_xici_generate():
        """Regenerate Xi Ci for a given language."""
        from .xici import generate_xici
        data = request.json or {}
        lang = data.get("lang", "zh")
        result = generate_xici(base, lang)
        return jsonify(result)

    @app.route("/api/search")
    def api_search():
        q = request.args.get("q", "")
        top_k = int(request.args.get("top_k", 10))
        results = search(q, top_k=top_k, base_dir=base)
        return jsonify({"query": q, "results": results})

    @app.route("/api/ask", methods=["POST"])
    def api_ask():
        data = request.json or {}
        q = data.get("question", "")
        deep = data.get("deep", False)
        file_back = data.get("file_back", True)
        tone = data.get("tone", "default")
        promote = data.get("promote", False)
        # Per-request model override: empty/None falls back to LLMBASE_MODEL.
        # Reject non-string / oversized to keep the param plumbing tight.
        raw_model = data.get("model")
        model: str | None = None
        if raw_model is not None:
            if not isinstance(raw_model, str):
                return jsonify({"status": "error", "message": "model must be a string"}), 400
            raw_model = raw_model.strip()
            if len(raw_model) > 200:
                return jsonify({"status": "error", "message": "model too long"}), 400
            model = raw_model or None
        # Model override is a cost-impacting lever on public deployments —
        # untrusted callers could pin the most expensive model available to
        # the backing API key. When API_SECRET is set (prod signal), require
        # auth for model override the same way promote=True does. An
        # optional LLMBASE_MODEL_ALLOWLIST (comma-separated) gates which
        # models any caller — authed or not — may select.
        if model is not None:
            allowlist_raw = os.getenv("LLMBASE_MODEL_ALLOWLIST", "").strip()
            if allowlist_raw:
                allowed = {m.strip() for m in allowlist_raw.split(",") if m.strip()}
                if model not in allowed:
                    return jsonify({
                        "status": "error",
                        "message": f"model '{model}' not in LLMBASE_MODEL_ALLOWLIST",
                    }), 400
        auth_header = request.headers.get("Authorization", "").replace("Bearer ", "")
        auth_cookie = request.cookies.get("llmbase_auth", "")
        # Two auth levels: the SPA-convenience cookie (minted for anyone who
        # loads `/`) is accepted for promote — the existing UI relies on it.
        # Model override, which is a cost lever on public deployments, demands
        # the raw API secret via the Authorization header so a drive-by
        # browser visitor cannot pin an expensive model. See CHANGELOG.
        authed_cookie = bool(API_SECRET) and (
            hmac.compare_digest(auth_header, API_SECRET)
            or hmac.compare_digest(auth_cookie, SESSION_TOKEN)
        )
        authed_strong = bool(API_SECRET) and hmac.compare_digest(auth_header, API_SECRET)
        if promote and API_SECRET and not authed_cookie:
            return jsonify({
                "status": "error",
                "message": "promote=true requires authentication",
            }), 401
        if model is not None and API_SECRET and not authed_strong:
            return jsonify({
                "status": "error",
                "message": "model override requires Authorization: Bearer <API_SECRET>",
            }), 401
        if deep:
            # Route through operations.dispatch so promote=True acquires the
            # shared job_lock (same behavior as MCP / CLI / agent-HTTP).
            from . import operations as _ops
            ask_args = {
                "question": q,
                "tone": tone,
                "file_back": file_back,
                "deep": True,
                "promote": promote,
            }
            if model is not None:
                ask_args["model"] = model
            try:
                result = _ops.dispatch("kb_ask", base, ask_args)
            except RuntimeError as e:
                return jsonify({"status": "busy", "error": str(e)}), 409
            payload = {
                "answer": result["answer"],
                "consulted": result.get("consulted", []),
            }
            if result.get("output_path"):
                payload["output_path"] = result["output_path"]
            if "promotion" in result:
                payload["promotion"] = result["promotion"]
            return jsonify(payload)
        else:
            result = query(
                q,
                file_back=file_back,
                base_dir=base,
                tone=tone,
                return_path=True,
                model=model,
            )
            payload = {"answer": result["answer"]}
            if result.get("output_path"):
                payload["output_path"] = result["output_path"]
            return jsonify(payload)

    @app.route("/api/tones", methods=["GET"])
    def api_tones():
        """List available response tone modes."""
        from .query import TONE_INSTRUCTIONS
        tones = [
            {"id": "default", "label": "Default", "label_zh": "默认", "icon": "chat"},
            {"id": "caveman", "label": "Caveman", "label_zh": "原始人", "icon": "pets"},
            {"id": "wenyan", "label": "文言文", "label_zh": "文言文", "icon": "history_edu"},
            {"id": "scholar", "label": "Scholar", "label_zh": "学术", "icon": "school"},
            {"id": "eli5", "label": "ELI5", "label_zh": "幼儿园", "icon": "child_care"},
        ]
        return jsonify({"tones": [t for t in tones if t["id"] in TONE_INSTRUCTIONS]})

    @app.route("/api/sources")
    def api_sources():
        docs = list_raw(base)
        return jsonify({"documents": docs})

    @app.route("/api/sources/<path:slug>")
    def api_source_detail(slug):
        """Read raw document content for preview."""
        cfg = load_config(base)
        raw_dir = Path(cfg["paths"]["raw"])
        doc_dir = raw_dir / slug
        idx = doc_dir / "index.md"
        if not idx.exists():
            return jsonify({"status": "error", "message": "Not found"})
        post = frontmatter.load(str(idx))
        # Configurable content cap: sources.max_content_chars (default 50000)
        # null → use hard ceiling (500K); explicit int → clamped to [0, 500K]
        _HARD_CEILING = 500_000
        raw_max = cfg.get("sources", {}).get("max_content_chars", 50000)
        try:
            max_chars = min(max(0, int(raw_max)), _HARD_CEILING) if raw_max is not None else _HARD_CEILING
        except (TypeError, ValueError):
            max_chars = 50000
        content = post.content[:max_chars]
        return jsonify({
            "slug": slug,
            "title": post.metadata.get("title", slug),
            "type": post.metadata.get("type", "unknown"),
            "compiled": post.metadata.get("compiled", False),
            "content": content,
            "metadata": {k: str(v) for k, v in post.metadata.items()},
        })

    @app.route("/api/ingest", methods=["POST"])
    @require_auth
    def api_ingest():
        from . import operations as _ops
        data = request.json or {}
        try:
            result = _ops.dispatch("kb_ingest", base, {"source": data.get("source", "")})
        except RuntimeError as e:
            return jsonify({"status": "busy", "error": str(e)}), 409
        return jsonify({"status": "ok", **result})

    @app.route("/api/upload", methods=["POST"])
    @require_auth
    def api_upload():
        """Upload a PDF/markdown file for ingestion."""
        if "file" not in request.files:
            return jsonify({"status": "error", "message": "No file provided"}), 400
        f = request.files["file"]
        if not f.filename:
            return jsonify({"status": "error", "message": "Empty filename"}), 400

        cfg = load_config(base)
        raw_dir = Path(cfg["paths"]["raw"])
        raw_dir.mkdir(parents=True, exist_ok=True)

        # Save uploaded file
        import tempfile
        ext = Path(f.filename).suffix.lower()
        with tempfile.NamedTemporaryFile(delete=False, suffix=ext, dir=str(raw_dir)) as tmp:
            f.save(tmp)
            tmp_path = tmp.name

        # Process based on file type
        if ext == ".pdf":
            from .pdf import ingest_pdf
            paths = ingest_pdf(tmp_path, chunk_pages=20, base_dir=base)
            Path(tmp_path).unlink()  # Clean up temp file
            return jsonify({"status": "ok", "chunks": len(paths), "filename": f.filename})
        else:
            from .ingest import ingest_file
            path = ingest_file(tmp_path, base)
            Path(tmp_path).unlink()
            return jsonify({"status": "ok", "path": str(path), "filename": f.filename})

    @app.route("/api/articles/<slug>", methods=["DELETE"])
    @require_auth
    def api_delete_article(slug):
        """Delete a wiki article by slug."""
        cfg = load_config(base)
        concepts_dir = Path(cfg["paths"]["concepts"])
        article_path = concepts_dir / f"{slug}.md"
        if article_path.exists():
            article_path.unlink()
            return jsonify({"status": "ok", "deleted": slug})
        return jsonify({"status": "error", "message": "Not found"})

    @app.route("/api/wiki/clean", methods=["POST"])
    @require_auth
    def api_clean_wiki():
        """Remove garbage/empty stub articles and update taxonomy."""
        cfg = load_config(base)
        concepts_dir = Path(cfg["paths"]["concepts"])
        removed = []
        for f in sorted(concepts_dir.glob("*.md")):
            post = frontmatter.load(str(f))
            title = post.metadata.get("title", "")
            summary = post.metadata.get("summary", "")
            content = post.content.strip()
            if (
                "English Title / 中文标题" in title
                or "One-line summary in English" in summary
                or "The user says" in summary
                or "has not been fully written" in content
                or "has not been written yet" in content
                or "尚未完成撰写" in content
                or len(content) < 50
            ):
                f.unlink()
                removed.append(f.stem)
        if removed:
            rebuild_index(base)
        return jsonify({"status": "ok", "removed": len(removed), "slugs": removed})

    @app.route("/api/taxonomy/update", methods=["POST"])
    @require_auth
    def api_update_taxonomy():
        """Upload a new taxonomy.json. Automatically locked to prevent worker overwrite."""
        data = request.json
        if not data or "categories" not in data:
            return jsonify({"status": "error", "message": "Provide {categories: [...]}"})
        data["locked"] = True  # Prevent worker from overwriting
        cfg = load_config(base)
        meta_dir = Path(cfg["paths"]["meta"])
        meta_dir.mkdir(parents=True, exist_ok=True)
        path = meta_dir / "taxonomy.json"
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
        return jsonify({"status": "ok", "categories": len(data["categories"]), "locked": True})

    @app.route("/api/worker/status")
    @require_auth
    def api_worker_status():
        """Report whether the shared job_lock is held.

        Issue #7: the Web-UI Ingest page lost in-flight state on route
        change, so its compile button re-enabled while a worker job was
        still running and a second click 409'd. The SPA now polls this
        endpoint on Ingest mount to recover the lock state and keep the
        button disabled + status visible until the job settles.

        Auth-gated to match the write endpoints whose state it reflects —
        without ``API_SECRET`` the decorator is a no-op (dev mode).
        """
        from .worker import job_lock
        busy = job_lock.locked()
        return jsonify({"busy": busy})

    @app.route("/api/compile", methods=["POST"])
    @require_auth
    def api_compile():
        from . import operations as _ops
        try:
            result = _ops.dispatch("kb_compile", base, {})
        except RuntimeError as e:
            return jsonify({"status": "busy", "error": str(e)}), 409
        return jsonify({"status": "ok", "articles_created": result["articles_created"]})

    @app.route("/api/lint", methods=["POST"])
    def api_lint():
        data = request.json or {}
        if data.get("deep"):
            from .lint import lint_deep
            report = lint_deep(base)
            return jsonify({"report": report})
        else:
            results = lint(base)
            return jsonify({"results": results})

    @app.route("/api/lint/fix", methods=["POST"])
    @require_auth
    def api_lint_fix():
        """Run the full auto-fix pipeline in background thread."""
        import threading
        from .lint import auto_fix

        def run_fix():
            import json, logging
            from .worker import job_lock
            logger = logging.getLogger("llmbase.lint")
            if not job_lock.acquire(blocking=False):
                logger.warning("[lint/fix] Another job is running, skipping")
                return
            try:
                logger.info("[lint/fix] Starting auto-fix pipeline...")
                fixes = auto_fix(base)
                logger.info(f"[lint/fix] Done! {len(fixes)} fixes applied")
                # Persist result
                cfg = load_config(base)
                meta_dir = Path(cfg["paths"]["meta"])
                meta_dir.mkdir(parents=True, exist_ok=True)
                result = {"fixes": fixes, "fix_count": len(fixes), "status": "completed"}
                (meta_dir / "last_fix.json").write_text(
                    json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8"
                )
            except Exception as e:
                logger.error(f"[lint/fix] Error: {e}")
            finally:
                job_lock.release()

        threading.Thread(target=run_fix, daemon=True).start()
        return jsonify({"status": "started", "message": "Auto-fix pipeline running in background. Check /api/health for results."})

    @app.route("/api/health")
    def api_health():
        """Return the last persisted health report."""
        cfg = load_config(base)
        meta_dir = Path(cfg["paths"]["meta"])
        health_path = meta_dir / "health.json"
        if not health_path.exists():
            return jsonify({"report": None})
        report = json.loads(health_path.read_text())
        return jsonify({"report": report})

    @app.route("/api/wiki/export")
    def api_wiki_export():
        """Export all wiki articles as JSON (for backup/sync)."""
        cfg = load_config(base)
        concepts_dir = Path(cfg["paths"]["concepts"])
        articles = {}
        if concepts_dir.exists():
            for md_file in sorted(concepts_dir.glob("*.md")):
                post = frontmatter.load(str(md_file))
                articles[md_file.stem] = {
                    "metadata": dict(post.metadata),
                    "content": post.content,
                }
        return jsonify({"articles": articles, "count": len(articles)})

    @app.route("/api/index/rebuild", methods=["POST"])
    @require_auth
    def api_rebuild_index():
        from . import operations as _ops
        try:
            result = _ops.dispatch("kb_rebuild_index", base, {})
        except RuntimeError as e:
            return jsonify({"status": "busy", "error": str(e)}), 409
        return jsonify({"status": "ok", **result})

    # ─── SPA Fallback ──────────────────────────────────────────

    # Serve custom favicon from project static/ dir if it exists
    @app.route("/favicon.svg")
    def serve_favicon():
        custom_favicon = base / "static" / "favicon.svg"
        if custom_favicon.exists():
            return send_from_directory(str(custom_favicon.parent), "favicon.svg")
        return send_from_directory(str(static_dir), "favicon.svg")

    @app.route("/", defaults={"path": ""})
    @app.route("/<path:path>")
    def serve_spa(path):
        # Serve static file if it exists, otherwise fall back to index.html
        file_path = static_dir / path
        if path and file_path.exists():
            return send_from_directory(str(static_dir), path)
        # Set auth cookie with derived session token (never expose the raw secret)
        from flask import make_response
        resp = make_response(send_from_directory(str(static_dir), "index.html"))
        if SESSION_TOKEN:
            resp.set_cookie("llmbase_auth", SESSION_TOKEN,
                            httponly=True, samesite="Strict", secure=False)
        return resp

    # ─── Extension points ────────────────────────────────────────
    # Register extra routes from EXTRA_ROUTES
    for route_entry in EXTRA_ROUTES:
        rule, view_func = route_entry[0], route_entry[1]
        options = route_entry[2] if len(route_entry) > 2 else {}
        app.route(rule, **options)(view_func)

    # Register before/after request hooks
    for hook in BEFORE_REQUEST_HOOKS:
        app.before_request(hook)
    for hook in AFTER_REQUEST_HOOKS:
        app.after_request(hook)

    return app
