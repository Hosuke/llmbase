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
                }
                for op in ops.all_operations()
            ],
        })

    return app
