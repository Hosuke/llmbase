<div align="center">

# LLMBase

**A personal knowledge base that an LLM _compiles_, not just stores.**

[![GitHub stars](https://img.shields.io/github/stars/Hosuke/llmbase?style=social)](https://github.com/Hosuke/llmbase)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)
[![PyPI](https://img.shields.io/pypi/v/llmwiki.svg)](https://pypi.org/project/llmwiki/)
[![Python 3.11+](https://img.shields.io/badge/Python-3.11+-blue.svg)](https://www.python.org)
[![MCP Compatible](https://img.shields.io/badge/MCP-Compatible-purple.svg)](https://modelcontextprotocol.io/)
[![ClawHub Skill](https://img.shields.io/badge/ClawHub-llmwiki-orange.svg)](https://clawhub.ai)
[![Deploy on Railway](https://img.shields.io/badge/Deploy-Railway-blueviolet.svg)](https://railway.app)

Inspired by [Karpathy's LLM Knowledge Base pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) — raw material in, an LLM compiles it into a structured, interlinked wiki, and you keep querying & refining it. No vector DB. No embeddings pipeline. Just markdown, an LLM, and one operations contract that serves your CLI, your browser, and your AI agent.

**Live:**
**[華藏閣](https://huazangge-production.up.railway.app)** — autonomous trilingual Buddhist KB, continuously learning from CBETA
· **[斯文](https://siwen.ink)** — classical-Chinese (文言) KB of Confucian, Daoist & Buddhist classics

[English](#the-idea) · [中文说明](#中文说明)

</div>

---

## The Idea

Most "memory systems" store every word and hope semantic search finds it later. **LLMBase does the opposite** — an LLM *reads and restructures* your raw material into composed markdown articles, with `[[wiki-links]]`, backlinks, and a taxonomy that emerges from the content itself. Storage is cheap; structure is the product.

Everything compounds. Every query, every lint pass, every ingest batch adds to the same graph: duplicate concepts merge (叠加进化), broken links auto-generate stubs, tone modes explain the same idea in several registers, citations resolve back to their sources. Humans and agents read the same wiki.

One registry — `tools/operations.py` — declares every KB operation exactly once. The MCP server dispatches every tool through it; the CLI and HTTP API share the same `Operation` definitions and are being migrated onto it. Register an op via `operations.register(...)` and it's reachable from an MCP-enabled agent immediately.

## Pipeline

```
raw/ ──compile──>  wiki/concepts/  ──query/lint──>  wiki/ (enhanced)
                         │                               ↑
                         └─────── 叠加进化 ───────────────┘
```

**1 · Ingest** — URLs, PDFs, local files, or corpus plugins (CBETA canon, ctext.org, Wikisource) land in `raw/` with provenance metadata.

**2 · Compile** — the LLM extracts concepts and writes trilingual articles (EN / 中文 / 日本語) with cross-references, aliases, and an emergent taxonomy. Existing concepts update in place.

**3 · Query** — a deep-research loop pulls context from compiled concepts, answers in the voice you choose (scholar 🎓 · 文言 📜 · ELI5 👶 · caveman 🦴), and optionally files the answer back. Agents that need verbatim material can call `kb_search_raw` directly for a second-layer recall over the original sources.

**4 · Heal** — the lint pipeline detects broken links, garbage stubs, dirty tags, duplicates, uncategorized drift — and repairs them. The background worker runs this on a schedule.

## What Makes It Different

- **Synthesis, not archiving.** The wiki *is* the memory. No vector store, no giant transcript tape.
- **Two-layer recall.** `kb_search` scores compiled concepts with a TF-IDF tokenizer; `kb_search_raw` runs the same scoring over the original `raw/` sources — verbatim fallback when the compile glossed over a detail.
- **Trilingual by default.** Every article ships with English, 中文, and 日本語 sections. A multilingual alias map resolves `[[参禅]]` → `can-chan.md`, with simplified/traditional conversion via opencc.
- **Emergent structure.** Taxonomy is LLM-generated per-domain — nothing hardcoded to Buddhism or classics. Works for any field.
- **One contract, three surfaces.** The same `Operation(name=..., handler=..., params=...)` powers CLI / HTTP / MCP simultaneously.
- **Self-healing.** 7-step auto-fix: clean → metadata → broken-links → dedup → taxonomy. Merges `benevolence` + `ren` + `仁爱` into one article rather than three.
- **Library, not framework.** Downstream projects override module-level constants and register hooks. No forking. The customization contract is stable across versions.

## Install

```bash
pip install llmwiki                                    # PyPI
# or
git clone https://github.com/Hosuke/llmbase.git
cd llmbase && pip install -e .
```

## Quick Start

```bash
cd frontend && npm install && npx vite build && cd ..
cp .env.example .env                     # set API key / base URL / model
llmbase web                              # → http://localhost:5555
```

Any OpenAI-compatible endpoint works (self-hosted or aggregator). Configure a fallback chain and retry budget:

```bash
LLMBASE_API_KEY=...
LLMBASE_BASE_URL=...
LLMBASE_MODEL=...

LLMBASE_FALLBACK_MODELS=model-a,model-b   # optional; empty = no fallback
LLMBASE_PRIMARY_RETRIES=3                 # default 3
LLMBASE_FALLBACK_RETRIES=1                # default 1
```

`.env` is discovered in this order (first hit wins; shell `export` always
beats the file):

1. `$LLMBASE_ENV_FILE` — explicit path override
2. `$PWD/.env` — only when `$PWD/config.yaml` declares llmbase paths
   (`paths.concepts` / `paths.wiki` / `paths.raw`). This keeps a stray
   `config.yaml` from an unrelated project from qualifying CWD as a KB
   root and loading a hostile `.env` that could redirect requests.
3. `~/.config/llmbase/.env` — user-level default, works across projects
4. `<package_parent>/.env` — legacy source-install path

Works identically for `pip install llmwiki`, `pipx install llmwiki`, and
`pip install -e .`.

## Three Surfaces, One Contract

Every operation is declared once in `tools/operations.py` and exposed everywhere.

### CLI

```bash
# ─── Ingest ─────────────────────────────────────
llmbase ingest url   https://example.com/article
llmbase ingest pdf   ./book.pdf --chunk-pages 20
llmbase ingest file  ./notes.md
llmbase ingest dir   ./research-papers/

llmbase ingest cbeta-learn      --batch 10         # Buddhist canon
llmbase ingest ctext-book 论语  /analects/zh       # Chinese classics
llmbase ingest wikisource-learn --batch 5

# ─── Compile & maintain ─────────────────────────
llmbase compile new             # incremental (3-layer dedup)
llmbase compile all             # full rebuild
llmbase compile index           # rebuild index + aliases

llmbase lint check              # 8 categories of structural checks
llmbase lint heal               # check → fix → re-check → report
llmbase lint deep               # LLM deep quality analysis

# ─── Query ──────────────────────────────────────
llmbase query "何为空性" --tone wenyan            # 📜 Classical Chinese
llmbase query "Explain X"  --tone scholar         # 🎓 Academic
llmbase query "What is Y"  --tone eli5            # 👶 Simple
llmbase query "Compare A and B" --file-back       # save to wiki

# ─── Serve ──────────────────────────────────────
llmbase web                     # Web UI     :5555
llmbase serve                   # Agent API  :5556
```

### HTTP

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET  | `/api/articles`          | list articles |
| GET  | `/api/articles/<slug>`   | article + backlinks + citations |
| GET  | `/api/search?q=...`      | full-text search over concepts |
| POST | `/api/ask`               | deep-research Q&A |
| GET  | `/api/taxonomy?lang=zh`  | hierarchical categories |
| GET  | `/api/xici?lang=zh`      | guided reading (导读) |
| GET  | `/api/entities`          | people / events / places |
| GET  | `/api/trails`            | research exploration paths |
| POST | `/api/lint/fix`          | auto-fix pipeline |

Read endpoints (articles, search, taxonomy, xici, entities) stay open. Mutating endpoints (ingest, compile, delete, clean, lint/fix) auto-protect behind `LLMBASE_API_SECRET` in cloud deployments — auto-generated if unset; same-origin frontend cookies set automatically. `kb_search_raw` is reachable via CLI and MCP; a REST route can be added via the customization contract.

### MCP (AI agents)

```json
{
  "mcpServers": {
    "llmbase": {
      "command": "python",
      "args": ["-m", "tools.mcp_server", "--base-dir", "/path/to/kb"]
    }
  }
}
```

Tools: `kb_search`, `kb_search_raw`, `kb_ask`, `kb_get`, `kb_list`, `kb_backlinks`, `kb_taxonomy`, `kb_xici`, `kb_stats`, `kb_ingest`, `kb_compile`, `kb_lint`, `kb_export*` — plus anything you register.

Works with Claude Code, Cursor, Windsurf, ClawHub, and any MCP client. An agent mounted on this server can answer from compiled concepts, fall back to raw sources when the compile missed detail, ingest new material mid-session, and trigger healing. See [docs/mcp-server.md](docs/mcp-server.md).

## Autonomous Worker

Deploy once; the server keeps learning.

```yaml
# config.yaml
worker:
  enabled: true
  learn_source: cbeta              # or any registered learn source
  learn_interval_hours: 6
  learn_batch_size: 10
  compile_interval_hours: 1
  health_check_interval_hours: 24

health:
  auto_fix_broken_links: true
  max_stubs_per_run: 10
```

The worker starts automatically under the production WSGI entrypoint (`wsgi.py` → `start_worker_thread`) — single deployment, no separate queue or cron. Local dev with `llmbase web` does not start the worker; run `gunicorn wsgi:app` (or call `tools.worker.start_worker_thread` yourself) to exercise the autonomous loop locally.

## Customization

Library, not framework. Override module-level constants at import time, or register callbacks on lifecycle events.

```python
import tools.compile as c
import tools.query   as q
from tools.hooks     import register
from tools.worker    import register_learn_source
from tools.operations import register as register_op, Operation

# Single-language KB
c.SECTION_HEADERS = [("wenyan", "## 文言")]

# Custom tone mode
q.TONE_INSTRUCTIONS["formal_zh"] = "請以正式中文回答。"

# React to lifecycle events (10 emitted across compile/lint/xici/entities/…)
register("compiled", lambda source, title, **kw: sync.push(source, title))

# Custom learn source for the worker
register_learn_source("my_corpus", my_learn_handler)

# One op, three surfaces
register_op(Operation(
    name="kb_custom",
    description="My custom KB op",
    handler=my_handler,
    params={"type": "object", "properties": {"query": {"type": "string"}}},
))
```

Stable contract — see [docs/customization.md](docs/customization.md) for the full surface (compile, query, taxonomy, xici, entities, lint, web, worker, operations).

## Live Deployments

Both run pure `llmwiki` as a dependency; all customization goes through hooks and overrides, no forks.

- **[華藏閣](https://huazangge-production.up.railway.app)** — autonomous Buddhist KB, continuously learning from CBETA canon, trilingual (EN / 中 / 日). Supabase sync wired via lifecycle hooks.
- **[斯文](https://siwen.ink)** — classical-Chinese KB of Confucian, Daoist & Buddhist classics. Single-language 文言 frontend. CJK slugs enabled via the customization contract.

## Design Principles

- **Domain-agnostic** — taxonomy emerges from content; nothing is hardcoded
- **No vector DB** — markdown + a thoughtful tokenizer + LLM context is enough at personal scale; `kb_search_raw` covers verbatim recall
- **Explorations add up** — every query, lint, and ingest compounds the wiki
- **LLM writes, you curate** — the LLM maintains; you direct
- **Incremental, not batch** — 叠加进化: concepts merge, never overwrite
- **Extensible without forking** — stable customization contract
- **Agent-native** — humans and agents are equal users
- **Self-healing** — the system detects and fixes its own drift

## Deploy

```bash
docker compose up -d                                                    # Docker
railway up                                                              # Railway
gunicorn --bind 0.0.0.0:5555 --workers 2 --timeout 300 wsgi:app         # any VPS
```

---

## 中文说明

### 这是什么

LLMBase（PyPI：`llmwiki`）是一个 **由 LLM 合成、而非单纯存储的知识库**。大多数"记忆系统"把每一个字塞进向量库、靠语义检索找回；LLMBase 恰恰相反——让 LLM **读懂并重写**你的原始材料为结构化、互链的 markdown 文章，带 `[[wiki 链接]]`、反向链接、以及由内容涌现的分类体系。**存储是廉价的，结构才是产品。**

一切会累加。每次查询、每轮 lint、每批摄入都落进同一张图：重复概念合并（叠加进化）、断链自动补 stub、多种语气模式换不同方式讲同一件事、引用可溯源回原文。人和 agent 读的是同一套 wiki。

单一注册表 `tools/operations.py` 把每个操作声明一次：MCP server 全部工具走该注册表派发；CLI 和 HTTP API 共用同一套 `Operation` 定义，正在逐步迁移接入。通过 `operations.register(...)` 注册一个 op，MCP-enabled agent 即可调用。

灵感来自 [Karpathy 的 LLM Knowledge Base 设计](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)。

### 管道

```
raw/ ──compile──>  wiki/concepts/  ──query/lint──>  wiki/（增强）
                         │                                ↑
                         └───────  叠加进化  ──────────────┘
```

1. **摄入** — URL、PDF、本地文件、语料插件（CBETA 大藏经、ctext、Wikisource）落地 `raw/`，带出处元数据
2. **编译** — LLM 提取概念，生成三语文章（EN/中/日），建立交叉引用、别名图与涌现式分类
3. **查询** — 深度研究式问答从编译后的概念层召回；agent 需要原文时可直接调用 `kb_search_raw` 走原文兜底。支持学者 🎓、文言 📜、幼稚园 👶、原始人 🦴 等语气；答案可归档回 wiki
4. **自愈** — lint pipeline 检测断链、垃圾 stub、脏 tag、重复、未分类，并修复。Worker 按时自动执行

### 与其他方案的区别

- **合成 vs 存档**——wiki 本身即记忆，不用向量库，不堆原文磁带
- **双层召回**——`kb_search` 对编译后概念做 TF-IDF 打分；`kb_search_raw` 用同一套打分器跑 `raw/` 原文，兜底取被 compile 抽象掉的细节
- **默认三语**——每篇都有 English / 中文 / 日本語；alias 图跨脚本解析 `[[参禅]]` → `can-chan.md`，繁简互通
- **结构涌现**——分类由 LLM 按域生成，零硬编码
- **一套契约，三个面**——同一个 `Operation(...)` 同时驱动 CLI / HTTP / MCP
- **自愈**——7 步 auto-fix：清理 → 元数据 → 断链 → 合并 → 分类
- **库而非框架**——下游通过覆盖模块常数和注册 hook 定制，不 fork，契约跨版本稳定

### 安装

```bash
pip install llmwiki
# 或
git clone https://github.com/Hosuke/llmbase.git
cd llmbase && pip install -e .
```

### 快速开始

```bash
cd frontend && npm install && npx vite build && cd ..
cp .env.example .env                     # 配置 API key / base URL / 模型
llmbase web                              # → http://localhost:5555
```

兼容任何 OpenAI 协议端点（自部署或聚合器）。可配置备选模型链与重试预算：

```bash
LLMBASE_API_KEY=...
LLMBASE_BASE_URL=...
LLMBASE_MODEL=...

LLMBASE_FALLBACK_MODELS=model-a,model-b   # 可选；空 = 不降级
LLMBASE_PRIMARY_RETRIES=3                 # 默认 3
LLMBASE_FALLBACK_RETRIES=1                # 默认 1
```

### 三个面，一套契约

#### CLI

```bash
# 摄入
llmbase ingest url   https://...
llmbase ingest pdf   ./book.pdf --chunk-pages 20
llmbase ingest cbeta-learn      --batch 10
llmbase ingest ctext-book 论语  /analects/zh

# 编译与维护
llmbase compile new           # 增量编译
llmbase lint heal             # 检查 → 修复 → 复查 → 报告
llmbase lint deep             # LLM 深度质量分析

# 查询
llmbase query "何为空性" --tone wenyan
llmbase query "比较儒道佛"   --file-back

# 服务
llmbase web                   # Web UI    :5555
llmbase serve                 # Agent API :5556
```

#### HTTP

读接口（articles / search / taxonomy / xici / entities）常开；变更类接口（ingest / compile / delete / clean / lint/fix）由 `LLMBASE_API_SECRET` 保护（云端部署自动生成；同源前端自动种 cookie）。`kb_search_raw` 目前通过 CLI 与 MCP 暴露，如需 REST 路由可通过定制契约添加。

#### MCP

```json
{
  "mcpServers": {
    "llmbase": {
      "command": "python",
      "args": ["-m", "tools.mcp_server", "--base-dir", "/path/to/kb"]
    }
  }
}
```

工具集：`kb_search`、`kb_search_raw`（原文兜底）、`kb_ask`、`kb_get`、`kb_list`、`kb_backlinks`、`kb_taxonomy`、`kb_xici`、`kb_stats`、`kb_ingest`、`kb_compile`、`kb_lint`、`kb_export*`，以及下游自注册的 op。

支持 Claude Code、Cursor、Windsurf、ClawHub 等所有 MCP 客户端。挂载后 agent 可从概念层答题、原文层兜底、会话中继续摄入、触发自愈。

### 自治 Worker

```yaml
worker:
  enabled: true
  learn_source: cbeta
  learn_interval_hours: 6
  learn_batch_size: 10
  compile_interval_hours: 1
  health_check_interval_hours: 24

health:
  auto_fix_broken_links: true
  max_stubs_per_run: 10
```

Worker 在生产 WSGI 入口（`wsgi.py` → `start_worker_thread`）自动启动——单部署，无需额外队列或 cron。本地 `llmbase web` 开发时 worker 不会自启；要在本地跑自治循环，请用 `gunicorn wsgi:app`，或自行调用 `tools.worker.start_worker_thread`。

### 定制与扩展

"库而非框架"。import 时覆盖模块常数，或注册 hook：

```python
import tools.compile as c
import tools.query   as q
from tools.hooks     import register
from tools.worker    import register_learn_source
from tools.operations import register as register_op, Operation

c.SECTION_HEADERS = [("wenyan", "## 文言")]                 # 单语 KB
q.TONE_INSTRUCTIONS["formal_zh"] = "請以正式中文回答。"       # 自定义语气
register("compiled", my_sync_handler)                        # 编译后回调
register_learn_source("my_corpus", my_handler)               # 自定义学习源
register_op(Operation(                                       # 一次注册，三面同暴露
    name="kb_custom", description="自定义 op",
    handler=my_handler,
    params={"type": "object", "properties": {"query": {"type": "string"}}},
))
```

完整契约见 [docs/customization.md](docs/customization.md)（涵盖 compile / query / taxonomy / xici / entities / lint / web / worker / operations）。

### 线上实例

两者均为纯 `llmwiki` 依赖，定制全走 hook + override，无 fork。

- **[華藏閣](https://huazangge-production.up.railway.app)**——自治佛学 KB，从 CBETA 大藏经持续学习，三语；Supabase 同步通过生命周期 hook 接入
- **[斯文](https://siwen.ink)**——文言知识库，儒释道经典，纯文言前端；通过定制契约启用 CJK slug

### 设计原则

- **域无关**——分类由内容涌现，零硬编码
- **不依赖向量库**——个人规模 markdown + 合理 tokenizer 足矣，`kb_search_raw` 补足原文召回
- **探索会累加**——每次查询、lint、摄入都让知识库更强
- **LLM 写，你审**——LLM 维护，你指导方向
- **增量而非批处理**——叠加进化，不覆盖重来
- **可扩展而不 fork**——契约稳定
- **Agent 原生**——人与 agent 平权
- **自愈**——系统自己发现并修复漂移

### 部署

```bash
docker compose up -d                                                    # Docker
railway up                                                              # Railway
gunicorn --bind 0.0.0.0:5555 --workers 2 --timeout 300 wsgi:app         # 任意 VPS
```

---

## Star History

<a href="https://star-history.com/#Hosuke/llmbase&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hosuke/llmbase&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hosuke/llmbase&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hosuke/llmbase&type=Date" />
 </picture>
</a>

## License

MIT

---

<div align="center">
<sub>Built with LLMs, for LLMs. Knowledge compounds. 温故而知新。</sub>
</div>
