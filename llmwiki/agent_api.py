"""Agent-facing API: exposes knowledge base operations as callable functions.

All operations route through ``tools.operations`` — the single source of
truth shared with the CLI and MCP server. Legacy semantic endpoints
(``/api/ask``, ``/api/search``, …) remain as thin wrappers for backwards
compatibility; new clients should prefer the generic ``/api/op/<name>``.
"""

import json
from pathlib import Path

from flask import Flask, request, jsonify

from . import operations as ops
from .config import load_config, ensure_dirs
from .ingest import ingest_url, ingest_file, list_raw
from .compile import compile_new, compile_all, rebuild_index
from .query import query, query_with_search
from .search import search
from .lint import lint, lint_deep, auto_fix


class KnowledgeBase:
    """High-level API for agents to interact with the knowledge base."""

    def __init__(self, base_dir: str | Path | None = None):
        self.base_dir = Path(base_dir) if base_dir else Path.cwd()
        self.cfg = load_config(self.base_dir)
        ensure_dirs(self.cfg)

    def ingest(self, source: str) -> dict:
        """Ingest a URL or local file path."""
        if source.startswith(("http://", "https://")):
            path = ingest_url(source, self.base_dir)
        else:
            path = ingest_file(source, self.base_dir)
        return {"status": "ok", "path": str(path)}

    def compile(self, full: bool = False) -> dict:
        """Compile raw documents into wiki. full=True recompiles everything."""
        if full:
            articles = compile_all(self.base_dir)
        else:
            articles = compile_new(self.base_dir)
        return {"status": "ok", "articles_created": len(articles), "articles": articles}

    def ask(
        self,
        question: str,
        deep: bool = False,
        file_back: bool = True,
        tone: str = "default",
        promote: bool = False,
    ) -> dict:
        """Ask a question against the knowledge base.

        When deep=True, runs the two-step search → answer pipeline and returns
        the articles consulted. When promote=True, an LLM judge decides whether
        the Q&A should become a new wiki concept; if yes, the article is
        written into wiki/concepts and the index rebuilt.
        """
        if deep:
            result = query_with_search(
                question,
                self.base_dir,
                tone=tone,
                file_back=file_back,
                return_context=True,
                promote=promote,
            )
            if isinstance(result, dict):
                return {"status": "ok", **result}
            return {"status": "ok", "answer": result}
        answer = query(question, file_back=file_back, base_dir=self.base_dir, tone=tone)
        return {"status": "ok", "answer": answer}

    def search(self, query_text: str, top_k: int = 10) -> dict:
        """Full-text search."""
        results = search(query_text, top_k=top_k, base_dir=self.base_dir)
        return {"status": "ok", "results": results}

    def lint_check(self, deep_check: bool = False) -> dict:
        """Run health checks."""
        if deep_check:
            report = lint_deep(self.base_dir)
            return {"status": "ok", "report": report}
        else:
            results = lint(self.base_dir)
            return {"status": "ok", "results": results}

    def lint_fix(self) -> dict:
        """Run auto-fix on lint issues (metadata + broken links)."""
        fixes = auto_fix(self.base_dir)
        return {"status": "ok", "fixes": fixes, "fix_count": len(fixes)}

    def health_report(self) -> dict:
        """Return the last persisted health report."""
        from . import _native
        if _native.available():
            return _native.dispatch("compat_kb_health", {}, self.base_dir, paths=self.cfg["paths"])["result"]
        meta_dir = Path(self.cfg["paths"]["meta"])
        health_path = meta_dir / "health.json"
        if not health_path.exists():
            return {"status": "ok", "report": None, "message": "No health check has run yet"}
        report = json.loads(health_path.read_text())
        return {"status": "ok", "report": report}

    def get_xici(self, lang: str = "zh") -> dict:
        """Get the Xi Ci (guided introduction) for the knowledge base."""
        from .xici import get_xici
        return {"status": "ok", **get_xici(self.base_dir, lang)}

    def generate_xici(self, lang: str = "zh") -> dict:
        """Regenerate the Xi Ci."""
        from .xici import generate_xici
        result = generate_xici(self.base_dir, lang)
        return {"status": "ok", **result}

    def list_sources(self) -> dict:
        """List all ingested raw documents."""
        docs = list_raw(self.base_dir)
        return {"status": "ok", "documents": docs}

    def rebuild_index(self) -> dict:
        """Rebuild the wiki index."""
        entries = rebuild_index(self.base_dir)
        return {"status": "ok", "article_count": len(entries)}

    def get_article(self, slug: str) -> dict:
        """Read a specific wiki article by slug."""
        from . import _native
        if _native.available():
            result = _native.dispatch("compat_kb_get", {"slug": slug, "no_alias": True},
                                      self.base_dir, paths=self.cfg["paths"])["result"]
            if not result["found"]:
                return {"status": "error", "message": f"Article not found: {slug}"}
            return {"status": "ok", **{key: result[key] for key in ("slug", "title", "summary", "tags", "content")}}
        import frontmatter as fm
        concepts_dir = Path(self.cfg["paths"]["concepts"])
        article_path = concepts_dir / f"{slug}.md"
        if not article_path.exists():
            return {"status": "error", "message": f"Article not found: {slug}"}
        post = fm.load(str(article_path))
        return {
            "status": "ok",
            "slug": slug,
            "title": post.metadata.get("title", slug),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []),
            "content": post.content,
        }

    def export_article(self, slug: str) -> dict:
        from .export import export_article
        result = export_article(slug, self.base_dir)
        if not result:
            return {"status": "error", "message": f"Article not found: {slug}"}
        return {"status": "ok", **result}

    def export_by_tag(self, tag: str) -> dict:
        from .export import export_by_tag
        return {"status": "ok", **export_by_tag(tag, self.base_dir)}

    def export_graph(self, slug: str, depth: int = 2) -> dict:
        from .export import export_graph
        return {"status": "ok", **export_graph(slug, depth, self.base_dir)}

    def list_articles(self) -> dict:
        """List all wiki articles with metadata."""
        from . import _native
        if _native.available():
            result = _native.dispatch("compat_kb_list", {}, self.base_dir, paths=self.cfg["paths"])["result"]
            return {"status": "ok", **result}
        import frontmatter as fm
        concepts_dir = Path(self.cfg["paths"]["concepts"])
        articles = []
        if concepts_dir.exists():
            for md_file in sorted(concepts_dir.glob("*.md")):
                post = fm.load(str(md_file))
                articles.append({
                    "slug": md_file.stem,
                    "title": post.metadata.get("title", md_file.stem),
                    "summary": post.metadata.get("summary", ""),
                    "tags": post.metadata.get("tags", []),
                })
        return {"status": "ok", "articles": articles}


def create_agent_server(base_dir: str | Path | None = None, port: int = 5556):
    """Create an HTTP API server for agent access."""
    app = Flask(__name__)
    kb = KnowledgeBase(base_dir)
    import os
    from flask import g
    from .http_auth import (authorize, can_read_private, operation_scopes, private_metadata,
                            private_response_headers, runtime_config, visibility_snapshot,
                            visibility_snapshot_error)
    app.config["llmbase"] = runtime_config(kb.base_dir, kb.cfg,
        api_secret=os.getenv("LLMBASE_API_SECRET", ""))
    app.after_request(private_response_headers)

    @app.before_request
    def authorize_operation():
        data = request.get_json(silent=True) or {}
        if not isinstance(data, dict):
            return jsonify({"status": "error", "message": "arguments must be an object"}), 400
        legacy_names = {"api_ingest": "kb_ingest", "api_compile": "kb_compile", "api_ask": "kb_ask",
                        "api_lint": "kb_lint", "api_lint_fix": "kb_lint_fix", "api_rebuild_index": "kb_rebuild_index"}
        name = (request.view_args or {}).get("name") if request.endpoint == "api_op" else legacy_names.get(request.endpoint)
        if request.endpoint == "api_ask":
            data = {**data, "file_back": data.get("file_back", True)}
        op = ops.get(name) if name else None
        scopes = operation_scopes(op, data) if op is not None else ("read",)
        denied = authorize(*scopes)
        if denied is not None:
            return denied
        if can_read_private():
            return None
        try:
            snapshot = visibility_snapshot(kb.base_dir, kb.cfg)
        except (OSError, ValueError, RuntimeError) as error:
            return jsonify(visibility_snapshot_error(error)), 503
        g.llmbase_visibility = snapshot
        opaque = {"kb_lint", "kb_health", "kb_stats", "kb_taxonomy", "kb_backlinks", "kb_export", "kb_export_article",
                  "kb_export_tag", "kb_export_graph", "kb_xici"}
        if snapshot["has_nonpublic"] and (set(scopes) & {"model", "write", "operator"}
                or name in opaque or request.endpoint in {"api_lint", "api_health"}):
            return authorize("private")
        if request.endpoint == "api_article":
            if not snapshot["compiled"].get((request.view_args or {}).get("slug"), {}).get("public", False):
                return jsonify({"status": "error", "message": "Article not found"}), 404
        return None

    @app.after_request
    def filter_public_rows(response):
        if can_read_private() or not response.is_json or response.status_code != 200:
            return response
        snapshot = getattr(g, "llmbase_visibility", None)
        if snapshot is None:
            return response
        data = response.get_json()
        if not isinstance(data, dict):
            return response
        value = data.get("result", data)
        if not isinstance(value, dict):
            return response
        public = lambda slug: snapshot["compiled"].get(slug, {}).get("public", False)
        for key in ("articles",):
            if isinstance(value.get(key), list):
                value[key] = [row for row in value[key] if isinstance(row, dict) and public(row.get("slug", ""))]
        if isinstance(value.get("results"), list) and (request.endpoint == "api_search"
                or (request.view_args or {}).get("name") in {"kb_search", "kb_search_raw"}):
            from .http_auth import public_search_hit
            raw = (request.view_args or {}).get("name") == "kb_search_raw"
            value["results"] = [row for row in value["results"] if public_search_hit(snapshot, kb.cfg, row, raw=raw)]
        if isinstance(value.get("documents"), list):
            value["documents"] = [row for row in value["documents"]
                                  if ".original" not in Path(row.get("path", "")).parts
                                  and not private_metadata(row)]
        if request.endpoint == "api_op" and (request.view_args or {}).get("name") in {"kb_get", "kb_get_sections"}:
            if value.get("found") and not public(value.get("slug", "")):
                data["result"] = {"found": False, "slug": (request.get_json(silent=True) or {}).get("slug", "")}
        response.set_data(app.json.dumps(data))
        return response

    def _legacy_dispatch(op_name: str, args: dict):
        try:
            result = ops.dispatch(op_name, kb.base_dir, args)
        except RuntimeError as e:
            return jsonify({"status": "busy", "error": str(e)}), 409
        return jsonify({"status": "ok", **result})

    @app.route("/api/ingest", methods=["POST"])
    def api_ingest():
        data = request.json or {}
        return _legacy_dispatch("kb_ingest", {"source": data["source"]})

    @app.route("/api/compile", methods=["POST"])
    def api_compile():
        data = request.json or {}
        return _legacy_dispatch("kb_compile", {"full": data.get("full", False)})

    @app.route("/api/ask", methods=["POST"])
    def api_ask():
        data = request.json or {}
        # Route through operations.dispatch so promote=True acquires the same
        # job-lock that the MCP and CLI surfaces use.
        try:
            result = ops.dispatch("kb_ask", kb.base_dir, {
                "question": data["question"],
                "deep": data.get("deep", False),
                "file_back": data.get("file_back", True),
                "tone": data.get("tone", "default"),
                "promote": data.get("promote", False),
            })
        except RuntimeError as e:
            return jsonify({"status": "busy", "error": str(e)}), 409
        return jsonify({"status": "ok", **result})

    @app.route("/api/search", methods=["GET"])
    def api_search():
        q = request.args.get("q", "")
        top_k = int(request.args.get("top_k", 10))
        from .search import search_visibility
        with search_visibility(public_only=not can_read_private()):
            return jsonify(kb.search(q, top_k=top_k))

    @app.route("/api/lint", methods=["POST"])
    def api_lint():
        data = request.json or {}
        return jsonify(kb.lint_check(deep_check=data.get("deep", False)))

    @app.route("/api/lint/fix", methods=["POST"])
    def api_lint_fix():
        return _legacy_dispatch("kb_lint_fix", {})

    @app.route("/api/health", methods=["GET"])
    def api_health():
        return jsonify(kb.health_report())

    @app.route("/api/sources", methods=["GET"])
    def api_sources():
        return jsonify(kb.list_sources())

    @app.route("/api/articles", methods=["GET"])
    def api_articles():
        return jsonify(kb.list_articles())

    @app.route("/api/articles/<slug>", methods=["GET"])
    def api_article(slug):
        return jsonify(kb.get_article(slug))

    @app.route("/api/index/rebuild", methods=["POST"])
    def api_rebuild_index():
        return _legacy_dispatch("kb_rebuild_index", {})

    # Generic operations dispatcher — exposes every registered op,
    # including ones added by downstream projects after import.
    @app.route("/api/op/<name>", methods=["POST"])
    def api_op(name):
        if ops.get(name) is None:
            return jsonify({"status": "error", "error": f"unknown operation: {name}"}), 404
        args = request.get_json(silent=True) or {}
        try:
            from .search import search_visibility
            with search_visibility(public_only=not can_read_private()):
                result = ops.dispatch(name, kb.base_dir, args)
            return jsonify({"status": "ok", "result": result})
        except RuntimeError as e:
            return jsonify({"status": "busy", "error": str(e)}), 409
        except TypeError as e:
            return jsonify({"status": "error", "error": f"bad arguments: {e}"}), 400
        except Exception as e:
            return jsonify({"status": "error", "error": str(e)}), 500

    @app.route("/api/op", methods=["GET"])
    def api_op_list():
        return jsonify({
            "status": "ok",
            "operations": [
                {
                    "name": op.name,
                    "description": op.description,
                    "params": op.params,
                    "writes": op.writes,
                    "category": op.category,
                    "required_scopes": list(operation_scopes(op, {})),
                }
                for op in ops.all_operations()
            ],
        })

    return app
