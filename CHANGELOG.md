# Changelog

All notable changes to LLMBase (llmwiki) will be documented in this file.

## [0.6.0] — 2026-04-14

### Added
- **Unified operations contract** (`tools/operations.py`) — 17 canonical KB operations declared once, dispatched by CLI, agent HTTP, and MCP server from the same registry. Eliminates the three-way drift that existed between `cli.py`, `agent_api.py`, and `mcp_server.py` in 0.5.x.
- **`register(Operation(...))`** — downstream projects add custom operations at import time; they auto-surface in all three surfaces (MCP tools list, `/api/op/<name>`, `llmbase ops call`).
- **`llmbase ops list` / `llmbase ops call <name> --json-args '...'`** — generic CLI dispatcher matching the MCP tool names.
- **`POST /api/op/<name>` + `GET /api/op`** — generic HTTP dispatcher. Legacy semantic endpoints (`/api/ask`, `/api/search`, `/api/articles`, …) remain as wrappers for backwards compatibility.

### Changed
- **`mcp_server.py` slimmed from 398 to ~90 lines.** TOOLS list + dispatch now generated from the operations registry; no more hand-maintained duplication.
- MCP tool handlers now use `operations.dispatch`, picking up the write-lock automatically for ops marked `writes=True`.

### Notes
- No breaking changes. All existing CLI subcommands, HTTP endpoints, and MCP tool names are preserved.
- All write surfaces route through `operations.dispatch`, which acquires `tools.worker.job_lock` for ops marked `writes=True`:
  - MCP tool calls (`tools/mcp_server.py`)
  - CLI (`llmbase ops call <name>`)
  - `POST /api/op/<name>` (generic, `tools/agent_api.py`)
  - Legacy agent-HTTP: `/api/ingest`, `/api/compile`, `/api/lint/fix`, `/api/index/rebuild`, `/api/ask` (`tools/agent_api.py`)
  - Web-UI HTTP: `/api/ingest`, `/api/compile`, `/api/index/rebuild`, `/api/ask` (`tools/web.py`)
- Two ops escalate to the lock based on arguments (not just the `writes` flag): `kb_ask` with `promote=True`, and `kb_lint` with the legacy `fix=True`. The Web-UI `/api/lint/fix` keeps its pre-existing background-thread pattern (acquires `job_lock` itself).

## [0.5.2] — 2026-04-13

### Fixed
- **`promote_to_concept` now respects `SECTION_HEADERS`.** The Q&A→concept promotion judge had the trilingual content schema hard-coded in its prompt, so downstream projects that override `compile.SECTION_HEADERS` (e.g. siwen's single-section `[("文言", "")]`) still got articles written with `## English / ## 中文 / ## 日本語` sections. Prompt examples are now derived from `compile.SECTION_HEADERS` at call time.

### Added
- **`PROMOTE_CONTENT_EXAMPLE` / `PROMOTE_TITLE_EXAMPLE`** (tools/query.py) — module-level overrides for the promote judge's content/title schema hints. Default `None` auto-derives from `SECTION_HEADERS`.

## [0.5.1] — 2026-04-13

### Added
- **CJK-aware default search tokenizer** — `tools/search.py:_tokenize` now emits Latin words (filtered by `STOPWORDS`, len>1) plus CJK single chars and bigrams. Previously `\w+` captured an entire CJK run as one token, so single-char or short-phrase CJK queries returned nothing — making search effectively unusable for CJK-heavy bases (siwen, huazangge, etc.). English search behavior is unchanged.
- **`SEARCH_TOKENIZER` customization point** (tools/search.py) — set to a `Callable[[str], list[str]]` to fully replace the tokenizer (e.g., for jieba/MeCab). Default `None` uses the built-in CJK-aware tokenizer.
- **`STOPWORDS` / `CJK_STOPWORDS`** module-level sets — overridable by downstream.

### Changed
- IDF document-frequency check now uses cached `tokens_set` (O(1) membership) instead of `term in tokens_list` (O(n)).

## [0.5.0] — 2026-04-13

### ⚠️ Breaking
- **`get_fallback_models()` no longer auto-generates a fallback chain.** Empty/unset `LLMBASE_FALLBACK_MODELS` now means *no fallback* (only the primary model is retried). Previous releases guessed `gpt-4o-mini`, `MiniMax-M2.5`, etc., which silently failed on aggregator deployments where the API token only had rights to the primary model. Downstream that relies on fallback must now set the env var explicitly:
  ```
  LLMBASE_FALLBACK_MODELS=gpt-4o-mini,gpt-3.5-turbo
  ```

### Added
- **`LLMBASE_PRIMARY_RETRIES`** (default 3) and **`LLMBASE_FALLBACK_RETRIES`** (default 1) env vars — tune retry budget per role. Helpful for aggregators with transient 5xx where the primary model recovers if retried more aggressively before falling back.
- **`/api/ask` returns `output_path`** when `file_back=true` — frontend no longer has to guess the filed-back filename.
- **`query()` `return_path` flag** — when True, returns `{"answer", "output_path"}` dict instead of bare string. `query_with_search(return_context=True)` also includes `output_path` in its dict.

## [0.4.0] — 2026-04-12

### Added
- **`require_auth` module-level decorator** — downstream EXTRA_ROUTES handlers and blueprints can now wrap custom views with the same `LLMBASE_API_SECRET` / session-cookie check used by built-in write endpoints. Import via `from tools.web import require_auth`.
- **`app.config["llmbase"]` namespace** — `create_web_app` now publishes `base_dir`, `cfg`, `api_secret`, and `session_token` under a single config key. Extension handlers reach runtime paths via `current_app.config["llmbase"]` instead of `Path.cwd()` or re-loading config.

## [0.3.0] — 2026-04-12

### Added
- **Customization Contract** — downstream projects override module-level constants without forking
  - `COMPILE_USER_PROMPT`, `COMPILE_ARTICLE_FORMAT`, `SECTION_HEADERS` (compile.py)
  - `TONE_INSTRUCTIONS` (query.py), `XICI_SYSTEM_PROMPT`, `LANG_STYLES` (xici.py)
  - `ENTITY_SYSTEM_PROMPT`, `ENTITY_PROMPT`, `ENTITY_ARTICLE_FORMATTER` (entities.py)
  - `TAXONOMY_GENERATOR`, `TAXONOMY_LABEL_KEYS` (taxonomy.py)
  - `ALLOW_CJK_SLUGS` (lint/checks.py)
- **Lifecycle Hooks** — 10 events across 7 modules: `ingested`, `before_compile`, `compiled`, `after_compile_batch`, `index_rebuilt`, `taxonomy_generated`, `after_lint_check`, `after_auto_fix`, `xici_generated`, `entity_extracted`
- **Worker Extensibility** — `register_learn_source()` and `register_job()` replace hardcoded source routing; built-in cbeta/wikisource auto-registered
- **Web Extensibility** — `EXTRA_ROUTES`, `BEFORE_REQUEST_HOOKS`, `AFTER_REQUEST_HOOKS`; configurable `web.static_dir` in config.yaml
- **Session Token API** — `derive_session_token()` public function for custom auth middleware
- **Source API Enhancement** — `/api/sources` returns all frontmatter fields; `/api/sources/<slug>` content cap configurable via `sources.max_content_chars`
- **QA Concept Promotion** — semi-auto promotion of Q&A answers to wiki concepts
- **Customization Guide** — `docs/customization.md` with examples for constants, hooks, worker, web

### Changed
- **Taxonomy Phase 2** — removed domain-specific examples (Confucianism, Buddhism, etc.) from prompt; now fully domain-agnostic
- **Export** — `export_article()` uses `compile.SECTION_HEADERS` at runtime (not import-time copy)
- **Merge** — `_merge_into` / `_split_sections` / `_assemble_sections` driven by configurable `SECTION_HEADERS`
- **Design Philosophy** — added "Extensible without forking" principle

### Fixed
- **Static dir** — pip-installed deployments correctly resolve `static/dist` path
- **Supabase sync** — upsert 409 conflict handling
- **Path security** — local filesystem paths redacted from `/api/sources` output; `web.static_dir` path-traversal guarded
- **Negative config values** — `max_content_chars` clamped; worker `interval_hours` validated

## [0.2.0] — 2026-04-07

### Added
- **Structured Export API** — `export_article`, `export_by_tag`, `export_graph` for downstream projects
- **MCP Server** — Model Context Protocol support for Claude Code, Cursor, Windsurf, ClawHub (12 tools)
- **Research Trails** — Rabbithole-style exploration paths, auto-generated from deep research queries
- **Entity Extraction** — opt-in people/events/places extraction with timeline, people, and map views
- **Guided Reading** — LLM-generated 导读 (literary introduction), 文言文 as base for all languages
- **Reference Sources** — pluggable citation system with CBETA, Wikisource, ctext.org plugins
- **Backlinks Panel** — article detail page shows "Cited by" with resolved backlinks
- **D3 Timeline** — horizontal time axis with era bands, glow effects, zoom/pan
- **Voice/Tone Modes** — caveman, 文言文, scholar, ELI5
- **Tag Normalization** — LLM merges synonymous tags across wiki
- **Test Suite** — 54 tests covering core modules
- **ClawHub Skill** — `npx clawhub install llmwiki`
- **PyPI Package** — `pip install llmwiki`

### Changed
- **Taxonomy** — now LLM-generated (emergent, domain-agnostic), not hardcoded
- **Search** — default to deep research, single "Ask" button
- **Graph** — density control slider, inverted-index links, adaptive force layout
- **QA** — Chinese defaults to wenyan (文言文) tone
- **Dependencies** — matplotlib, pymupdf, mcp, watchdog moved to optional extras

### Fixed
- **Alias System** — multilingual wiki-link resolution (参禅 → can-chan, 繁简互转)
- **Compile Dedup** — 3-layer duplicate prevention (slug + alias + CJK substring)
- **Thinking Mode** — extract_json handles MiniMax thinking tokens before JSON output
- **Security** — SSRF protection, path traversal guards, constant-time auth, atomic JSON writes, job lock
- **Taxonomy Labels** — fixed string→trilingual dict normalization
- **lint.py** — split into `lint/checks.py`, `lint/fixes.py`, `lint/dedup.py` (was 943 lines)

### Architecture
- `tools/lint/` — package with checks, fixes, dedup (was monolithic 943-line file)
- `tools/refs/` — pluggable reference source plugins (auto-discovery)
- `tools/export.py` — structured export for downstream projects
- `tools/entities.py` — entity extraction with dedup
- `tools/xici.py` — guided reading generation
- `tools/resolve.py` — alias resolution with opencc support
- `tools/atomic.py` — atomic file writes
- `tools/mcp_server.py` — MCP stdio server

## [0.1.0] — 2026-04-04

### Added
- Initial release: ingest, compile, query, search, lint, worker
- Trilingual output (EN/中/日)
- Web UI with React + Tailwind
- Agent HTTP API + Python SDK
- CBETA, ctext.org, Wikisource data source plugins
- D3.js knowledge graph
- Docker + Railway deployment
