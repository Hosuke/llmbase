"""MCP Server — expose LLMBase as a Model Context Protocol server.

Any MCP-compatible client (Claude Code, Cursor, Windsurf, ClawHub, etc.)
can query, search, and contribute to the knowledge base directly.

Usage:
    python -m tools.mcp_server [--base-dir .]

Or register in Claude Code / client settings:
    "mcpServers": {
      "llmbase": {
        "command": "python",
        "args": ["-m", "tools.mcp_server", "--base-dir", "/path/to/kb"]
      }
    }
"""

import argparse
import asyncio
import json
import logging
import sys
from pathlib import Path

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent

logging.basicConfig(level=logging.INFO, stream=sys.stderr)
logger = logging.getLogger("llmbase.mcp")

# ─── Tool definitions ─────────────────────────────────────

TOOLS = [
    Tool(
        name="kb_search",
        description="Full-text search across the knowledge base wiki articles",
        inputSchema={
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search query"},
                "top_k": {"type": "integer", "description": "Max results (default 10)", "default": 10},
            },
            "required": ["query"],
        },
    ),
    Tool(
        name="kb_ask",
        description="Ask a question against the knowledge base (deep research with context retrieval)",
        inputSchema={
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "Question to ask"},
                "tone": {"type": "string", "description": "Response tone: default, wenyan, scholar, caveman, eli5", "default": "default"},
            },
            "required": ["question"],
        },
    ),
    Tool(
        name="kb_get",
        description="Get a specific wiki article by slug (supports alias resolution: Chinese titles, pinyin, etc.)",
        inputSchema={
            "type": "object",
            "properties": {
                "slug": {"type": "string", "description": "Article slug or any known name (e.g., 'kong', '空', 'emptiness')"},
            },
            "required": ["slug"],
        },
    ),
    Tool(
        name="kb_list",
        description="List all wiki articles with titles and tags",
        inputSchema={
            "type": "object",
            "properties": {
                "tag": {"type": "string", "description": "Filter by tag (optional)"},
            },
        },
    ),
    Tool(
        name="kb_backlinks",
        description="Find all articles that reference a given article",
        inputSchema={
            "type": "object",
            "properties": {
                "slug": {"type": "string", "description": "Article slug to find backlinks for"},
            },
            "required": ["slug"],
        },
    ),
    Tool(
        name="kb_taxonomy",
        description="Get the hierarchical category tree of the knowledge base",
        inputSchema={
            "type": "object",
            "properties": {
                "lang": {"type": "string", "description": "Language: zh, en, ja, zh-en", "default": "zh"},
            },
        },
    ),
    Tool(
        name="kb_stats",
        description="Get knowledge base statistics: article count, word count, health score, etc.",
        inputSchema={"type": "object", "properties": {}},
    ),
    Tool(
        name="kb_ingest",
        description="Ingest a URL into the knowledge base as a raw document",
        inputSchema={
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to ingest"},
            },
            "required": ["url"],
        },
    ),
    Tool(
        name="kb_compile",
        description="Compile new raw documents into wiki articles",
        inputSchema={"type": "object", "properties": {}},
    ),
    Tool(
        name="kb_lint",
        description="Run health checks on the knowledge base",
        inputSchema={
            "type": "object",
            "properties": {
                "fix": {"type": "boolean", "description": "Auto-fix issues (default false)", "default": False},
            },
        },
    ),
    Tool(
        name="kb_export",
        description="Export structured data: article with full context, articles by tag, or subgraph",
        inputSchema={
            "type": "object",
            "properties": {
                "type": {"type": "string", "description": "Export type: article, tag, graph", "enum": ["article", "tag", "graph"]},
                "slug": {"type": "string", "description": "Article slug or tag name"},
                "depth": {"type": "integer", "description": "Graph traversal depth (default 2)", "default": 2},
            },
            "required": ["type", "slug"],
        },
    ),
    Tool(
        name="kb_xici",
        description="Get the guided reading (导读) — an LLM-generated introduction to the knowledge base",
        inputSchema={
            "type": "object",
            "properties": {
                "lang": {"type": "string", "description": "Language: zh, en, ja, zh-en", "default": "zh"},
            },
        },
    ),
]


# ─── Tool handlers ─────────────────────────────────────────

def handle_tool(name: str, arguments: dict, base_dir: Path) -> str:
    """Dispatch tool call to the appropriate handler."""

    # Write operations use the worker job lock to prevent concurrent mutations
    write_tools = {"kb_ingest", "kb_compile", "kb_lint"}
    if name in write_tools:
        from .worker import job_lock
        if not job_lock.acquire(blocking=False):
            return "Another write operation is running. Try again later."

    try:
        return _dispatch_tool(name, arguments, base_dir)
    finally:
        if name in write_tools:
            from .worker import job_lock
            try:
                job_lock.release()
            except RuntimeError:
                pass


def _dispatch_tool(name: str, arguments: dict, base_dir: Path) -> str:
    """Inner dispatch — separated for lock management."""

    if name == "kb_search":
        from .search import search
        results = search(arguments["query"], top_k=arguments.get("top_k", 10), base_dir=base_dir)
        return json.dumps(results, ensure_ascii=False, indent=2)

    elif name == "kb_ask":
        from .query import query_with_search
        result = query_with_search(
            arguments["question"],
            base_dir=base_dir,
            tone=arguments.get("tone", "default"),
            file_back=False,  # MCP queries don't write by default
            return_context=True,
        )
        if isinstance(result, dict):
            return f"{result['answer']}\n\n---\nConsulted: {', '.join(a['title'] for a in result.get('consulted', []))}"
        return result

    elif name == "kb_get":
        from .resolve import load_aliases, resolve_link
        from .config import load_config
        import frontmatter

        cfg = load_config(base_dir)
        concepts_dir = Path(cfg["paths"]["concepts"])
        meta_dir = Path(cfg["paths"]["meta"])
        slug = arguments["slug"]

        article_path = concepts_dir / f"{slug}.md"
        if not article_path.exists():
            aliases = load_aliases(meta_dir)
            resolved = resolve_link(slug, aliases)
            if resolved:
                article_path = concepts_dir / f"{resolved}.md"
                slug = resolved

        if not article_path.exists():
            return f"Article not found: {arguments['slug']}"

        post = frontmatter.load(str(article_path))
        return json.dumps({
            "slug": slug,
            "title": post.metadata.get("title", slug),
            "summary": post.metadata.get("summary", ""),
            "tags": post.metadata.get("tags", []),
            "sources": post.metadata.get("sources", []),
            "content": post.content[:8000],
        }, ensure_ascii=False, indent=2)

    elif name == "kb_list":
        from .config import load_config
        import frontmatter

        cfg = load_config(base_dir)
        concepts_dir = Path(cfg["paths"]["concepts"])
        tag_filter = arguments.get("tag")

        articles = []
        for md_file in sorted(concepts_dir.glob("*.md")):
            post = frontmatter.load(str(md_file))
            tags = post.metadata.get("tags", [])
            if tag_filter and tag_filter not in tags:
                continue
            articles.append({
                "slug": md_file.stem,
                "title": post.metadata.get("title", md_file.stem),
                "tags": tags,
            })
        return json.dumps(articles, ensure_ascii=False, indent=2)

    elif name == "kb_backlinks":
        from .config import load_config
        cfg = load_config(base_dir)
        meta_dir = Path(cfg["paths"]["meta"])
        bl_path = meta_dir / "backlinks.json"
        if not bl_path.exists():
            return "No backlinks data. Run `llmbase compile index` first."
        data = json.loads(bl_path.read_text())
        slug = arguments["slug"]
        backlinks = data.get(slug, [])
        return json.dumps({"slug": slug, "cited_by": backlinks}, ensure_ascii=False)

    elif name == "kb_taxonomy":
        from .taxonomy import build_taxonomy
        lang = arguments.get("lang", "zh")
        categories = build_taxonomy(base_dir, lang)

        def flatten(cats, depth=0):
            lines = []
            for c in cats:
                prefix = "  " * depth
                lines.append(f"{prefix}{c['label']} ({c['total']})")
                lines.extend(flatten(c.get("children", []), depth + 1))
            return lines

        return "\n".join(flatten(categories))

    elif name == "kb_stats":
        from .config import load_config
        cfg = load_config(base_dir)
        concepts_dir = Path(cfg["paths"]["concepts"])
        raw_dir = Path(cfg["paths"]["raw"])

        article_count = len(list(concepts_dir.glob("*.md"))) if concepts_dir.exists() else 0
        raw_count = len(list(raw_dir.glob("*"))) if raw_dir.exists() else 0

        return json.dumps({
            "articles": article_count,
            "raw_documents": raw_count,
        }, indent=2)

    elif name == "kb_ingest":
        from .ingest import ingest_url
        path = ingest_url(arguments["url"], base_dir)
        return f"Ingested to: {path}"

    elif name == "kb_compile":
        from .compile import compile_new
        articles = compile_new(base_dir)
        return f"Compiled {len(articles)} new articles: {articles[:5]}"

    elif name == "kb_lint":
        if arguments.get("fix"):
            from .lint import auto_fix
            fixes = auto_fix(base_dir)
            return f"Applied {len(fixes)} fixes:\n" + "\n".join(f"  - {f}" for f in fixes[:20])
        else:
            from .lint import lint
            results = lint(base_dir)
            lines = [f"Total issues: {results['total_issues']}"]
            for k, v in results.items():
                if k != "total_issues" and isinstance(v, list) and v:
                    lines.append(f"  {k}: {len(v)}")
            return "\n".join(lines)

    elif name == "kb_export":
        from .export import export_article, export_by_tag, export_graph
        export_type = arguments.get("type", "article")
        slug = arguments.get("slug", "")
        if export_type == "article":
            result = export_article(slug, base_dir)
            return json.dumps(result, ensure_ascii=False, indent=2) if result else f"Article not found: {slug}"
        elif export_type == "tag":
            return json.dumps(export_by_tag(slug, base_dir), ensure_ascii=False, indent=2)
        elif export_type == "graph":
            depth = arguments.get("depth", 2)
            return json.dumps(export_graph(slug, depth, base_dir), ensure_ascii=False, indent=2)
        return f"Unknown export type: {export_type}"

    elif name == "kb_xici":
        from .xici import get_xici
        lang = arguments.get("lang", "zh")
        xici = get_xici(base_dir, lang)
        if xici.get("text"):
            return f"{xici['text']}\n\nThemes: {', '.join(xici.get('themes', []))}"
        return "No guided reading generated yet. Trigger via worker or POST /api/xici/generate."

    return f"Unknown tool: {name}"


# ─── MCP Server setup ─────────────────────────────────────

def create_server(base_dir: Path) -> Server:
    server = Server("llmbase")

    @server.list_tools()
    async def list_tools():
        return TOOLS

    @server.call_tool()
    async def call_tool(name: str, arguments: dict):
        known_tools = {t.name for t in TOOLS}
        if name not in known_tools:
            raise ValueError(f"Unknown tool: {name}")
        try:
            result = await asyncio.to_thread(handle_tool, name, arguments, base_dir)
            return [TextContent(type="text", text=result)]
        except Exception as e:
            logger.error(f"Tool {name} failed: {e}")
            raise  # Let MCP framework handle as isError=true

    return server


async def main():
    parser = argparse.ArgumentParser(description="LLMBase MCP Server")
    parser.add_argument("--base-dir", type=str, default=".", help="Knowledge base directory")
    args = parser.parse_args()

    base_dir = Path(args.base_dir).resolve()
    logger.info(f"Starting LLMBase MCP server (base: {base_dir})")

    server = create_server(base_dir)
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
