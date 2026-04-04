<div align="center">

# LLMBase

**LLM-powered personal knowledge base**

Inspired by [Karpathy's LLM Knowledge Base pattern](https://x.com/karpathy/status/2039805659525644595) ([detailed design](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)) — raw data goes in, an LLM compiles it into a structured, interlinked wiki, and you query & enhance it over time.

No vector database. No embeddings pipeline. Just markdown, an LLM, and a clean UI.

**[Live Demo](https://huazangge-production.up.railway.app)** — 華藏閣, an autonomous knowledge base that continuously learns Chinese and Buddhist classics

[English](#how-it-works) | [中文](#中文说明)

</div>

---

## How It Works

```
raw/  ──LLM compile──>  wiki/  ──query/lint──>  wiki/ (enhanced)
 │                        │                        │
 ├─ web articles          ├─ concept articles       ├─ filed answers
 ├─ papers / PDFs         ├─ index + backlinks      ├─ new connections
 └─ local files           └─ cross-references       └─ health fixes
                              ↑                        │
                              └────────────────────────┘
                                explorations add up
```

**Phase 1: Ingest** — Collect documents from URLs, PDFs, local files, or data sources (CBETA, ctext.org) into `raw/`

**Phase 2: Compile** — LLM reads raw docs, extracts concepts, writes trilingual wiki articles (EN/中/日) with `[[wiki-links]]`, builds index. Duplicate concepts are merged, not recreated.

**Phase 3: Query & Enhance** — Ask questions against the wiki. Answers get filed back, strengthening the knowledge base. Every exploration adds up.

**Phase 4: Lint** — LLM health checks: find inconsistencies, broken links, orphan articles, suggest new connections. Auto-fix metadata.

## Key Features

| Feature | Description |
|---------|-------------|
| **Trilingual Output** | Every article compiled in English, 中文, and 日本語 with global language switcher |
| **Autonomous Learning** | Background worker continuously ingests and compiles from configured sources |
| **Model Fallback** | Primary LLM fails? Auto-falls back to secondary models. Knowledge base keeps growing. |
| **PDF Ingestion** | `llmbase ingest pdf ./book.pdf` — auto-chunks and converts to markdown |
| **Explorations Add Up** | Q&A answers file back into the wiki. Lint passes suggest new articles. Knowledge compounds. |
| **Agent-First API** | HTTP API + Python SDK for LLM agents to query and contribute to the knowledge base |
| **Knowledge Graph** | D3.js force-directed visualization of concept connections |
| **Deploy Anywhere** | Docker, Railway, Render, or any VPS. One-command cloud deploy. |

## Quick Start

```bash
git clone https://github.com/Hosuke/llmbase.git && cd llmbase

# Backend
pip install -e .

# Frontend
cd frontend && npm install && npx vite build && cd ..

# Configure (any OpenAI-compatible API)
cp .env.example .env    # edit with your API key

# Launch
llmbase web              # http://localhost:5555
```

## Use Cases

LLMBase is designed for anyone building a personal or domain-specific knowledge base:

- **Researchers** — Compile papers and notes into an interlinked wiki that grows with every reading
- **Students** — Build a study knowledge base that deepens with each review session
- **Domain experts** — Create specialized reference wikis (law, medicine, history, philosophy)
- **Cultural preservation** — Digitize and compile classical texts with multilingual annotations
- **AI developers** — Build structured knowledge for agent retrieval without vector databases

## CLI Reference

```bash
# Ingest from various sources
llmbase ingest url https://example.com/article
llmbase ingest pdf ./book.pdf --chunk-pages 20
llmbase ingest file ./notes.md
llmbase ingest dir ./research-papers/

# Data source plugins
llmbase ingest cbeta-learn --batch 10         # Buddhist canon (CBETA)
llmbase ingest cbeta-work T0235               # Specific sutra (Heart Sutra)
llmbase ingest ctext-book 论语 /analects/zh   # Chinese classics (ctext.org)

# Compile & maintain
llmbase compile new          # Incremental compilation
llmbase compile all          # Full rebuild
llmbase compile index        # Rebuild index only
llmbase lint check           # Structural health check
llmbase lint deep            # LLM-powered deep analysis

# Query & search
llmbase query "What are the key concepts?"
llmbase query "Compare X and Y" --format marp --file-back
llmbase search query "topic"

# Serve
llmbase web                  # Full web UI (localhost:5555)
llmbase serve                # Agent HTTP API (localhost:5556)
```

## LLM Provider

Works with **any OpenAI-compatible API**:

```bash
LLMBASE_API_KEY=sk-...
LLMBASE_BASE_URL=https://api.openai.com/v1
LLMBASE_MODEL=gpt-4o

# Auto-fallback when primary model fails
LLMBASE_FALLBACK_MODELS=gpt-4o-mini,deepseek-chat
```

Supports: OpenAI, OpenRouter (200+ models), Ollama (local/free), Together, Groq, and any compatible endpoint.

## Autonomous Worker

Deploy once, and the server learns on its own:

```yaml
# config.yaml
worker:
  enabled: true
  learn_source: cbeta          # auto-ingest from CBETA Buddhist canon
  learn_interval_hours: 6      # every 6 hours
  learn_batch_size: 10         # 10 new texts per batch
  compile_interval_hours: 1    # compile new docs every hour
```

The worker runs alongside the web server — no separate process needed.

## Deployment

```bash
# Docker
docker compose up -d

# Railway (connects to GitHub, auto-deploys on push)
railway init && railway up

# Manual
gunicorn --bind 0.0.0.0:5555 --workers 2 --timeout 300 wsgi:app
```

## Agent API

```python
from tools.agent_api import KnowledgeBase

kb = KnowledgeBase("./")
kb.ingest("https://example.com/article")
kb.compile()
result = kb.ask("What is X?", deep=True)
results = kb.search("keyword")
```

HTTP endpoints: `/api/articles`, `/api/ask`, `/api/search`, `/api/ingest`, `/api/compile`, `/api/upload`, `/api/wiki/export`, `/api/taxonomy`

## Design Philosophy

- **No vector DB** — Index files + LLM context window are sufficient at personal scale
- **Explorations add up** — Every query, every lint pass, every batch ingestion compounds the knowledge
- **LLM writes, you curate** — The LLM maintains the wiki; you direct what to learn
- **Incremental, not batch** — New data merges into existing articles, never starts from scratch
- **Trilingual by default** — Built for international scholarship

---

## 中文说明

LLMBase 是一个 **LLM 驱动的个人知识库系统**，灵感来自 [Karpathy 的设计](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)。

原始文档输入 → LLM 编译成三语结构化 wiki → 持续查询增强，知识持续叠加。

### 核心功能

| 功能 | 说明 |
|------|------|
| **三语输出** | 每篇文章自动生成 English / 中文 / 日本語，全局语言切换 |
| **自治学习** | 后台 Worker 自动从 CBETA 等数据源持续摄入和编译 |
| **模型容错** | 主模型失败自动切换备选模型，知识库持续增长不中断 |
| **PDF 摄入** | `llmbase ingest pdf` 自动切分 PDF 为 markdown |
| **知识叠加** | Q&A 答案归档回 wiki，Lint 建议新连接，温故而知新 |
| **Agent API** | HTTP + Python SDK，便于 AI agent 直接调用 |

### 快速开始

```bash
git clone https://github.com/Hosuke/llmbase.git && cd llmbase
pip install -e .
cd frontend && npm install && npx vite build && cd ..
cp .env.example .env  # 配置 API key
llmbase web           # 启动 http://localhost:5555
```

### 数据源插件

- **CBETA** — 大藏经渐进式学习 (`llmbase ingest cbeta-learn`)
- **ctext.org** — 儒道经典抓取 (`llmbase ingest ctext-book`)
- **PDF** — 任意 PDF 自动转换 (`llmbase ingest pdf`)

---

## License

MIT

---

<div align="center">
<sub>Built with LLMs, for LLMs. Knowledge compounds.</sub>
</div>
