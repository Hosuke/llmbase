# Changelog

All notable changes to LLMBase (llmwiki) will be documented in this file.

## [0.7.2] — 2026-04-18

### Added
- **`/api/articles/lite?tag=<slug>` server-side filter.** The lite endpoint (index.json-backed, no frontmatter parse — added in v0.6.4) now narrows in-process by frontmatter tag when `?tag=` is supplied. Empty match returns `200 {"articles": [], "total": 0}` — same shape as `/api/articles?tag=nonexistent`; lite deliberately doesn't load taxonomy to validate tag existence. Driven by siwen.ink (~13k articles): the sidebar's category view used to download every entry and filter client-side; now it pulls only the slice it renders. Matching is case-sensitive, exact (matches frontmatter storage), and tolerates the frontmatter pattern of a tag stored as a single string rather than a list (via the existing `_normalize_tags` helper, same path `/api/articles?tag=` already uses).
- **`LLMBASE_LITE_CACHE_MAX_AGE` env var.** When set to a positive integer, `/api/articles/lite` (and its 304 responses) emit `Cache-Control: public, max-age=<N>` instead of the default `no-cache`. Lets large-KB deployments take a browser-side fast path on every navigation; the existing ETag still handles freshness past the TTL. Default of `0` (or unset, or invalid value) keeps the existing `no-cache` behaviour, so no other endpoint or caller is affected. **Caveat:** during the max-age window, sidebars can lag behind a fresh compile — only set this if a few minutes of staleness is acceptable.

### Changed
- **`/api/articles/lite` ETag now keys on the tag param** (via the existing `_kb_etag(extra=...)` hook). Distinct slices get distinct ETags, so a 304 will never serve a stale partial slice to the wrong caller.

## [0.7.1] — 2026-04-18

### Added
- **Section-slicing API for long articles.** New `tools/sections.py` parses an article's body into a nested section tree (level + title + anchor + char offsets + children); fenced code blocks are skipped so `## ` lines inside ` ``` ` blocks aren't mis-treated as headings. Driven by 斯文·太虛間 needs (太虛大師全書: 法華 138k 字, 宗體論 112k 字 — single-article TOCs and chapter-level navigation were forcing the frontend to re-parse `####` heads itself).
- **`GET /api/articles/<slug>/sections`** — returns `{slug, title, sections}` where each section is `{level, title, anchor, start, end, children}`. Same alias-aware slug resolution and path-traversal guard as `/api/articles/<slug>`.
- **`kb_get_sections` operation** — table-of-contents discovery via the unified ops registry (CLI + HTTP + MCP).
- **`kb_get section=<anchor>` parameter** — extracts just that section's subtree (heading + content + descendants) using `body[start:end]`. Lets MCP clients fetch a single chapter from a 100k+ article without paying the full-body context cost. When `section` is omitted, behaviour is unchanged.

### Notes
- **Anchor format:** `h{level}-{slug-short}-{hash6}` (e.g. `h4-第三章判教-a3f95c`). `slug-short` is the title with invisibles / brackets / punctuation / dashes / whitespace stripped, truncated to 20 code points (CJK + ASCII / kana preserved). `hash6` is the first 6 hex digits of `sha1(joined-normalized-ancestor-chain)` — the *full* ancestor chain, joined by U+203A "›". (The original spec called for 4 hex; bumped to 6 after Codex pointed out 4-hex collisions hit ~50% per book at the section counts 太虛 reaches via the birthday bound — 6 hex drops it to ~0.3%.) Anchors are stable across (a) trivial whitespace / punctuation / zero-width / BiDi-control edits in any title and (b) sibling reordering. They break on (c) title 字 changes anywhere in the chain and (d) reparenting — both of which v0.7.2 will paper over with a content-hash-driven aliases map. Collisions append `-2`, `-3`.
- **CommonMark conformance:** ATX heading parsing follows §4.2 (max 3-space indent, trailing `#+` only stripped when whitespace-preceded — so `## C#` keeps the `#`); fenced code blocks follow §4.5 (closer must use the same fence char and be ≥ opener in length, no info string), so `` ```mermaid `` blocks containing `## ` lines never get parsed as headings even when nested inside longer fence runs.
- **v0.7.2 will add** persisted `wiki/_meta/sections/<slug>.json` tracking each section's content-hash; on re-compile, sections matched by content-hash but with shifted anchors emit an `aliases` field in the API response so old shared URLs keep resolving.

### Fixed (in this release, found by Codex pre-commit review)
- **Path-traversal guard tightened on `/api/articles/<slug>` and `/api/articles/<slug>/sections`.** Prior `str.startswith(str(concepts_dir.resolve()))` check was bypassable when `concepts_dir` shared a string prefix with a sibling directory (e.g. `concepts` vs `concepts_evil`). Switched to `Path.is_relative_to`, which compares path components rather than raw strings. The `kb_get` and `kb_get_sections` ops gained the same guard at the operations layer (previously had no guard at all — only the HTTP layer enforced it, so direct CLI/MCP callers were unprotected).

## [0.6.9] — 2026-04-18

### Added
- **Mermaid in `Markdown` component.** Wiki / classics / Q&A pages now render ` ```mermaid ` code fences as live diagrams. The `mermaid` library is dynamically imported the first time a diagram appears (no first-paint cost on plain pages) and re-renders on theme toggle, picking up the dark/light palette automatically. `securityLevel: 'strict'` so labels are escaped. Failures fall back to a labeled error block with the original source visible.
- **Deep-nest visual hierarchy in `.prose-article`.** `<ul>` and `<ol>` now rotate list-style markers per nesting depth (disc → circle → square → ▸ → ⋄ → ▫ → ◆ → ◇ for `<ul>`; decimal → lower-alpha → lower-roman → upper-alpha → upper-roman → cjk-ideographic → hiragana → katakana for `<ol>`) and gain a 1px outline-variant "rail" on the left from depth 2 onward, fading at depth 3+. Targeted at 古籍解經 / 太虛大師全書 content nested 7-8 layers (甲乙丙丁戊己庚辛) where Markdown's 6 head levels run out and bare nested lists collapse into a wall.

## [0.6.8] — 2026-04-18

### Fixed
- **Web-UI compile button survives navigation (issue #7).** The Ingest page held its `compiling` flag in local component state, so routing away and back re-enabled the button while the worker lock was still held — a second click then 409'd. The page now polls the new `/api/worker/status` endpoint on mount to recover in-flight state, keeps the button disabled while the lock is held, and falls back to the same polling path when a click 409s. Also generalises `api.compile()` to throw a typed `ApiError` so the UI can distinguish 409 (busy) from other failures.

### Added
- **`GET /api/worker/status`** — reports `{busy: bool}` derived from `tools.worker.job_lock.locked()`. Auth-gated (same `require_auth` policy as the write endpoints whose state it reflects); a no-op decorator when `LLMBASE_API_SECRET` is unset (local/dev). Intended for SPA polling and dashboard status widgets.

## [0.6.7] — 2026-04-18

### Fixed
- **`/api/ask` model override now requires the raw API secret when `LLMBASE_API_SECRET` is set.** v0.6.6 introduced a per-request `model` field on `/api/ask`, but left it open on public deployments so an untrusted caller could pin the most expensive model the backing API key can reach. When a secret is configured (prod signal), model override now returns 401 without `Authorization: Bearer <API_SECRET>`. Unlike `promote=True` — which still accepts the SPA-minted session cookie for browser convenience — model override refuses cookie auth, because the SPA cookie is handed out to anyone who loads `/` and provides no real barrier against a drive-by visitor pinning an expensive model. Local/dev mode (no secret) is unchanged.
- **URL-shaped slugs no longer corrupt the wiki namespace (issue #5).** `lint heal`'s broken-link stub fixer passed raw wiki-link targets through as filenames, so a target like `[[reasons-just-vs-expl/?ref=…]]` produced `concepts/reasons-just-vs-expl/?ref=….md` — a literal subdirectory that later crashed `lint heal` with `FileNotFoundError`. Slug sanitization is now centralised in `tools.compile.sanitize_slug()` and applied consistently across `_write_article` and `fix_broken_links`. A new `heal_urly_slugs` pass runs first in the `auto_fix` pipeline to rename surviving dirty files, rewrite wikilink references, and rebuild the index.
- **Surrogate sanitizer now reaches the deep-search selector prompt.** v0.6.6 applied `strip_surrogates` inside `chat_with_context`, but the earlier `query_with_search` selector call built its prompt directly from `index.json` titles/summaries and still crashed when those carried lone surrogates from pre-0.6.6 ingests.
- **`strip_surrogates` docstring corrected** — it substitutes `?` (0x3F), not U+FFFD; the docstring claimed the latter.

### Added
- **`LLMBASE_HTTP_TIMEOUT` / `LLMBASE_HTTP_CONNECT_TIMEOUT` env vars (issue #6).** The OpenAI client's HTTP read/connect timeouts were hard-coded at 300s/30s. Local Ollama users on large models (gpt-oss:20b with long context) routinely exceed 300s per call; there was no way to extend it without patching source. Both are now env-overridable with the former defaults.
- **`LLMBASE_MODEL_ALLOWLIST` env var.** Comma-separated allowlist for the `/api/ask` model override — applies to both authed and unauthed callers, gating which models may be selected at all. Complements the auth gate above.
- **`llmbase -v / -vv / -vvv` (issue #6).** CLI now accepts a top-level verbosity flag that configures logging for `llmbase.*`, `httpx`, and `openai`. `-v` enables INFO on llmbase internals, `-vv` adds INFO for HTTP clients (see requests land), `-vvv` enables wire-level DEBUG. Addresses "compile silently hangs / fails" debugging friction.
- **`tools.lint.fixes.heal_urly_slugs`** — public function; callable directly or via `auto_fix`.
- **`tools.compile.sanitize_slug`** — public helper for anyone building concepts outside the main compile path.

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
