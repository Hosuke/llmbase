"""Agent-facing API: exposes knowledge base operations as callable functions.

This module provides a clean interface for AI agents to interact with the
knowledge base programmatically, either via direct Python imports or via
the JSON-RPC HTTP server.
"""

import json
from pathlib import Path

from flask import Flask, request, jsonify

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

    def ask(self, question: str, deep: bool = False, file_back: bool = True, tone: str = "default") -> dict:
        """Ask a question against the knowledge base."""
        if deep:
            answer = query_with_search(question, self.base_dir, tone=tone)
        else:
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

    @app.route("/api/ingest", methods=["POST"])
    def api_ingest():
        data = request.json
        return jsonify(kb.ingest(data["source"]))

    @app.route("/api/compile", methods=["POST"])
    def api_compile():
        data = request.json or {}
        return jsonify(kb.compile(full=data.get("full", False)))

    @app.route("/api/ask", methods=["POST"])
    def api_ask():
        data = request.json
        return jsonify(kb.ask(
            data["question"],
            deep=data.get("deep", False),
            file_back=data.get("file_back", True),
            tone=data.get("tone", "default"),
        ))

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
        return jsonify(kb.lint_fix())

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
        return jsonify(kb.rebuild_index())

    return app
