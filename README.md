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

**Phase 4: Lint & Heal** — LLM health checks: find inconsistencies, broken links, orphan articles. Auto-generates stub articles for missing concepts, fixes metadata, rebuilds index. The worker runs this cycle every 24h.

## Key Features

| Feature | Description |
|---------|-------------|
| **Trilingual Output** | Every article compiled in English, 中文, and 日本語 with global language switcher |
| **Autonomous Learning** | Background worker continuously ingests, compiles, and self-heals from configured sources |
| **Self-Healing Wiki** | Periodic health checks auto-generate stubs for broken links, fix metadata, rebuild index |
| **Voice/Tone Modes** | Query in different styles: caveman 🦴, 文言文 📜, scholar 🎓, ELI5 👶, or default |
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
llmbase lint fix             # Auto-fix metadata + broken links
llmbase lint heal            # Full cycle: check → fix → recheck → report

# Query & search
llmbase query "What are the key concepts?"
llmbase query "Compare X and Y" --format marp --file-back
llmbase query "Explain emptiness" --tone caveman    # 🦴 Ugg. Empty good.
llmbase query "何为空性" --tone wenyan               # 📜 佛者，覺也...
llmbase query "What is karma?" --tone scholar        # 🎓 Academic style
llmbase query "What is nirvana?" --tone eli5         # 👶 Simple explanation
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
  learn_source: cbeta              # auto-ingest from CBETA Buddhist canon
  learn_interval_hours: 6          # every 6 hours
  learn_batch_size: 10             # 10 new texts per batch
  compile_interval_hours: 1        # compile new docs every hour
  health_check_interval_hours: 24  # self-heal every 24 hours

health:
  auto_fix_broken_links: true      # generate stubs for broken [[wiki-links]]
  max_stubs_per_run: 10            # cap LLM calls per health cycle
```

The worker runs alongside the web server — no separate process needed. Health checks auto-generate stub articles for broken links and persist reports to `wiki/_meta/health.json`.

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
result = kb.ask("What is X?", deep=True, tone="wenyan")
results = kb.search("keyword")
health = kb.health_report()
```

HTTP endpoints: `/api/articles`, `/api/ask`, `/api/search`, `/api/ingest`, `/api/compile`, `/api/upload`, `/api/wiki/export`, `/api/taxonomy`, `/api/tones`, `/api/lint/fix`, `/api/health`

## Design Philosophy

- **No vector DB** — Index files + LLM context window are sufficient at personal scale
- **Explorations add up** — Every query, every lint pass, every batch ingestion compounds the knowledge
- **LLM writes, you curate** — The LLM maintains the wiki; you direct what to learn
- **Incremental, not batch** — New data merges into existing articles, never starts from scratch
- **Trilingual by default** — Built for international scholarship

---

## 中文说明

### 这是什么？

LLMBase 是一个 **LLM 驱动的个人知识库系统**，灵感来自 [Karpathy 的 LLM Knowledge Base 设计](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)。

核心理念：**原始文档输入 → LLM 编译成三语结构化 wiki → 持续查询增强 → 知识不断叠加，温故而知新。**

不需要向量数据库，不需要 embedding pipeline。只需要 markdown 文件、一个 LLM、和一套干净的 Web UI。

### 架构设计

```
┌─ 数据摄入层 ────────────────────────────────────────────┐
│  URL 抓取 | PDF 自动转换 | 本地文件 | CBETA 大藏经       │
│  ctext.org 儒道经典 | 浏览器抓取 (OpenCLI)              │
└─────────────────────────┬───────────────────────────────┘
                          ↓
┌─ LLM 编译层 ────────────────────────────────────────────┐
│  提取概念 → 生成三语文章 (EN/中/日)                      │
│  建立 [[wiki 链接]] → 交叉引用 → 反向链接                │
│  重复概念自动合并 → 知识叠加而非覆盖                     │
└─────────────────────────┬───────────────────────────────┘
                          ↓
┌─ 知识库层 ──────────────────────────────────────────────┐
│  wiki/concepts/*.md    三语结构化文章                     │
│  wiki/_meta/index.json 全文索引                          │
│  wiki/_meta/taxonomy.json 自动生成的分类体系              │
│  wiki/_meta/backlinks.json 反向链接图谱                  │
│  wiki/outputs/*.md     Q&A 答案归档                      │
└─────────────────────────┬───────────────────────────────┘
                          ↓
┌─ 应用层 ────────────────────────────────────────────────┐
│  React Web UI (亮暗双主题，全局语言切换)                   │
│  CLI 命令行工具                                          │
│  Agent HTTP API + Python SDK                             │
│  D3.js 知识图谱可视化                                    │
│  自治 Worker（后台自动学习 + 编译 + 健康检查）             │
└─────────────────────────────────────────────────────────┘
```

### 四个阶段循环

1. **摄入 (Ingest)** — 从 URL、PDF、本地文件、或数据源插件（CBETA、ctext.org）收集原始文档
2. **编译 (Compile)** — LLM 阅读原始文档，提取概念，撰写三语文章，构建索引。已有概念自动合并更新
3. **查询与增强 (Query & Enhance)** — 基于 wiki 的智能问答，答案归档回 wiki。每次查询都让知识库更强
4. **检查与自愈 (Lint & Heal)** — 断链检测 → 自动生成 stub 文章、孤立文章发现、元数据补全、LLM 深度分析。Worker 每 24h 自动执行

### 核心功能

| 功能 | 说明 |
|------|------|
| **三语输出** | 每篇文章自动生成 English / 中文 / 日本語 三个版本，顶栏全局语言切换，支持中英双语模式 |
| **自治学习** | 后台 Worker 自动摄入、编译、自愈，部署后无需人工干预 |
| **自愈系统** | 定期健康检查，自动为断链生成 stub 文章，修复元数据，重建索引 |
| **语气模式** | 问答支持多种风格：原始人 🦴、文言文 📜、学术 🎓、幼儿园 👶 |
| **模型容错** | 主模型失败自动切换备选模型，知识库持续增长不中断 |
| **PDF 摄入** | `llmbase ingest pdf ./book.pdf` 自动切分为 20 页/块的 markdown，支持中英文 PDF |
| **知识叠加** | Q&A 答案归档回 wiki，Lint 建议新连接，重复概念合并而非覆盖。温故而知新 |
| **分类体系** | LLM 自动生成层级分类（参考四库全书分类法），左栏按分类浏览 |
| **Agent API** | HTTP API + Python SDK，便于 AI agent 直接查询、搜索、贡献知识 |
| **知识图谱** | D3.js 力导向图，可视化概念间的连接关系，发现意外关联 |

### 快速开始

```bash
# 克隆并安装
git clone https://github.com/Hosuke/llmbase.git && cd llmbase
pip install -e .
cd frontend && npm install && npx vite build && cd ..

# 配置 LLM（支持任何 OpenAI 兼容 API）
cp .env.example .env
# 编辑 .env 填入 API key、模型名、备选模型

# 启动
llmbase web    # 浏览器打开 http://localhost:5555
```

### CLI 命令速查

```bash
# 摄入
llmbase ingest url https://example.com/article   # 抓取网页
llmbase ingest pdf ./book.pdf --chunk-pages 20    # PDF 自动转换
llmbase ingest cbeta-learn --batch 10             # CBETA 大藏经渐进学习
llmbase ingest ctext-book 论语 /analects/zh       # ctext 经典抓取

# 编译与维护
llmbase compile new       # 增量编译新文档
llmbase lint check        # 结构健康检查
llmbase lint deep         # LLM 深度分析
llmbase lint heal         # 全自愈周期：检查 → 修复 → 复查 → 报告

# 查询（支持语气模式）
llmbase query "什么是般若？"                       # 默认风格
llmbase query "何为空性" --tone wenyan              # 📜 文言文风格
llmbase query "什么是因果" --tone caveman           # 🦴 原始人风格
llmbase query "比较儒道佛的核心思想" --file-back     # 答案归档回 wiki

# 部署
llmbase web               # Web UI (localhost:5555)
llmbase serve             # Agent API (localhost:5556)
```

### 自治 Worker 配置

```yaml
# config.yaml
worker:
  enabled: true
  learn_source: cbeta        # 数据源（CBETA 大藏经）
  learn_interval_hours: 6    # 每 6 小时自动学习一批
  learn_batch_size: 10       # 每批 10 部经文
  compile_interval_hours: 1        # 每小时自动编译
  health_check_interval_hours: 24  # 每 24 小时自愈检查

health:
  auto_fix_broken_links: true      # 自动为断链生成 stub 文章
  max_stubs_per_run: 10            # 每次自愈最多生成 10 篇 stub
```

部署后服务器会自己学、自己编译、自己建索引、自己修复断链。你只需要偶尔上传新 PDF 或调整学习方向。

### 数据源插件

| 插件 | 数据量 | 用法 |
|------|--------|------|
| **CBETA** | 4,868 部佛经，2.23 亿字 | `llmbase ingest cbeta-learn` |
| **ctext.org** | 儒道墨法兵等先秦经典 | `llmbase ingest ctext-book` |
| **PDF** | 任意 PDF 文件 | `llmbase ingest pdf` |

### 项目结构

```
llmbase/
├── frontend/              # React + TypeScript + Tailwind CSS
│   └── src/
│       ├── pages/         # Dashboard, Wiki, Search, Q&A, Graph, Ingest, Health
│       ├── components/    # Layout, Markdown, ArticleCard, Tag, Icon
│       └── lib/           # API 客户端, 主题管理, 语言管理, 品牌配置
├── tools/                 # Python 后端
│   ├── cli.py             # Click CLI 入口
│   ├── ingest.py          # 文档摄入（URL/文件/目录）
│   ├── compile.py         # LLM 三语编译 + 去重合并
│   ├── query.py           # Q&A 引擎
│   ├── search.py          # TF-IDF 全文搜索 + Web UI
│   ├── lint.py            # 健康检查 + 自动修复
│   ├── worker.py          # 自治学习后台 Worker
│   ├── cbeta.py           # CBETA 大藏经插件
│   ├── ctext.py           # ctext.org 经典插件
│   ├── pdf.py             # PDF → Markdown 转换
│   ├── taxonomy.py        # LLM 自动分类体系生成
│   ├── agent_api.py       # Agent HTTP API + Python SDK
│   ├── web.py             # Flask Web 服务器
│   └── llm.py             # LLM 客户端（多模型容错）
├── config.yaml            # 配置文件
├── .env.example           # LLM API 配置模板
├── Dockerfile             # Docker 部署
└── pyproject.toml
```

### 部署方式

| 方式 | 说明 |
|------|------|
| **Docker** | `docker compose up -d`，一行命令 |
| **Railway** | 连接 GitHub 仓库，自动部署，push 即更新 |
| **Render** | 免费 tier 可用 |
| **VPS** | `gunicorn wsgi:app`，任何服务器 |

---

## License

MIT

---

<div align="center">
<sub>Built with LLMs, for LLMs. Knowledge compounds. 温故而知新。</sub>
</div>
