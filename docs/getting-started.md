# Getting Started

## Install

```bash
git clone https://github.com/Hosuke/llmbase.git && cd llmbase
pip install -e .
cd frontend && npm install && npx vite build && cd ..
```

## Configure LLM

```bash
cp .env.example .env
```

Edit `.env` with any OpenAI-compatible API:

```
LLMBASE_API_KEY=sk-...
LLMBASE_BASE_URL=https://api.openai.com/v1
LLMBASE_MODEL=gpt-4o
LLMBASE_FALLBACK_MODELS=gpt-4o-mini,deepseek-chat   # optional; empty = no fallback
```

> Since 0.5.0, `LLMBASE_FALLBACK_MODELS` is *not* auto-generated. Leave it
> empty and only the primary model will be retried — recommended for
> aggregator deployments where your token may not have rights to other
> providers. Tune `LLMBASE_PRIMARY_RETRIES` (default 3) /
> `LLMBASE_FALLBACK_RETRIES` (default 1) if you need a different budget.

## First Run

```bash
llmbase web    # http://localhost:5555
```

## Build Your First Knowledge Base

```bash
# 1. Ingest some content
llmbase ingest url https://en.wikipedia.org/wiki/Stoicism
llmbase ingest pdf ./my-notes.pdf

# 2. Compile into wiki articles
llmbase compile new

# 3. Ask questions
llmbase query "What are the key ideas?"

# 4. Check health
llmbase lint check
```

## Web UI Pages

| Page | URL | Purpose |
|------|-----|---------|
| Dashboard | `/` | Guided reading (导读) + stats + quick actions |
| Wiki | `/wiki` | Browse all articles with sidebar categories |
| Search | `/search` | Full-text search |
| Q&A | `/qa` | Ask questions (deep research + tone modes) |
| Graph | `/graph` | Knowledge graph visualization |
| Explore | `/explore` | Timeline, people, map views (opt-in) |
| Trails | `/trails` | Research exploration paths |
| Ingest | `/ingest` | Upload files, enter URLs |
| Health | `/health` | Check + repair wiki health |

## Configuration

All settings in `config.yaml`:

```yaml
llm:
  max_tokens: 16384

paths:
  raw: "./raw"
  wiki: "./wiki"

worker:
  enabled: false          # Set true for autonomous learning
  learn_source: cbeta
  learn_interval_hours: 6

health:
  auto_fix_broken_links: true
  max_stubs_per_run: 10

entities:
  enabled: false          # Set true for timeline/people/map views
```
