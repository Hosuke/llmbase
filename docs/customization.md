# Customization & Extension Guide

LLMBase is designed as a domain-agnostic library. Downstream projects customize behavior by overriding module-level constants and registering hooks — no forking needed.

## Table of Contents

- [Module Constants](#module-constants)
- [Lifecycle Hooks](#lifecycle-hooks)
- [Worker Extensibility](#worker-extensibility)
- [Web Extensibility](#web-extensibility)
- [Configuration Options](#configuration-options)

---

## Module Constants

Override at import time, before any function calls:

```python
# patches.py — run this at startup
import tools.compile as compile_mod
import tools.query as query_mod
import tools.taxonomy as tax_mod
import tools.lint.checks as checks_mod

# Single-language KB (e.g. classical Chinese)
compile_mod.SECTION_HEADERS = [("wenyan", "## 文言")]
compile_mod.COMPILE_ARTICLE_FORMAT = "## 文言\n\n以文言撰寫完整內容。"
compile_mod.SYSTEM_PROMPT = "You are a classical-Chinese knowledge compiler..."

# Custom query tone
query_mod.TONE_INSTRUCTIONS["formal_zh"] = "請以正式中文回答。"

# Rule-based taxonomy (skip LLM)
tax_mod.TAXONOMY_GENERATOR = my_keyword_taxonomy_fn

# Accept CJK slugs
checks_mod.ALLOW_CJK_SLUGS = True
```

### Full Constants Reference

| Module | Constant | Purpose |
|--------|----------|---------|
| `tools.compile` | `SYSTEM_PROMPT` | LLM system message for compilation |
| `tools.compile` | `COMPILE_USER_PROMPT` | User prompt template (`{title}`, `{content}`, `{existing}`, `{article_format}`) |
| `tools.compile` | `COMPILE_ARTICLE_FORMAT` | Example article format in user prompt (most common override) |
| `tools.compile` | `SECTION_HEADERS` | Language sections: `[("key", "## Header"), ...]` |
| `tools.taxonomy` | `TAXONOMY_SYSTEM_PROMPT` | LLM system message for taxonomy |
| `tools.taxonomy` | `TAXONOMY_LABEL_KEYS` | Language keys in label dicts (default `["en", "zh", "ja"]`) |
| `tools.taxonomy` | `TAXONOMY_GENERATOR` | Callable `(articles, cfg) -> tree` or `None` for LLM |
| `tools.query` | `SYSTEM_PROMPT` | LLM system message for Q&A |
| `tools.query` | `TONE_INSTRUCTIONS` | Dict of `tone_id -> instruction_string` |
| `tools.xici` | `XICI_SYSTEM_PROMPT` | LLM system for guided introduction |
| `tools.xici` | `LANG_STYLES` | Dict of `lang -> style_instruction` |
| `tools.entities` | `ENTITY_SYSTEM_PROMPT` | LLM system for entity extraction |
| `tools.entities` | `ENTITY_PROMPT` | User prompt template for entities |
| `tools.entities` | `ENTITY_ARTICLE_FORMATTER` | Callable `(articles) -> list[str]` or `None` |
| `tools.lint.checks` | `ALLOW_CJK_SLUGS` | Accept CJK slugs as valid (bool) |
| `tools.lint.checks` | `SYSTEM_PROMPT` | LLM system for deep lint |
| `tools.lint.fixes` | `STUB_SYSTEM_PROMPT` | LLM system for stub generation |

---

## Lifecycle Hooks

Register callbacks for key events. Hooks are best-effort: exceptions are logged but never propagate.

```python
from tools.hooks import register

# Sync to remote DB after compilation
register("compiled", lambda source, work_id, **kw: sync.push(source, work_id))

# Notify on health issues
register("after_lint_check", lambda total_issues, **kw: 
    alert(f"{total_issues} issues") if total_issues > 10 else None)

# Log all ingestions
register("ingested", lambda source, title, **kw:
    logger.info(f"Ingested: {title} from {source}"))
```

### Events Reference

| Event | Emitter | Kwargs |
|-------|---------|--------|
| `ingested` | ingest.py | `source`, `title`, `path`, `url?` |
| `before_compile` | compile.py | `batch_size`, `titles` |
| `compiled` | compile.py | `source`, `work_id`, `raw_type`, `title`, `metadata` |
| `after_compile_batch` | compile.py | `count`, `articles` |
| `index_rebuilt` | compile.py | `article_count` |
| `taxonomy_generated` | taxonomy.py | `category_count`, `article_count`, `generated` |
| `after_lint_check` | lint/checks.py | `total_issues`, `results` |
| `after_auto_fix` | lint/fixes.py | `fix_count`, `fixes` |
| `xici_generated` | xici.py | `lang`, `article_count` |
| `entity_extracted` | entities.py | `people_count`, `events_count`, `places_count`, `article_count` |

---

## Worker Extensibility

### Custom Learn Sources

```python
from tools.worker import register_learn_source

def learn_from_arxiv(batch_size, base_dir, **kwargs):
    """Ingest papers from arXiv."""
    papers = fetch_arxiv(batch_size)
    paths = []
    for paper in papers:
        path = ingest_paper(paper, base_dir)
        paths.append(str(path))
    return paths

register_learn_source("arxiv", learn_from_arxiv)
```

Then in `config.yaml`:
```yaml
worker:
  enabled: true
  learn_source: arxiv       # uses your registered handler
  learn_batch_size: 5
```

### Custom Background Jobs

```python
from tools.worker import register_job

def sync_to_supabase(base_dir):
    """Push wiki state to Supabase every 2 hours."""
    ...

register_job("supabase_sync", interval_hours=2, handler=sync_to_supabase)
```

Custom jobs run in the same worker loop, share the global `job_lock`, and have the same crash-guard protection as built-in tasks.

---

## Web Extensibility

### Custom API Routes

**Option 1: EXTRA_ROUTES (before create_web_app)**

```python
import tools.web as web

def my_classics_api():
    from flask import jsonify
    return jsonify({"classics": get_classics_list()})

web.EXTRA_ROUTES.append(("/api/classics", my_classics_api, {"methods": ["GET"]}))
app = web.create_web_app(base_dir)
```

**Option 2: Flask Blueprint (after create_web_app)**

```python
from flask import Blueprint, jsonify
from tools.web import create_web_app

classics_bp = Blueprint("classics", __name__)

@classics_bp.route("/api/classics")
def list_classics():
    return jsonify({"classics": get_classics_list()})

app = create_web_app(base_dir)
app.register_blueprint(classics_bp)
```

### Request Middleware

```python
import tools.web as web

def log_requests():
    import logging
    from flask import request
    logging.getLogger("api").info(f"{request.method} {request.path}")

web.BEFORE_REQUEST_HOOKS.append(log_requests)
app = web.create_web_app(base_dir)
```

### Protected Extra Routes

`require_auth` is a module-level decorator that enforces the same
`LLMBASE_API_SECRET` / session-cookie check used by built-in write
endpoints. Wrap custom handlers with it so downstream routes honour the
same auth contract:

```python
import tools.web as web
from tools.web import require_auth
from flask import jsonify

@require_auth
def my_write_handler():
    return jsonify({"status": "ok"})

web.EXTRA_ROUTES.append(("/api/my-write", my_write_handler, {"methods": ["POST"]}))
app = web.create_web_app(base_dir)
```

When `LLMBASE_API_SECRET` is unset (local/dev), the decorator is a
no-op — same behaviour as the built-in routes.

### Runtime Config (base_dir, cfg)

`create_web_app` publishes the resolved `base_dir`, loaded `cfg`, and
auth tokens under `app.config["llmbase"]`. Handlers registered via
`EXTRA_ROUTES` or blueprints should read from there rather than calling
`Path.cwd()` or re-loading config:

```python
from flask import current_app, jsonify

def my_handler():
    llm = current_app.config["llmbase"]
    base_dir = llm["base_dir"]   # Path — project root
    cfg = llm["cfg"]             # dict — loaded config.yaml
    # ... api_secret / session_token also available if needed
    return jsonify({"root": str(base_dir)})
```

### Session Token

```python
from tools.web import derive_session_token
import os

secret = os.getenv("LLMBASE_API_SECRET", "")
token = derive_session_token(secret)
# Use token for custom cookie/auth logic
```

---

## LLM Layer (env vars)

Since 0.5.0 the LLM call layer takes no implicit fallback. All behavior is
explicit via env vars:

| Env var | Default | Notes |
|---------|---------|-------|
| `LLMBASE_API_KEY` | — | Required. Falls back to `OPENAI_API_KEY`. |
| `LLMBASE_BASE_URL` | OpenAI | Any OpenAI-compatible endpoint. |
| `LLMBASE_MODEL` | `gpt-4o` | Primary model. |
| `LLMBASE_FALLBACK_MODELS` | (empty = none) | Comma-separated. Empty/unset means **no fallback** — only the primary model is retried. Set explicitly if you want a chain (e.g. `gpt-4o-mini,deepseek-chat`). |
| `LLMBASE_PRIMARY_RETRIES` | `3` | Per-model retry budget for the primary. |
| `LLMBASE_FALLBACK_RETRIES` | `1` | Per-model retry budget for each fallback. |
| `LLMBASE_API_SECRET` | (empty) | When set, write endpoints require Bearer/cookie auth. |

> **Migration from ≤0.4**: earlier versions auto-generated a fallback
> chain (`gpt-4o → gpt-4o-mini → gpt-3.5-turbo`, `MiniMax-M2.7 → M2.5
> → deepseek-chat`, etc.). On aggregator deployments where the API
> token only had rights to the primary model, this caused silent 403
> loops. 0.5.0 removes the autogen — your existing
> `LLMBASE_FALLBACK_MODELS` setting (if any) keeps working unchanged;
> empty/unset now means what it says.

## Configuration Options

New config.yaml options added in recent releases:

```yaml
# Web server
web:
  static_dir: "./my-frontend/dist"    # Custom frontend build path (default: static/dist)

# Source API
sources:
  max_content_chars: 50000            # Content cap for /api/sources/<slug> (default 50K, max 500K, null = 500K)

# Lint
# Note: ALLOW_CJK_SLUGS is set via module constant, not config.yaml:
#   import tools.lint.checks as c; c.ALLOW_CJK_SLUGS = True
```

---

## Real-World Example: Classical Chinese KB

The [siwen.ink](https://siwen.ink) project customizes llmbase for a classical Chinese knowledge base using only patches — no forked functions:

```python
# patches.py — loaded at startup
import tools.compile as c
import tools.taxonomy as t
import tools.query as q
import tools.lint.checks as lc
from tools.worker import register_learn_source
from tools.hooks import register

# Single-language: 文言 only
c.SECTION_HEADERS = [("wenyan", "## 文言")]
c.COMPILE_ARTICLE_FORMAT = "## 文言\n\n以文言撰寫完整內容。"
c.SYSTEM_PROMPT = "You are a classical-Chinese knowledge compiler..."

# Rule-based taxonomy (四部分類)
t.TAXONOMY_GENERATOR = sibu_taxonomy

# Custom tone
q.TONE_INSTRUCTIONS["siwen"] = "以典雅文言作答..."

# Accept CJK slugs
lc.ALLOW_CJK_SLUGS = True

# Custom learn source
register_learn_source("ctext", ctext_learn_handler)

# Sync to remote on compile
register("compiled", push_to_supabase)
```
