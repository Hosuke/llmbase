---
name: llmwiki
version: "0.6.8"
description: "LLM-powered personal knowledge base. Raw documents in, an LLM compiles them into a structured interlinked wiki with trilingual articles, emergent taxonomy, and self-healing. One operations contract serves CLI, HTTP, and MCP."
author: Hosuke
homepage: https://github.com/Hosuke/llmbase
source: https://github.com/Hosuke/llmbase
license: MIT
keywords:
  - knowledge-base
  - wiki
  - llm
  - karpathy
  - research
  - mcp
  - personal-wiki
  - multilingual
  - self-healing
  - agent-tools
  - claude-code
  - openclaw
install: "pip install llmwiki"
requires:
  credentials:
    - name: LLMBASE_API_KEY
      description: "API key for any OpenAI-compatible LLM endpoint (user-supplied)"
      required: true
    - name: LLMBASE_BASE_URL
      description: "LLM API base URL"
      required: false
    - name: LLMBASE_MODEL
      description: "Primary model name"
      required: false
    - name: LLMBASE_FALLBACK_MODELS
      description: "Comma-separated fallback model chain (empty = no fallback)"
      required: false
  permissions:
    - network: "Fetches URLs during ingest (SSRF-protected), corpus plugins (CBETA/Wikisource/ctext), and scheduled fetches when the autonomous worker is enabled"
    - filesystem: "Reads/writes markdown under local raw/ and wiki/"
    - server: "Optional: web UI (:5555), agent API (:5556), MCP server (stdio)"
  notes: |
    Manages a local knowledge base. Network activity covers user-initiated
    ingest (URLs, PDFs, corpus plugins) plus the autonomous worker when
    explicitly enabled in config. Web server and worker are opt-in. No data
    is sent anywhere except the configured LLM API.
---

# llmwiki

A personal knowledge base that an LLM _compiles_, not just stores. Raw documents go in, an LLM writes trilingual (EN / 中文 / 日本語) wiki articles with `[[wiki-links]]`, backlinks, and an emergent taxonomy. The MCP server dispatches every tool through `tools/operations.py`; the CLI exposes the same registry via `llmbase ops call`; individual HTTP/CLI wrappers are being migrated onto the registry over time.

- **PyPI**: `pip install llmwiki`
- **CLI command**: `llmbase` (the package name and the command differ)
- **GitHub**: https://github.com/Hosuke/llmbase
- **Demo**: https://huazangge-production.up.railway.app

## Setup

```bash
pip install llmwiki

mkdir my-kb && cd my-kb

cat > .env << 'EOF'
LLMBASE_API_KEY=sk-your-key
LLMBASE_BASE_URL=https://your-endpoint/v1
LLMBASE_MODEL=your-model
# Optional: LLMBASE_FALLBACK_MODELS=backup-1,backup-2
EOF

cat > config.yaml << 'EOF'
llm:
  max_tokens: 16384
paths:
  raw: "./raw"
  wiki: "./wiki"
EOF
```

## Commands

| Command | Description |
|---------|-------------|
| `llmbase ingest url <url>` | Ingest a web article |
| `llmbase ingest pdf <file>` | Ingest a PDF (auto-chunks) |
| `llmbase ingest file <file>` | Ingest any local file |
| `llmbase ingest dir <dir>` | Ingest all files from a directory |
| `llmbase ingest cbeta-learn --batch 10` | Corpus plugin: Buddhist canon |
| `llmbase ingest ctext-book 论语 /analects/zh` | Corpus plugin: Chinese classics |
| `llmbase compile new` | Compile new raw docs incrementally (3-layer dedup) |
| `llmbase compile all` | Full rebuild |
| `llmbase compile index` | Rebuild index + aliases |
| `llmbase query "<q>"` | Ask a question (single-pass; add `--deep` for multi-step research) |
| `llmbase query "<q>" --tone wenyan` | 📜 classical Chinese voice |
| `llmbase query "<q>" --tone scholar` | 🎓 academic voice |
| `llmbase query "<q>" --tone eli5` | 👶 simple voice |
| `llmbase query "<q>" --tone caveman` | 🦴 primitive voice |
| `llmbase query "<q>" --file-back` | File answer back into the wiki |
| `llmbase lint check` | 8-category structural health check |
| `llmbase lint heal` | Check → fix → re-check → report |
| `llmbase lint deep` | LLM deep quality analysis |
| `llmbase web` | Web UI at :5555 |
| `llmbase serve` | Agent HTTP API at :5556 |
| `llmbase mcp` | Start MCP server (stdio) |
| `llmbase stats` | KB statistics |

## MCP Integration (for AI clients)

```json
{
  "mcpServers": {
    "llmwiki": {
      "command": "python",
      "args": ["-m", "tools.mcp_server", "--base-dir", "/path/to/my-kb"]
    }
  }
}
```

Tools exposed by the MCP server:

| Tool | Purpose |
|------|---------|
| `kb_search` | Full-text search over compiled concepts |
| `kb_search_raw` | Verbatim full-text fallback over raw/ sources (v0.6.2+) |
| `kb_ask` | Deep-research Q&A with tone modes |
| `kb_get` | Get article by slug or alias (`空`, `kong`, `emptiness` all work) |
| `kb_list` | List articles, filter by tag |
| `kb_backlinks` | Find articles citing a given article |
| `kb_taxonomy` | Multilingual category tree |
| `kb_stats` | Article count, word count |
| `kb_xici` | Guided reading (导读) |
| `kb_ingest` | Ingest a URL |
| `kb_compile` | Compile raw → wiki |
| `kb_lint` | Health check / auto-fix |
| `kb_export` / `kb_export_article` / `kb_export_tag` / `kb_export_graph` | Structured export for downstream projects |

All tools are declared in `tools/operations.py` — downstream projects register custom ops via `operations.register(...)` and they become available on CLI + MCP automatically.

Agents mounted on this server can answer from compiled concepts, fall back to raw sources with `kb_search_raw` when compile glossed a detail, ingest new material mid-session, and trigger healing.

## Workflows

### Build a KB from scratch

```
llmbase ingest url https://example.com/topic
llmbase ingest pdf ./paper.pdf
llmbase compile new
llmbase query "What are the key concepts?"
llmbase lint heal
```

### Autonomous mode (deploy once, server keeps learning)

```yaml
# config.yaml
worker:
  enabled: true
  learn_source: cbeta         # built-in: cbeta | wikisource | both; custom via register_learn_source()
  learn_interval_hours: 6
  compile_interval_hours: 1
  health_check_interval_hours: 24

health:
  auto_fix_broken_links: true
  max_stubs_per_run: 10
```

The worker starts under the production WSGI entrypoint (`wsgi.py` → `start_worker_thread`). Deploy with `gunicorn wsgi:app`; `llmbase web` alone does not self-start the worker.

### Daily use as agent memory

1. Agent receives a task → calls `kb_search` for relevant concepts
2. If the compiled answer is too abstract → calls `kb_search_raw` for verbatim detail
3. Learns something new → calls `kb_ingest` with the URL
4. Optionally `kb_compile` to fold it into concepts for next session
5. Periodically `kb_lint` heals the graph

## Key Concepts

- **Synthesis, not archiving** — LLM reads raw material and writes composed articles; storage is the cheap part
- **Two-layer recall** — `kb_search` (concepts) + `kb_search_raw` (verbatim raw sources)
- **Trilingual default** — every article has EN / 中文 / 日本語 sections
- **叠加进化** — new data merges into existing concepts, never overwrites
- **Domain-agnostic** — taxonomy emerges per-domain, nothing hardcoded
- **Self-healing** — 7-step auto-fix pipeline repairs drift
- **Alias resolution** — `[[参禅]]` → `can-chan.md` across scripts and simplified/traditional
- **Registry-backed ops** — MCP dispatches every tool through `operations.py`; CLI exposes the same registry via `llmbase ops list` / `llmbase ops call`; direct HTTP/CLI wrappers are being migrated onto the registry

## Tips

- `--file-back` saves Q&A answers into the wiki so future queries benefit
- `--tone wenyan` for Chinese users (classical Chinese responses)
- Run `llmbase lint heal` after large ingestion batches
- Web UI `/health` has buttons for every repair op
- Knowledge graph at `/graph` — density slider for large KBs
- Timeline at `/explore` — requires `entities: { enabled: true }` in config

## Security & Privacy

- **All data stays local** — wiki files are plain markdown on your filesystem
- **LLM API key** — user-supplied, loaded from `.env`
- **Network access** — user-initiated (URL ingest, SSRF-protected) plus corpus plugins (`cbeta-learn`, `wikisource-learn`, `ctext-book`) and the autonomous worker when enabled
- **Web server** — optional; binds `0.0.0.0` so LAN-accessible by default — front with a reverse proxy or bind override for public exposure
- **API secret** — cloud deployments (with `PORT` env) gate most mutating endpoints behind `LLMBASE_API_SECRET` (auto-generated if unset). Note: `/api/ask` is open by default and writes Q&A back via `file_back`; only promotion to concepts requires the secret
- **Autonomous worker** — opt-in via config, disabled by default
- **No telemetry** — nothing is sent anywhere except the configured LLM API
