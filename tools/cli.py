"""Main CLI entry point for LLMBase."""

from pathlib import Path

import click
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich.markdown import Markdown

from .config import load_config, ensure_dirs

console = Console()


@click.group()
@click.option("--base-dir", type=click.Path(exists=True), default=".", help="Project base directory")
@click.pass_context
def cli(ctx, base_dir):
    """LLMBase - LLM-powered personal knowledge base."""
    ctx.ensure_object(dict)
    ctx.obj["base_dir"] = Path(base_dir).resolve()


# ─── Ingest commands ───────────────────────────────────────────────

@cli.group()
def ingest():
    """Ingest raw documents into the knowledge base."""
    pass


@ingest.command("url")
@click.argument("url")
@click.pass_context
def ingest_url_cmd(ctx, url):
    """Ingest a web article by URL."""
    from .ingest import ingest_url
    with console.status("Fetching and converting..."):
        path = ingest_url(url, ctx.obj["base_dir"])
    console.print(f"[green]✓[/green] Ingested to: {path}")


@ingest.command("file")
@click.argument("file_path", type=click.Path(exists=True))
@click.pass_context
def ingest_file_cmd(ctx, file_path):
    """Ingest a local file."""
    from .ingest import ingest_file
    path = ingest_file(file_path, ctx.obj["base_dir"])
    console.print(f"[green]✓[/green] Ingested to: {path}")


@ingest.command("pdf")
@click.argument("pdf_path", type=click.Path(exists=True))
@click.option("--chunk-pages", type=int, default=20, help="Pages per chunk (0 = single doc)")
@click.pass_context
def ingest_pdf_cmd(ctx, pdf_path, chunk_pages):
    """Ingest a PDF file, converting to markdown chunks automatically."""
    from .pdf import ingest_pdf
    with console.status(f"Processing PDF ({chunk_pages} pages/chunk)..."):
        paths = ingest_pdf(pdf_path, chunk_pages, ctx.obj["base_dir"])
    console.print(f"[green]✓[/green] Ingested PDF into {len(paths)} chunks:")
    for p in paths:
        console.print(f"  • {p}")


@ingest.command("dir")
@click.argument("dir_path", type=click.Path(exists=True))
@click.pass_context
def ingest_dir_cmd(ctx, dir_path):
    """Ingest all supported files from a directory."""
    from .ingest import ingest_directory
    with console.status("Ingesting directory..."):
        paths = ingest_directory(dir_path, ctx.obj["base_dir"])
    console.print(f"[green]✓[/green] Ingested {len(paths)} files")


@ingest.command("browse")
@click.argument("url")
@click.pass_context
def ingest_browse_cmd(ctx, url):
    """Ingest a web page via opencli browser (uses your local Chrome session)."""
    from .browser import is_opencli_available, fetch_article
    from .ingest import ingest_url

    if not is_opencli_available():
        console.print("[yellow]opencli not found. Install: npm install -g @jackwener/opencli[/yellow]")
        console.print("[dim]Falling back to HTTP fetch...[/dim]")
        with console.status("Fetching..."):
            path = ingest_url(url, ctx.obj["base_dir"])
        console.print(f"[green]✓[/green] Ingested to: {path}")
        return

    with console.status("Browsing with opencli..."):
        article = fetch_article(url)

    if article.get("error"):
        console.print(f"[red]Browser fetch failed: {article['error']}[/red]")
        console.print("[dim]Falling back to HTTP fetch...[/dim]")
        with console.status("Fetching..."):
            path = ingest_url(url, ctx.obj["base_dir"])
        console.print(f"[green]✓[/green] Ingested to: {path}")
    else:
        # Save as raw document
        from .ingest import _slugify
        from datetime import datetime, timezone
        import frontmatter as fm
        raw_dir = Path(ctx.obj["base_dir"]) / "raw"
        slug = _slugify(article["title"] or "untitled")
        doc_dir = raw_dir / slug
        doc_dir.mkdir(parents=True, exist_ok=True)
        post = fm.Post(article["content"])
        post.metadata["title"] = article["title"]
        post.metadata["source"] = url
        post.metadata["ingested_at"] = datetime.now(timezone.utc).isoformat()
        post.metadata["type"] = "browser_article"
        post.metadata["compiled"] = False
        doc_path = doc_dir / "index.md"
        doc_path.write_text(fm.dumps(post), encoding="utf-8")
        console.print(f"[green]✓[/green] Ingested via browser to: {doc_path}")


@ingest.command("wikisource-learn")
@click.option("--list", "reading_list", type=click.Choice(["confucianism", "daoism", "mohism", "legalism", "military", "history", "poetry", "divination", "zhuzi"]), default=None)
@click.option("--batch", type=int, default=3, help="Number of works per run")
@click.pass_context
def ingest_wikisource_learn_cmd(ctx, reading_list, batch):
    """Progressive learning from zh.wikisource.org (维基文库)."""
    from .wikisource import learn
    console.print(f"[cyan]Learning from Wikisource (list: {reading_list or 'all'})...[/cyan]")
    with console.status("Fetching from Wikisource..."):
        results = learn(reading_list, batch, ctx.obj["base_dir"])
    if results:
        console.print(f"[green]✓[/green] Ingested {len(results)} works: {', '.join(results)}")
    else:
        console.print("[yellow]No new works to ingest.[/yellow]")


@ingest.command("wikisource-work")
@click.argument("title")
@click.pass_context
def ingest_wikisource_work_cmd(ctx, title):
    """Ingest a specific work from Wikisource (e.g. 道德經, 論語)."""
    from .wikisource import ingest_work
    with console.status(f"Fetching {title}..."):
        paths = ingest_work(title, ctx.obj["base_dir"])
    console.print(f"[green]✓[/green] Ingested {len(paths)} pages from {title}")


@ingest.command("cbeta-learn")
@click.option("--category", type=str, default=None, help="Category: agama, bore, fahua, huayan, chanzong, jingtu, etc.")
@click.option("--batch", type=int, default=5, help="Number of sutras per run")
@click.pass_context
def ingest_cbeta_learn_cmd(ctx, category, batch):
    """Progressive learning from CBETA. Each run ingests a batch of new sutras."""
    from .cbeta import learn, status
    s = status(ctx.obj["base_dir"])
    console.print(f"[dim]Progress: {s['total_ingested']} sutras ingested so far[/dim]")
    cat_label = category or "all categories (sequential)"
    console.print(f"[cyan]Learning {batch} sutras from {cat_label}...[/cyan]")
    with console.status("Fetching and ingesting..."):
        results = learn(category, batch, ctx.obj["base_dir"])
    if results:
        console.print(f"[green]✓[/green] Ingested {len(results)} new sutras:")
        for w in results:
            console.print(f"  • {w}")
        console.print(f"[dim]Total progress: {s['total_ingested'] + len(results)} sutras[/dim]")
    else:
        console.print("[yellow]No new sutras to ingest in this category.[/yellow]")


@ingest.command("cbeta-status")
@click.pass_context
def ingest_cbeta_status_cmd(ctx):
    """Show CBETA learning progress."""
    from .cbeta import status
    s = status(ctx.obj["base_dir"])
    console.print(f"[cyan]CBETA Learning Progress[/cyan]")
    console.print(f"  Total ingested: [green]{s['total_ingested']}[/green] sutras")
    console.print(f"  Last run: {s.get('last_run', 'never')}")
    if s.get("ingested_works"):
        console.print(f"  Recent: {', '.join(s['ingested_works'][:10])}")


@ingest.command("cbeta-work")
@click.argument("work_id")
@click.pass_context
def ingest_cbeta_work_cmd(ctx, work_id):
    """Ingest a specific CBETA work by ID (e.g. T0001, T0235, X1456)."""
    from .cbeta import ingest_work
    with console.status(f"Fetching {work_id}..."):
        path = ingest_work(work_id, base_dir=ctx.obj["base_dir"])
    if path:
        console.print(f"[green]✓[/green] Ingested: {path}")
    else:
        console.print(f"[yellow]Already ingested or not found: {work_id}[/yellow]")


@ingest.command("ctext-book")
@click.argument("book_name")
@click.argument("book_path")
@click.option("--delay", type=float, default=1.5, help="Delay between requests (seconds)")
@click.option("--browser", is_flag=True, help="Use opencli browser instead of HTTP (handles anti-scraping)")
@click.pass_context
def ingest_ctext_book_cmd(ctx, book_name, book_path, delay, browser):
    """Ingest a book from ctext.org. Example: llmbase ingest ctext-book 论语 /analects/zh"""
    from .ctext import ingest_book
    method = "opencli browser" if browser else "HTTP"
    console.print(f"[cyan]Ingesting {book_name} via {method}...[/cyan]")
    paths = ingest_book(book_name, book_path, delay, ctx.obj["base_dir"], browser)
    console.print(f"[green]✓[/green] Ingested {len(paths)} chapters from {book_name}")


@ingest.command("ctext-catalog")
@click.argument("catalog", type=click.Choice(["confucianism", "daoism", "mohism", "legalism", "military", "histories", "medicine"]))
@click.option("--delay", type=float, default=1.5, help="Delay between requests (seconds)")
@click.option("--browser", is_flag=True, help="Use opencli browser instead of HTTP")
@click.pass_context
def ingest_ctext_catalog_cmd(ctx, catalog, delay, browser):
    """Ingest an entire catalog from ctext.org (e.g. confucianism, daoism)."""
    from .ctext import ingest_catalog
    method = "opencli browser" if browser else "HTTP"
    console.print(f"[cyan]Ingesting catalog '{catalog}' via {method}...[/cyan]")
    results = ingest_catalog(catalog, delay, ctx.obj["base_dir"], browser)
    total = sum(len(v) for v in results.values())
    console.print(f"\n[green]✓[/green] Ingested {total} chapters from {len(results)} books:")
    for book, paths in results.items():
        console.print(f"  • {book}: {len(paths)} chapters")


@ingest.command("list")
@click.pass_context
def ingest_list_cmd(ctx):
    """List all raw documents."""
    from .ingest import list_raw
    docs = list_raw(ctx.obj["base_dir"])
    if not docs:
        console.print("[yellow]No raw documents found.[/yellow]")
        return

    table = Table(title="Raw Documents")
    table.add_column("Title", style="cyan")
    table.add_column("Type", style="green")
    table.add_column("Compiled", style="yellow")
    table.add_column("Ingested At")

    for doc in docs:
        compiled = "✓" if doc["compiled"] else "✗"
        table.add_row(doc["title"], doc["type"], compiled, doc["ingested_at"][:10])

    console.print(table)


# ─── Compile commands ──────────────────────────────────────────────

@cli.group()
def compile():
    """Compile raw documents into the wiki."""
    pass


@compile.command("new")
@click.option("--batch-size", type=int, default=None, help="Max documents to process")
@click.pass_context
def compile_new_cmd(ctx, batch_size):
    """Compile only new/unprocessed raw documents."""
    from .compile import compile_new
    with console.status("Compiling new documents..."):
        articles = compile_new(ctx.obj["base_dir"], batch_size)
    if articles:
        console.print(f"[green]✓[/green] Created {len(articles)} articles:")
        for a in articles:
            console.print(f"  • {a}")
    else:
        console.print("[yellow]No new documents to compile.[/yellow]")


@compile.command("all")
@click.pass_context
def compile_all_cmd(ctx):
    """Recompile all raw documents (reset and rebuild)."""
    from .compile import compile_all
    with console.status("Recompiling all documents..."):
        articles = compile_all(ctx.obj["base_dir"])
    console.print(f"[green]✓[/green] Compiled {len(articles)} articles")


@compile.command("index")
@click.pass_context
def compile_index_cmd(ctx):
    """Rebuild the wiki index without recompiling articles."""
    from .compile import rebuild_index
    entries = rebuild_index(ctx.obj["base_dir"])
    console.print(f"[green]✓[/green] Index rebuilt with {len(entries)} articles")


# ─── Query commands ────────────────────────────────────────────────

@cli.command()
@click.argument("question")
@click.option("--format", "output_format", type=click.Choice(["markdown", "marp", "chart"]), default="markdown")
@click.option("--tone", type=click.Choice(["default", "caveman", "wenyan", "scholar", "eli5"]), default="default",
              help="Response tone: caveman (primitive), wenyan (文言文), scholar (academic), eli5 (simple)")
@click.option("--file-back", is_flag=True, help="Save the answer back into the wiki")
@click.option("--deep", is_flag=True, help="Use multi-step search for complex queries")
@click.pass_context
def query(ctx, question, output_format, tone, file_back, deep):
    """Ask a question against the knowledge base."""
    from .query import query as do_query, query_with_search

    with console.status("Researching..."):
        if deep:
            answer = query_with_search(question, ctx.obj["base_dir"], tone=tone, file_back=file_back)
        else:
            answer = do_query(question, output_format, file_back, ctx.obj["base_dir"], tone=tone)

    console.print(Panel(Markdown(answer), title="Answer", border_style="green"))

    if file_back:
        console.print("[dim]Answer filed back to wiki/outputs/[/dim]")


# ─── Search commands ───────────────────────────────────────────────

@cli.group()
def search():
    """Search the knowledge base."""
    pass


@search.command("query")
@click.argument("query_text")
@click.option("--top-k", type=int, default=10)
@click.option("--json-output", is_flag=True, help="Output as JSON (for LLM tool use)")
@click.pass_context
def search_query_cmd(ctx, query_text, top_k, json_output):
    """Full-text search over the wiki."""
    from .search import search as do_search, search_cli
    import json

    if json_output:
        results = do_search(query_text, top_k, ctx.obj["base_dir"])
        click.echo(json.dumps(results, indent=2))
    else:
        output = search_cli(query_text, ctx.obj["base_dir"])
        console.print(output)


@search.command("serve")
@click.option("--port", type=int, default=None)
@click.pass_context
def search_serve_cmd(ctx, port):
    """Start the search web UI."""
    from .search import create_search_app

    cfg = load_config(ctx.obj["base_dir"])
    if port is None:
        port = cfg.get("search", {}).get("port", 5555)

    app = create_search_app(ctx.obj["base_dir"])
    console.print(f"[green]Search UI running at http://localhost:{port}[/green]")
    app.run(host="0.0.0.0", port=port, debug=False)


# ─── Lint commands ─────────────────────────────────────────────────

@cli.group()
def lint():
    """Run health checks on the knowledge base."""
    pass


@lint.command("check")
@click.pass_context
def lint_check_cmd(ctx):
    """Run structural lint checks."""
    from .lint import lint as do_lint

    results = do_lint(ctx.obj["base_dir"])

    for category, issues in results.items():
        if category == "total_issues":
            continue
        if issues:
            console.print(f"\n[bold red]{category}[/bold red] ({len(issues)} issues):")
            for issue in issues:
                console.print(f"  • {issue}")

    total = results["total_issues"]
    if total == 0:
        console.print("[green]✓ No issues found![/green]")
    else:
        console.print(f"\n[yellow]Total: {total} issues[/yellow]")


@lint.command("deep")
@click.pass_context
def lint_deep_cmd(ctx):
    """Use LLM for deep content quality check."""
    from .lint import lint_deep

    with console.status("Running deep analysis..."):
        report = lint_deep(ctx.obj["base_dir"])

    console.print(Panel(Markdown(report), title="Deep Lint Report", border_style="yellow"))


@lint.command("fix")
@click.pass_context
def lint_fix_cmd(ctx):
    """Auto-fix common issues using LLM."""
    from .lint import auto_fix

    with console.status("Auto-fixing issues..."):
        fixes = auto_fix(ctx.obj["base_dir"])

    if fixes:
        for fix in fixes:
            console.print(f"[green]✓[/green] {fix}")
    else:
        console.print("[green]Nothing to fix.[/green]")


@lint.command("normalize-tags")
@click.pass_context
def lint_normalize_tags_cmd(ctx):
    """Normalize tags: merge synonymous tags across the wiki using LLM."""
    from .lint import normalize_tags

    console.print("[cyan]Normalizing tags across wiki...[/cyan]")
    with console.status("Analyzing tag synonyms..."):
        fixes = normalize_tags(ctx.obj["base_dir"])

    if fixes:
        console.print(f"[green]Normalized {len(fixes)} article(s)[/green]")
        for f in fixes[:10]:
            console.print(f"  [green]✓[/green] {f}")
        if len(fixes) > 10:
            console.print(f"  ...and {len(fixes) - 10} more")
    else:
        console.print("[green]Tags are already clean.[/green]")


@lint.command("clean")
@click.pass_context
def lint_clean_cmd(ctx):
    """Remove garbage/empty stub articles."""
    from .lint import check_stubs, clean_garbage

    cfg = load_config(ctx.obj["base_dir"])
    stubs = check_stubs(cfg)
    if not stubs:
        console.print("[green]No garbage articles found.[/green]")
        return

    for s in stubs:
        console.print(f"  [yellow]![/yellow] {s}")

    console.print(f"\n[cyan]Removing {len(stubs)} garbage article(s)...[/cyan]")
    removed = clean_garbage(ctx.obj["base_dir"])
    for r in removed:
        console.print(f"  [green]✓[/green] Removed: {r}")


@lint.command("dedup")
@click.pass_context
def lint_dedup_cmd(ctx):
    """Detect and merge duplicate articles using LLM."""
    from .lint import check_duplicates, merge_duplicates

    cfg = load_config(ctx.obj["base_dir"])
    console.print("[cyan]Scanning for duplicates...[/cyan]")
    with console.status("Analyzing article similarity..."):
        dupes = check_duplicates(cfg)

    if not dupes:
        console.print("[green]No duplicates found.[/green]")
        return

    for d in dupes:
        console.print(f"  [yellow]![/yellow] {d}")

    console.print(f"\n[cyan]Merging {len(dupes)} duplicate pair(s)...[/cyan]")
    with console.status("Merging..."):
        fixes = merge_duplicates(ctx.obj["base_dir"])

    for fix in fixes:
        console.print(f"  [green]✓[/green] {fix}")
    if not fixes:
        console.print("  No confirmed merges")


@lint.command("heal")
@click.pass_context
def lint_heal_cmd(ctx):
    """Full health cycle: check -> fix -> recheck -> report."""
    from .lint import lint as do_lint, auto_fix
    import json
    from datetime import datetime, timezone

    # Phase 1: Check
    console.print("[cyan]Phase 1: Checking...[/cyan]")
    results = do_lint(ctx.obj["base_dir"])
    total_before = results["total_issues"]
    for category, issues in results.items():
        if category == "total_issues" or not issues:
            continue
        console.print(f"  {category}: {len(issues)} issues")
    console.print(f"  Total: {total_before} issues")

    if total_before == 0:
        console.print("[green]No issues found. Knowledge base is healthy.[/green]")
        return

    # Phase 2: Fix
    console.print("\n[cyan]Phase 2: Fixing...[/cyan]")
    with console.status("Auto-fixing issues..."):
        fixes = auto_fix(ctx.obj["base_dir"])
    if fixes:
        for fix in fixes:
            console.print(f"  [green]✓[/green] {fix}")
    else:
        console.print("  No auto-fixes available")

    # Phase 3: Recheck
    console.print("\n[cyan]Phase 3: Rechecking...[/cyan]")
    results_after = do_lint(ctx.obj["base_dir"])
    total_after = results_after["total_issues"]

    # Phase 4: Persist report
    cfg = load_config(ctx.obj["base_dir"])
    meta_dir = Path(cfg["paths"]["meta"])
    meta_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "checked_at": datetime.now(timezone.utc).isoformat(),
        "issues_before": total_before,
        "issues_after": total_after,
        "results": results_after,
        "fixes_applied": fixes,
    }
    health_path = meta_dir / "health.json"
    health_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")

    # Summary
    resolved = total_before - total_after
    console.print(f"\n[bold]Summary:[/bold] {resolved} resolved, {total_after} remaining")
    if total_after == 0:
        console.print("[green]Knowledge base is now healthy.[/green]")
    console.print(f"[dim]Report saved to {health_path}[/dim]")


# ─── Stats command ─────────────────────────────────────────────────

@cli.command()
@click.pass_context
def stats(ctx):
    """Show knowledge base statistics."""
    cfg = load_config(ctx.obj["base_dir"])
    raw_dir = Path(cfg["paths"]["raw"])
    concepts_dir = Path(cfg["paths"]["concepts"])
    outputs_dir = Path(cfg["paths"]["outputs"])

    raw_count = len(list(raw_dir.glob("*"))) if raw_dir.exists() else 0
    article_count = len(list(concepts_dir.glob("*.md"))) if concepts_dir.exists() else 0
    output_count = len(list(outputs_dir.glob("*.md"))) if outputs_dir.exists() else 0

    # Count total words
    total_words = 0
    if concepts_dir.exists():
        for f in concepts_dir.glob("*.md"):
            total_words += len(f.read_text().split())

    table = Table(title="Knowledge Base Stats")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", style="green", justify="right")

    table.add_row("Raw documents", str(raw_count))
    table.add_row("Wiki articles", str(article_count))
    table.add_row("Filed outputs", str(output_count))
    table.add_row("Total words", f"{total_words:,}")

    console.print(table)


# ─── Serve command (agent API) ─────────────────────────────────────

@cli.command()
@click.option("--port", type=int, default=5556)
@click.pass_context
def serve(ctx, port):
    """Start the agent-facing HTTP API server."""
    from .agent_api import create_agent_server

    app = create_agent_server(ctx.obj["base_dir"], port)
    console.print(f"[green]Agent API running at http://localhost:{port}[/green]")
    console.print("[dim]Endpoints: /api/ingest, /api/compile, /api/ask, /api/search, /api/articles, /api/lint[/dim]")
    app.run(host="0.0.0.0", port=port, debug=False)


# ─── Web UI command ──────────────────────────────────────────────���─

@cli.command()
@click.option("--port", type=int, default=5555)
@click.pass_context
def web(ctx, port):
    """Start the full web UI (browsing, search, Q&A)."""
    from .web import create_web_app

    app = create_web_app(ctx.obj["base_dir"])
    console.print(f"[green]Web UI running at http://localhost:{port}[/green]")
    app.run(host="0.0.0.0", port=port, debug=False)


@cli.command()
@click.pass_context
def mcp(ctx):
    """Start the MCP (Model Context Protocol) server for AI client integration."""
    import asyncio
    from .mcp_server import main as mcp_main
    console.print("[green]Starting LLMBase MCP server (stdio)...[/green]")
    console.print("[dim]Register in your AI client settings to connect.[/dim]")
    asyncio.run(mcp_main())


@cli.group()
def export():
    """Export structured data for downstream projects."""
    pass


@export.command("article")
@click.argument("slug")
@click.pass_context
def export_article_cmd(ctx, slug):
    """Export a single article with full context."""
    from .export import export_article
    import json
    result = export_article(slug, ctx.obj["base_dir"])
    if not result:
        console.print(f"[red]Article not found: {slug}[/red]")
        return
    click.echo(json.dumps(result, indent=2, ensure_ascii=False))


@export.command("tag")
@click.argument("tag")
@click.pass_context
def export_tag_cmd(ctx, tag):
    """Export all articles with a given tag."""
    from .export import export_by_tag
    import json
    click.echo(json.dumps(export_by_tag(tag, ctx.obj["base_dir"]), indent=2, ensure_ascii=False))


@export.command("graph")
@click.argument("slug")
@click.option("--depth", type=int, default=2)
@click.pass_context
def export_graph_cmd(ctx, slug, depth):
    """Export article subgraph (N levels of connections)."""
    from .export import export_graph
    import json
    click.echo(json.dumps(export_graph(slug, depth, ctx.obj["base_dir"]), indent=2, ensure_ascii=False))


# ─── Operations contract (generic dispatch) ────────────────────────

@cli.group()
def ops():
    """Inspect and invoke operations via the unified contract (CLI/HTTP/MCP share)."""
    pass


@ops.command("list")
@click.pass_context
def ops_list_cmd(ctx):
    """List all registered operations."""
    from . import operations as _ops
    table = Table(title="Operations")
    table.add_column("name", style="cyan")
    table.add_column("category")
    table.add_column("writes", justify="center")
    table.add_column("description")
    for op in _ops.all_operations():
        table.add_row(op.name, op.category, "✓" if op.writes else "", op.description)
    console.print(table)


@ops.command("call")
@click.argument("name")
@click.option("--json-args", "json_args", default="{}", help="JSON-encoded arguments")
@click.pass_context
def ops_call_cmd(ctx, name, json_args):
    """Invoke an operation by name with JSON arguments.

    Example: llmbase ops call kb_search --json-args '{"query": "kong"}'
    """
    import json as _json
    from . import operations as _ops

    if _ops.get(name) is None:
        click.echo(f"Unknown operation: {name}", err=True)
        ctx.exit(1)
    try:
        args = _json.loads(json_args)
    except _json.JSONDecodeError as e:
        click.echo(f"--json-args is not valid JSON: {e}", err=True)
        ctx.exit(1)
    if not isinstance(args, dict):
        click.echo("--json-args must be a JSON object", err=True)
        ctx.exit(1)
    try:
        result = _ops.dispatch(name, ctx.obj["base_dir"], args)
    except TypeError as e:
        click.echo(f"Bad arguments: {e}", err=True)
        ctx.exit(2)
    except RuntimeError as e:
        click.echo(f"Busy: {e}", err=True)
        ctx.exit(3)
    click.echo(_json.dumps(result, ensure_ascii=False, indent=2, default=str))


def main():
    cli()

if __name__ == "__main__":
    main()
