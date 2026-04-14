"""Operations contract — single source of truth for CLI / HTTP / MCP.

Every knowledge-base operation is declared exactly once, here. The CLI
(`tools/cli.py`), the agent HTTP server (`tools/agent_api.py`), and the
MCP server (`tools/mcp_server.py`) all dispatch through this registry.

Adding or modifying an op — change it here and all three surfaces update.

Downstream projects extend the contract via ``operations.register``::

    from tools.operations import register, Operation
    register(Operation(
        name="kb_custom",
        description="...",
        handler=my_handler,
        params={"type": "object", ...},
        writes=False,
    ))

The handler signature is ``f(base_dir: Path, **kwargs) -> Any``. Return
values should be JSON-serialisable (dicts, lists, strings, numbers).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable


@dataclass
class Operation:
    name: str
    description: str
    handler: Callable[..., Any]
    params: dict = field(default_factory=lambda: {"type": "object", "properties": {}})
    writes: bool = False  # True → acquire worker.job_lock before dispatch
    category: str = "general"  # grouping for CLI help / docs


_REGISTRY: dict[str, Operation] = {}


def register(op: Operation) -> Operation:
    """Register an operation. Idempotent by name (last write wins)."""
    _REGISTRY[op.name] = op
    return op


def get(name: str) -> Operation | None:
    return _REGISTRY.get(name)


def all_operations() -> list[Operation]:
    return list(_REGISTRY.values())


def _needs_write_lock(op: "Operation", args: dict) -> bool:
    """Some ops are normally read-only but escalate to writes based on args.

    Currently only kb_ask does this (promote=True triggers index rebuilds
    and concept writes inside query_with_search).
    """
    if op.writes:
        return True
    if op.name == "kb_ask" and args.get("promote") is True:
        return True
    if op.name == "kb_lint" and args.get("fix") is True:
        return True
    return False


def dispatch(name: str, base_dir: Path, arguments: dict | None = None) -> Any:
    """Invoke an operation by name with lock management for writes.

    Raises KeyError if the operation is not registered.
    Returns whatever the handler returns.
    """
    op = _REGISTRY.get(name)
    if op is None:
        raise KeyError(f"unknown operation: {name}")
    args = arguments or {}
    if _needs_write_lock(op, args):
        from .worker import job_lock
        if not job_lock.acquire(blocking=False):
            raise RuntimeError("another write operation is running")
        try:
            return op.handler(base_dir, **args)
        finally:
            try:
                job_lock.release()
            except RuntimeError:
                pass
    return op.handler(base_dir, **args)


# ─── Canonical handler implementations ──────────────────────────────
# Kept terse: each handler imports its deps lazily so operations.py has
# no heavy import graph (MCP stdio wants fast startup).


def _op_search(base_dir: Path, query: str, top_k: int = 10) -> dict:
    from .search import search
    return {"results": search(query, top_k=top_k, base_dir=base_dir)}


def _op_search_raw(base_dir: Path, query: str, top_k: int = 10) -> dict:
    from .search import search_raw
    return {"results": search_raw(query, top_k=top_k, base_dir=base_dir)}


def _op_ask(
    base_dir: Path,
    question: str,
    tone: str = "default",
    file_back: bool = False,
    deep: bool = True,
    promote: bool = False,
) -> dict:
    from .query import query, query_with_search
    if deep:
        result = query_with_search(
            question,
            base_dir=base_dir,
            tone=tone,
            file_back=file_back,
            return_context=True,
            promote=promote,
        )
        if isinstance(result, dict):
            return result
        return {"answer": result}
    answer = query(question, file_back=file_back, base_dir=base_dir, tone=tone)
    return {"answer": answer}


def _op_get(base_dir: Path, slug: str) -> dict:
    import frontmatter
    from .config import load_config
    from .resolve import load_aliases, resolve_link

    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])

    article_path = concepts_dir / f"{slug}.md"
    resolved = slug
    if not article_path.exists():
        aliases = load_aliases(meta_dir)
        alt = resolve_link(slug, aliases)
        if alt:
            resolved = alt
            article_path = concepts_dir / f"{alt}.md"
    if not article_path.exists():
        return {"found": False, "slug": slug}

    post = frontmatter.load(str(article_path))
    return {
        "found": True,
        "slug": resolved,
        "title": post.metadata.get("title", resolved),
        "summary": post.metadata.get("summary", ""),
        "tags": post.metadata.get("tags", []),
        "sources": post.metadata.get("sources", []),
        "content": post.content,
    }


def _op_list(base_dir: Path, tag: str | None = None) -> dict:
    import frontmatter
    from .config import load_config

    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    if not concepts_dir.exists():
        return {"articles": []}

    articles = []
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        tags = post.metadata.get("tags", [])
        if tag and tag not in tags:
            continue
        articles.append({
            "slug": md_file.stem,
            "title": post.metadata.get("title", md_file.stem),
            "summary": post.metadata.get("summary", ""),
            "tags": tags,
        })
    return {"articles": articles}


def _op_backlinks(base_dir: Path, slug: str) -> dict:
    import json
    from .config import load_config

    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    bl_path = meta_dir / "backlinks.json"
    if not bl_path.exists():
        return {"slug": slug, "cited_by": [], "note": "run `llmbase compile index` first"}
    data = json.loads(bl_path.read_text())
    return {"slug": slug, "cited_by": data.get(slug, [])}


def _op_taxonomy(base_dir: Path, lang: str = "zh") -> dict:
    from .taxonomy import build_taxonomy
    return {"categories": build_taxonomy(base_dir, lang)}


def _op_stats(base_dir: Path) -> dict:
    from .config import load_config

    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    raw_dir = Path(cfg["paths"]["raw"])
    outputs_dir = Path(cfg["paths"]["outputs"])

    article_count = len(list(concepts_dir.glob("*.md"))) if concepts_dir.exists() else 0
    raw_count = len(list(raw_dir.glob("*"))) if raw_dir.exists() else 0
    output_count = len(list(outputs_dir.glob("*.md"))) if outputs_dir.exists() else 0
    total_words = 0
    if concepts_dir.exists():
        for f in concepts_dir.glob("*.md"):
            total_words += len(f.read_text().split())
    return {
        "articles": article_count,
        "raw_documents": raw_count,
        "filed_outputs": output_count,
        "total_words": total_words,
    }


def _op_ingest(base_dir: Path, source: str | None = None, url: str | None = None) -> dict:
    """Ingest a URL or local file path. ``url`` is a legacy alias for ``source``."""
    from .ingest import ingest_url, ingest_file
    target = source or url
    if not target:
        raise TypeError("kb_ingest requires 'source' (or legacy 'url')")
    if target.startswith(("http://", "https://")):
        path = ingest_url(target, base_dir)
    else:
        path = ingest_file(target, base_dir)
    return {"path": str(path)}


def _op_compile(base_dir: Path, full: bool = False) -> dict:
    from .compile import compile_new, compile_all
    articles = compile_all(base_dir) if full else compile_new(base_dir)
    return {"articles_created": len(articles), "articles": articles}


def _op_lint(base_dir: Path, deep: bool = False, fix: bool = False) -> dict:
    """Lint the KB. fix=True is a legacy shortcut that delegates to kb_lint_fix."""
    if fix:
        return _op_lint_fix(base_dir)
    from .lint import lint, lint_deep
    return {"report": lint_deep(base_dir) if deep else lint(base_dir)}


def _op_lint_fix(base_dir: Path) -> dict:
    from .lint import auto_fix
    fixes = auto_fix(base_dir)
    return {"fixes": fixes, "fix_count": len(fixes)}


def _op_export(base_dir: Path, type: str, slug: str, depth: int = 2) -> dict:
    """Legacy unified export dispatcher (pre-0.6.0 MCP clients)."""
    if type == "article":
        return _op_export_article(base_dir, slug)
    if type == "tag":
        return _op_export_tag(base_dir, slug)
    if type == "graph":
        return _op_export_graph(base_dir, slug, depth=depth)
    raise TypeError(f"unknown export type: {type!r}")


def _op_export_article(base_dir: Path, slug: str) -> dict:
    from .export import export_article
    result = export_article(slug, base_dir)
    return result or {"found": False, "slug": slug}


def _op_export_tag(base_dir: Path, tag: str) -> dict:
    from .export import export_by_tag
    return export_by_tag(tag, base_dir)


def _op_export_graph(base_dir: Path, slug: str, depth: int = 2) -> dict:
    from .export import export_graph
    return export_graph(slug, depth, base_dir)


def _op_rebuild_index(base_dir: Path) -> dict:
    from .compile import rebuild_index
    entries = rebuild_index(base_dir)
    return {"article_count": len(entries)}


def _op_xici(base_dir: Path, lang: str = "zh") -> dict:
    from .xici import get_xici
    return get_xici(base_dir, lang)


# ─── Canonical registry ─────────────────────────────────────────────

_CANONICAL: list[Operation] = [
    Operation(
        name="kb_search",
        description="Full-text search across the knowledge base wiki articles.",
        handler=_op_search,
        params={
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search query"},
                "top_k": {"type": "integer", "default": 10},
            },
            "required": ["query"],
        },
        category="read",
    ),
    Operation(
        name="kb_search_raw",
        description=(
            "Fallback full-text search over the raw/ ingest directory "
            "(pre-compile source material). Use when kb_search misses — "
            "raw holds verbatim scraped sources, dictionaries, and book "
            "chapters that may contain exact wording lost during compile."
        ),
        handler=_op_search_raw,
        params={
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search query"},
                "top_k": {"type": "integer", "default": 10},
            },
            "required": ["query"],
        },
        category="read",
    ),
    Operation(
        name="kb_ask",
        description=(
            "Ask a question against the knowledge base (deep research with "
            "context retrieval). Set promote=true to let the LLM judge "
            "whether to sediment the Q&A as a new wiki concept."
        ),
        handler=_op_ask,
        params={
            "type": "object",
            "properties": {
                "question": {"type": "string"},
                "tone": {"type": "string", "default": "default"},
                "file_back": {"type": "boolean", "default": False},
                "deep": {"type": "boolean", "default": True},
                "promote": {"type": "boolean", "default": False},
            },
            "required": ["question"],
        },
        writes=False,  # promote=True escalates to write via its own path
        category="read",
    ),
    Operation(
        name="kb_get",
        description="Get a wiki article by slug (alias-aware: accepts Chinese/pinyin/English names).",
        handler=_op_get,
        params={
            "type": "object",
            "properties": {"slug": {"type": "string"}},
            "required": ["slug"],
        },
        category="read",
    ),
    Operation(
        name="kb_list",
        description="List all wiki articles with titles and tags. Optional tag filter.",
        handler=_op_list,
        params={
            "type": "object",
            "properties": {"tag": {"type": "string"}},
        },
        category="read",
    ),
    Operation(
        name="kb_backlinks",
        description="Find all articles that reference a given article.",
        handler=_op_backlinks,
        params={
            "type": "object",
            "properties": {"slug": {"type": "string"}},
            "required": ["slug"],
        },
        category="read",
    ),
    Operation(
        name="kb_taxonomy",
        description="Get the hierarchical category tree of the knowledge base.",
        handler=_op_taxonomy,
        params={
            "type": "object",
            "properties": {"lang": {"type": "string", "default": "zh"}},
        },
        category="read",
    ),
    Operation(
        name="kb_stats",
        description="Knowledge-base statistics: article count, raw count, word count.",
        handler=_op_stats,
        params={"type": "object", "properties": {}},
        category="read",
    ),
    Operation(
        name="kb_ingest",
        description="Ingest a URL or local file path into the raw corpus.",
        handler=_op_ingest,
        params={
            "type": "object",
            "properties": {
                "source": {"type": "string", "description": "URL or local file path"},
                "url": {"type": "string", "description": "Legacy alias for source"},
            },
        },
        writes=True,
        category="write",
    ),
    Operation(
        name="kb_compile",
        description="Compile raw documents into wiki articles. full=true recompiles everything.",
        handler=_op_compile,
        params={
            "type": "object",
            "properties": {"full": {"type": "boolean", "default": False}},
        },
        writes=True,
        category="write",
    ),
    Operation(
        name="kb_lint",
        description="Run health checks on the KB. Legacy: fix=true delegates to kb_lint_fix.",
        handler=_op_lint,
        params={
            "type": "object",
            "properties": {
                "deep": {"type": "boolean", "default": False},
                "fix": {"type": "boolean", "default": False, "description": "Legacy: run auto_fix"},
            },
        },
        category="read",
    ),
    Operation(
        name="kb_lint_fix",
        description="Auto-fix lint issues (metadata, broken links, duplicates, categories).",
        handler=_op_lint_fix,
        params={"type": "object", "properties": {}},
        writes=True,
        category="write",
    ),
    Operation(
        name="kb_export_article",
        description="Export a single article with full context (linked articles, backlinks).",
        handler=_op_export_article,
        params={
            "type": "object",
            "properties": {"slug": {"type": "string"}},
            "required": ["slug"],
        },
        category="read",
    ),
    Operation(
        name="kb_export_tag",
        description="Export all articles with a given tag.",
        handler=_op_export_tag,
        params={
            "type": "object",
            "properties": {"tag": {"type": "string"}},
            "required": ["tag"],
        },
        category="read",
    ),
    Operation(
        name="kb_export_graph",
        description="Export the subgraph around an article up to a given depth.",
        handler=_op_export_graph,
        params={
            "type": "object",
            "properties": {
                "slug": {"type": "string"},
                "depth": {"type": "integer", "default": 2},
            },
            "required": ["slug"],
        },
        category="read",
    ),
    Operation(
        name="kb_export",
        description="Legacy unified export (type: article/tag/graph). Prefer kb_export_article/tag/graph.",
        handler=_op_export,
        params={
            "type": "object",
            "properties": {
                "type": {"type": "string", "enum": ["article", "tag", "graph"]},
                "slug": {"type": "string"},
                "depth": {"type": "integer", "default": 2},
            },
            "required": ["type", "slug"],
        },
        category="read",
    ),
    Operation(
        name="kb_rebuild_index",
        description="Rebuild the wiki index, aliases, backlinks metadata.",
        handler=_op_rebuild_index,
        params={"type": "object", "properties": {}},
        writes=True,
        category="write",
    ),
    Operation(
        name="kb_xici",
        description="Get the guided reading (导读) — an LLM-generated introduction.",
        handler=_op_xici,
        params={
            "type": "object",
            "properties": {"lang": {"type": "string", "default": "zh"}},
        },
        category="read",
    ),
]


for _op in _CANONICAL:
    register(_op)
