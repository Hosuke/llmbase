<div align="center">

# LLMBase

**LLM-powered personal knowledge base**

Inspired by [Karpathy's approach](https://x.com/karpathy/status/2039805659525644595) — raw data goes in, an LLM compiles it into a structured, interlinked wiki, and you query & enhance it over time.

No vector database. No embeddings pipeline. Just markdown, an LLM, and a clean UI.

[English](#english) | [中文](#中文)

</div>

---

<table>
<tr>
<td><img src="docs/images/dashboard-light.png" alt="Dashboard Light" /></td>
<td><img src="docs/images/dashboard-dark.png" alt="Dashboard Dark" /></td>
</tr>
<tr>
<td align="center"><em>Dashboard — Light Theme</em></td>
<td align="center"><em>Dashboard — Dark Theme</em></td>
</tr>
</table>

---

<a id="english"></a>

## How It Works

```
raw/  ──LLM compile──>  wiki/  ──query/lint──>  wiki/ (enhanced)
 │                        │                        │
 ├─ web articles          ├─ concept articles       ├─ filed answers
 ├─ papers                ├─ index + backlinks      ├─ new connections
 └─ local files           └─ cross-references       └─ health fixes
```

**Phase 1: Ingest** — Collect documents from URLs, local files, or directories into `raw/`

**Phase 2: Compile** — LLM reads raw docs, extracts concepts, writes wiki articles with `[[wiki-links]]`, builds index

**Phase 3: Query** — Ask questions against the wiki. Answers rendered as markdown, Marp slides, or charts. Outputs filed back into the wiki.

**Phase 4: Lint** — LLM health checks: find inconsistencies, broken links, orphan articles, suggest new connections

## Screenshots

<details>
<summary><strong>Wiki Browser</strong> — Browse, filter, and explore articles</summary>
<table><tr>
<td><img src="docs/images/wiki-light.png" alt="Wiki Light" /></td>
<td><img src="docs/images/wiki-dark.png" alt="Wiki Dark" /></td>
</tr></table>
</details>

<details>
<summary><strong>Article Detail</strong> — Full markdown rendering with wiki-links, TOC, and backlinks</summary>
<table><tr>
<td><img src="docs/images/article-light.png" alt="Article Light" /></td>
<td><img src="docs/images/article-dark.png" alt="Article Dark" /></td>
</tr></table>
</details>

<details>
<summary><strong>Q&A</strong> — Ask complex questions, get cited answers, file them back</summary>
<table><tr>
<td><img src="docs/images/qa-light.png" alt="QA Light" /></td>
<td><img src="docs/images/qa-dark.png" alt="QA Dark" /></td>
</tr></table>
</details>

<details>
<summary><strong>Knowledge Graph</strong> — Interactive D3 visualization of concept connections</summary>
<table><tr>
<td><img src="docs/images/graph-light.png" alt="Graph Light" /></td>
<td><img src="docs/images/graph-dark.png" alt="Graph Dark" /></td>
</tr></table>
</details>

<details>
<summary><strong>Ingest & Health</strong> — Document management and wiki quality dashboard</summary>
<table><tr>
<td><img src="docs/images/health-light.png" alt="Health Light" /></td>
<td><img src="docs/images/health-dark.png" alt="Health Dark" /></td>
</tr></table>
</details>

## Quick Start

```bash
# Clone
git clone https://github.com/YOUR_USERNAME/llmbase.git
cd llmbase

# Backend
pip install -e .

# Frontend
cd frontend && npm install && npx vite build && cd ..

# Configure LLM provider (any OpenAI-compatible API)
cp .env.example .env
# Edit .env with your API key and model

# Launch
llmbase web        # Web UI at http://localhost:5555
```

## CLI Commands

```bash
# Ingest
llmbase ingest url https://example.com/article
llmbase ingest file ./paper.md
llmbase ingest dir ./research-papers/

# Compile
llmbase compile new          # Incremental — only new docs
llmbase compile all          # Full rebuild
llmbase compile index        # Rebuild index only

# Query
llmbase query "What are the key concepts?"
llmbase query "Compare X and Y" --format marp --file-back

# Search
llmbase search query "topic"
llmbase search serve         # Search web UI

# Lint
llmbase lint check           # Structural checks
llmbase lint deep            # LLM-powered deep analysis
llmbase lint fix             # Auto-fix metadata

# Serve
llmbase web                  # Full web UI (localhost:5555)
llmbase serve                # Agent HTTP API (localhost:5556)
llmbase stats                # Show stats
```

## LLM Provider

Works with **any OpenAI-compatible API**. Configure via `.env`:

```bash
# OpenAI
LLMBASE_API_KEY=sk-...
LLMBASE_BASE_URL=https://api.openai.com/v1
LLMBASE_MODEL=gpt-4o

# OpenRouter (200+ models)
LLMBASE_API_KEY=sk-or-...
LLMBASE_BASE_URL=https://openrouter.ai/api/v1
LLMBASE_MODEL=anthropic/claude-sonnet-4-6

# Ollama (local, free)
LLMBASE_API_KEY=ollama
LLMBASE_BASE_URL=http://localhost:11434/v1
LLMBASE_MODEL=llama3.1
```

Also supports `OPENAI_API_KEY` / `OPENAI_BASE_URL` as fallback.

## Agent API

Agents can interact with the knowledge base programmatically:

```python
from tools.agent_api import KnowledgeBase

kb = KnowledgeBase("./")
kb.ingest("https://example.com/article")
kb.compile()
result = kb.ask("What is X?")
results = kb.search("keyword")
```

Or via HTTP at `localhost:5556`:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/ingest` | POST | Add documents |
| `/api/compile` | POST | Compile raw → wiki |
| `/api/ask` | POST | Q&A |
| `/api/search` | GET | Full-text search |
| `/api/articles` | GET | List all articles |
| `/api/articles/:slug` | GET | Read article |
| `/api/lint` | POST | Health check |

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Backend** | Python, Flask, Click |
| **Frontend** | React 18, TypeScript, Tailwind CSS, Vite |
| **Markdown** | react-markdown, remark-gfm |
| **Graph** | D3.js force simulation |
| **LLM** | Any OpenAI-compatible API |
| **Browser** | OpenCLI integration (optional) |

## Design Philosophy

- **No vector DB needed** — At personal scale (~100-500 articles), index files + LLM context window are sufficient
- **Explorations add up** — Every query answer gets filed back into the wiki
- **LLM does the writing** — You rarely edit the wiki manually
- **Incremental compilation** — New raw data gets integrated, not reprocessed from scratch
- **Agent-first** — Built for LLM agents to use as a tool
- **Light & Dark themes** — Scholarly light mode and deep-focus dark mode

---

<a id="中文"></a>

## 中文说明

### 这是什么？

LLMBase 是一个 **LLM 驱动的个人知识库系统**，灵感来自 [Karpathy 的推文](https://x.com/karpathy/status/2039805659525644595)。

核心理念：原始文档输入 → LLM 编译成结构化 wiki → 持续查询和增强。不需要向量数据库，不需要 embedding pipeline，只需要 markdown 文件和一个 LLM。

### 四个阶段

1. **摄入 (Ingest)** — 从 URL、本地文件或目录收集文档到 `raw/`
2. **编译 (Compile)** — LLM 阅读原始文档，提取概念，撰写带有 `[[wiki链接]]` 的文章，构建索引
3. **查询 (Query)** — 基于 wiki 问答，答案可渲染为 markdown/幻灯片/图表，并归档回 wiki
4. **检查 (Lint)** — LLM 健康检查：发现不一致、断链、孤立文章，建议新连接

### 快速开始

```bash
# 克隆
git clone https://github.com/YOUR_USERNAME/llmbase.git
cd llmbase

# 安装后端
pip install -e .

# 构建前端
cd frontend && npm install && npx vite build && cd ..

# 配置 LLM（支持任何 OpenAI 兼容 API）
cp .env.example .env
# 编辑 .env 填入你的 API key 和模型名

# 启动
llmbase web        # Web 界面 http://localhost:5555
```

### 主要功能

| 功能 | 说明 |
|------|------|
| **文档摄入** | 支持 URL、本地文件、目录批量导入，可选 OpenCLI 浏览器抓取 |
| **LLM 编译** | 自动提取概念、生成文章、建立交叉引用和反向链接 |
| **智能问答** | 基于知识库的 Q&A，支持深度搜索模式，答案自动归档 |
| **全文搜索** | TF-IDF 搜索引擎，支持 Web UI 和 CLI |
| **知识图谱** | D3.js 力导向图，可视化概念间的连接关系 |
| **健康检查** | 断链检测、孤立文章、元数据缺失、LLM 深度分析 |
| **Agent API** | HTTP API + Python SDK，便于 AI agent 直接调用 |
| **双主题** | 学术风亮色模式 + 深色专注模式，一键切换 |

### 支持的 LLM

通过 `.env` 配置，支持任何 OpenAI 兼容 API：

- OpenAI (GPT-4o, GPT-4 等)
- Anthropic Claude (通过 OpenRouter)
- Ollama (本地运行，免费)
- OpenRouter (200+ 模型)
- 以及任何 OpenAI 兼容接口

### 项目结构

```
llmbase/
├── frontend/             # React + TypeScript + Tailwind
│   └── src/
│       ├── pages/        # 7 个页面组件
│       ├── components/   # 共享组件 (Markdown, Layout, etc.)
│       └── lib/          # API 客户端, 主题管理
├── tools/                # Python 后端
│   ├── cli.py            # CLI 入口
│   ├── ingest.py         # 文档摄入
│   ├── compile.py        # LLM 编译
│   ├── query.py          # Q&A 引擎
│   ├── search.py         # 搜索引擎
│   ├── lint.py           # 健康检查
│   ├── agent_api.py      # Agent HTTP API
│   ├── web.py            # Web 服务器
│   └── llm.py            # LLM 客户端
├── config.yaml
├── .env.example
└── pyproject.toml
```

---

## License

MIT

---

<div align="center">
<sub>Built with LLMs, for LLMs.</sub>
</div>
