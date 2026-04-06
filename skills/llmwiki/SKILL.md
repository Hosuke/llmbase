---
name: llmwiki
version: "0.1.0"
description: "LLM-powered personal knowledge base. Ingest raw documents, compile into a structured interlinked wiki, query with deep research, self-heal. Works for any domain."
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
  - rag
  - multilingual
  - self-healing
  - agent-tools
  - claude-code
  - openclaw
---

# llmwiki

Build an LLM-powered personal knowledge base. Raw documents go in, an LLM compiles them into a structured, interlinked wiki, and you query & enhance it over time. Every exploration adds up.

**PyPI**: `pip install llmwiki`
**GitHub**: https://github.com/Hosuke/llmbase
**Demo**: https://huazangge-production.up.railway.app

## Setup

```bash
pip install llmwiki

# Create a new knowledge base
mkdir my-kb && cd my-kb

# Configure LLM (any OpenAI-compatible API)
cat > .env << 'EOF'
LLMBASE_API_KEY=sk-your-key
LLMBASE_BASE_URL=https://api.openai.com/v1
LLMBASE_MODEL=gpt-4o
EOF

# Initialize config
cat > config.yaml << 'EOF'
llm:
  max_tokens: 8192
paths:
  raw: "./raw"
  wiki: "./wiki"
EOF
```

## Commands

| Command | Description |
|---------|-------------|
| `llmwiki ingest url <url>` | Ingest a web article |
| `llmwiki ingest pdf <file>` | Ingest a PDF (auto-chunks) |
| `llmwiki ingest file <file>` | Ingest any local file |
| `llmwiki ingest dir <dir>` | Ingest all files from a directory |
| `llmwiki compile new` | Compile new raw docs into wiki articles |
| `llmwiki compile index` | Rebuild index + aliases |
| `llmwiki query "<question>"` | Ask a question (deep research) |
| `llmwiki query "<q>" --tone wenyan` | Ask in classical Chinese style |
| `llmwiki query "<q>" --tone scholar` | Ask in academic style |
| `llmwiki search query "<term>"` | Full-text search |
| `llmwiki lint check` | Health check (8 categories) |
| `llmwiki lint heal` | Full self-heal: check → fix → recheck |
| `llmwiki lint clean` | Remove garbage articles |
| `llmwiki lint dedup` | Detect and merge duplicates |
| `llmwiki lint normalize-tags` | Merge synonymous tags |
| `llmwiki web` | Start web UI at localhost:5555 |
| `llmwiki serve` | Start agent HTTP API at localhost:5556 |
| `llmwiki mcp` | Start MCP server (stdio) |
| `llmwiki stats` | Show KB statistics |

## Workflows

### Build a Knowledge Base from Scratch

```
1. llmwiki ingest url https://example.com/topic-overview
2. llmwiki ingest pdf ./my-research-paper.pdf
3. llmwiki compile new
4. llmwiki query "What are the key concepts?"
5. llmwiki lint heal
```

### Daily Learning Routine

```
1. Find a new article → llmwiki ingest url <url>
2. llmwiki compile new
3. llmwiki query "How does this relate to what I already know?"
4. Knowledge compounds with each cycle
```

### Autonomous Mode (set and forget)

```yaml
# config.yaml
worker:
  enabled: true
  learn_source: wikisource  # or: cbeta, both
  learn_interval_hours: 6
  compile_interval_hours: 1
  health_check_interval_hours: 24
```

Then `llmwiki web` — the server learns, compiles, and self-heals on its own.

### Health Maintenance

```
llmwiki lint check        # See issues
llmwiki lint heal         # Auto-fix everything
```

The auto-fix pipeline: clean garbage → fix dirty tags → normalize tags → fix metadata → fix broken links → merge duplicates → regenerate taxonomy.

## MCP Integration

Register as an MCP server so any AI client can use the knowledge base directly:

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

Available MCP tools: `kb_search`, `kb_ask`, `kb_get`, `kb_list`, `kb_backlinks`, `kb_taxonomy`, `kb_stats`, `kb_xici`, `kb_ingest`, `kb_compile`, `kb_lint`.

## Key Concepts

- **Raw → Wiki → Query**: three-layer architecture (Karpathy pattern)
- **Trilingual**: articles compiled in English, 中文, 日本語
- **叠加进化**: knowledge merges into existing articles, never starts from scratch
- **Domain-agnostic**: no hardcoded domains — works for any field
- **Self-healing**: auto-detects and repairs broken links, duplicates, dirty tags
- **Alias resolution**: `[[参禅]]` → `can-chan.md` across languages and scripts
- **Agent-native**: every feature accessible via CLI, HTTP API, and MCP

## Tips

- Use `--file-back` to save Q&A answers back into the wiki
- Use `--tone wenyan` for Chinese users (classical Chinese responses)
- Run `llmwiki lint heal` after large ingestion batches
- The web UI at `/health` has buttons for all repair operations
- Knowledge graph at `/graph` — use the density slider for large KBs
- Timeline at `/explore` — requires `entities: { enabled: true }` in config
