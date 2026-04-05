"""Web UI server — serves React frontend + API endpoints."""

import json
from pathlib import Path

import frontmatter
from flask import Flask, request, jsonify, send_from_directory

from .config import load_config, ensure_dirs
from .search import search
from .query import query, query_with_search
from .ingest import ingest_url, list_raw
from .compile import compile_new, rebuild_index
from .lint import lint


def create_web_app(base_dir: Path | None = None):
    """Create the full web application."""
    base = Path(base_dir) if base_dir else Path.cwd()
    static_dir = Path(__file__).resolve().parent.parent / "static" / "dist"

    app = Flask(__name__, static_folder=None)

    # ─── API Routes ────────────────────────────────────────────

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

        return jsonify({
            "raw_count": raw_count,
            "article_count": article_count,
            "output_count": output_count,
            "total_words": total_words,
        })

    @app.route("/api/taxonomy")
    def api_taxonomy():
        """Get hierarchical category taxonomy. ?lang=zh|en|ja"""
        from .taxonomy import build_taxonomy
        lang = request.args.get("lang", "zh")
        categories = build_taxonomy(base, lang)
        return jsonify({"categories": categories})

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
        arts = []
        if concepts_dir.exists():
            for md_file in sorted(concepts_dir.glob("*.md")):
                post = frontmatter.load(str(md_file))
                arts.append({
                    "slug": md_file.stem,
                    "title": post.metadata.get("title", md_file.stem),
                    "summary": post.metadata.get("summary", ""),
                    "tags": post.metadata.get("tags", []),
                })
        return jsonify({"articles": arts})

    @app.route("/api/articles/<slug>")
    def api_article(slug):
        cfg = load_config(base)
        concepts_dir = Path(cfg["paths"]["concepts"])
        article_path = concepts_dir / f"{slug}.md"
        if not article_path.exists():
            return jsonify({"status": "error", "message": f"Article not found: {slug}"})
        post = frontmatter.load(str(article_path))
        return jsonify({
            "status": "ok",
            "slug": slug,
            "title": post.metadata.get("title", slug),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []),
            "content": post.content,
        })

    @app.route("/api/search")
    def api_search():
        q = request.args.get("q", "")
        top_k = int(request.args.get("top_k", 10))
        results = search(q, top_k=top_k, base_dir=base)
        return jsonify({"query": q, "results": results})

    @app.route("/api/ask", methods=["POST"])
    def api_ask():
        data = request.json
        q = data.get("question", "")
        deep = data.get("deep", False)
        file_back = data.get("file_back", True)
        tone = data.get("tone", "default")
        if deep:
            answer = query_with_search(q, base, tone=tone)
        else:
            answer = query(q, file_back=file_back, base_dir=base, tone=tone)
        return jsonify({"answer": answer})

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
        return jsonify({
            "slug": slug,
            "title": post.metadata.get("title", slug),
            "type": post.metadata.get("type", "unknown"),
            "compiled": post.metadata.get("compiled", False),
            "content": post.content[:10000],  # Cap at 10K chars for preview
            "metadata": {k: str(v) for k, v in post.metadata.items()},
        })

    @app.route("/api/ingest", methods=["POST"])
    def api_ingest():
        data = request.json
        source = data.get("source", "")
        path = ingest_url(source, base)
        return jsonify({"status": "ok", "path": str(path)})

    @app.route("/api/upload", methods=["POST"])
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

    @app.route("/api/compile", methods=["POST"])
    def api_compile():
        articles = compile_new(base)
        return jsonify({"status": "ok", "articles_created": len(articles)})

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
    def api_rebuild_index():
        entries = rebuild_index(base)
        return jsonify({"status": "ok", "article_count": len(entries)})

    # ─── SPA Fallback ──────────────────────────────────────────

    @app.route("/", defaults={"path": ""})
    @app.route("/<path:path>")
    def serve_spa(path):
        # Serve static file if it exists, otherwise fall back to index.html
        file_path = static_dir / path
        if path and file_path.exists():
            return send_from_directory(str(static_dir), path)
        return send_from_directory(str(static_dir), "index.html")

    return app
