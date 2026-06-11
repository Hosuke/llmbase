# Changelog

All notable changes to LLMBase (llmwiki) will be documented in this file.

## [0.9.0] — 2026-06-11

### Added
- **Language profile contract (`config.yaml` `languages:` + `llmwiki.config.get_language_profile(cfg)` / `get_section_headers(cfg)`).** Language structure now has a single source of truth: a `languages:` block with `default`, named `profiles`, profile `sections` (`code`, `key`, `header`, `label`, `icon`, `title_hint`), and `composite views` (`code`, `show`, `primary`, ...). Resolution is deliberately compatibility-first: import-time assignment to `compile.SECTION_HEADERS` still wins as the legacy customization contract (zero downstream breakage), then config `languages:`, then the builtin trilingual profile. The factory default is unchanged and byte-compatible, including the zh-en bilingual view and the historical `"zh"` API default.
- **Single-section empty-header profiles are first-class.** Profiles such as `[("文言", "")]` (the siwen monolingual pattern) now mean "the whole body is the section": `_split_sections`, `_assemble_sections`, and `_merge_into` keep a bare body with no heading line. The resolver validates an empty header only for single-section profiles, preserving real section headings everywhere else.
- **`GET /api/languages` and profile-aware language validation across HTTP surfaces.** The new endpoint returns the normalized profile. `?lang=` on `/api/taxonomy`, `/api/xici`, and `/api/xici/generate` now validates against profile codes and returns `400` with the valid list for unknown codes. The legacy patch path synthesizes a `"_patched"` profile, bypasses validation, and keeps the historical accept-all plus `"zh"` default; `/api/xici/generate` additionally checks `LANG_STYLES` capability for config-defined profiles. Invalid language config now fails as clean JSON `500` instead of leaking an internal traceback shape.
- **Frontend language contract derives from `/api/languages`.** `lang.tsx` no longer carries the hardcoded `Lang` union, static `LANG_OPTIONS`, or literal `## English` / `## 中文` / `## 日本語` markers. Bare-body profiles render content untouched with identity titles and a hidden language switcher; memo/effect dependencies track the live contract, and language query params are URL-encoded.
- **`docs/siwen-migration.md` migration notes.** The downstream siwen patches are now categorized by what can be retired and what must remain: `languages:` governs article structure, not LLM prompt wording.
- **Worker xici regeneration enumerates profile codes by capability.** Regeneration now walks normalized profile codes filtered by `LANG_STYLES`, while the legacy patch path preserves the existing four-tuple behavior.

### Fixed
- **Latent `_split_sections` empty-header regex bug.** An empty section header previously produced the regex `^\s*$`, so every blank line looked like a section boundary and splits were corrupted. Empty headers are now valid only through the explicit single-section profile path, where the whole body is treated as content.

## [0.7.10] — 2026-04-24

### Fixed
- **`tools.wikisource._wikitext_to_markdown` — `{{*|content}}` small-note template was being silently stripped.** The generic `\{\{[^}]{0,200}\}\}` template stripper ran before any unwrap rule, so Chinese Wikisource's pervasive small-note annotation marker — used to wrap the content of classical commentaries (王弼注, 河上公章句, 黃帝陰符經集註 seven-family notes, 周易參同契 諸家注) — was deleted along with the commentators' actual text, leaving only the MediaWiki definition-list markers (`:`, `::`) as naked empty lines. Siwen's 2026-04-24 discovery: `raw/wikisource-黃帝陰符經集註/index.md` had 131 naked-colon lines out of 183 total (72% of the file was empty scaffolding); `raw/wikisource-道德經-王弼本/index.md` had 948; `raw/wikisource-老子河上公章句-德經/index.md` had 495 — **王弼 entire commentary, 河上公 entire 章句, 七家 entire 註 all lost at ingest**. Downstream `wiki/concepts/*.md` summaries were compiled from the lossy text, so the data damage had propagated one layer further. Fix: unwrap `{{*|content}} → content` **before** the generic stripper (rule order matters — the unwrap must win). Other templates (`{{header|...}}`, etc.) continue to be stripped as before. Downstream recovery requires re-running `tools.wikisource.ingest()` on affected works to rewrite `raw/` from wikitext, then re-compiling the concept summaries.

## [0.7.8] — 2026-04-21

### Added
- **`tools.llm.chat_with_meta(...) -> (str, LLMMeta)` — 议 5-甲.** Rich-return variant of `chat()` exposing provider `finish_reason`, flattened `usage` (including `reasoning_tokens` pulled from OpenAI's nested `completion_tokens_details`), the actually-used model (may be a fallback), and total attempts across the retry + fallback tree. **Direct response to siwen's 2026-04-21 post-mortem of an 11-hour wenguan failure:** root cause was the absence of `finish_reason == "length"` detection — a ratio check replaced it and did not distinguish length-cut from `stop`, so truncated LLM output was cached as-if-complete for 62 books before anyone noticed. `meta.truncated` is the convenience boolean that maps to this field; callers decide whether to `mark_partial` / shrink / accept the prefix — primitive does NOT raise on length-cut (议 5-甲 design Policy A, "mechanism not policy", matches the rest of the customization contract).
  - **`chat()` is now a thin wrapper** over `chat_with_meta()` that drops the meta tuple element — **v0.7.x callers unchanged, zero breaking change**. All existing `chat() / chat_with_context() / query() / query_with_search() / _op_ask()` call-sites continue working verbatim.
  - **Empty-exhaustion path preserved**: when every retry returns empty content, returns `("", LLMMeta(...))` where meta reflects the LAST observed response shape — matches `chat()`'s pre-v0.7.8 silent-empty contract so downstream that guarded on `if not text:` keeps working.
  - `LLMMeta` is a `@dataclass` with fields `model: str`, `finish_reason: str | None`, `truncated: bool`, `usage: dict`, `attempts: int`. `usage` is flat: `{prompt_tokens, completion_tokens, reasoning_tokens, total_tokens}` — any key may be `None` when the provider omits it. `reasoning_tokens` is the one nested field we flatten explicitly; other provider-specific nesting is left alone so the dataclass stays OpenAI-compat-shaped without locking out future aliases.
  - **`_call_llm` internal shape changed** from `-> str` to `-> (content, finish_reason, usage)`. Private symbol (`_`-prefixed); existing test mocks at `tests/test_xllm_key.py:58/78` updated to return the new tuple. The `boom` raise-path mock at line 448 was return-shape-agnostic, unchanged.
- **`tools.llm.reasoning_budget(max_tokens, tokens_per_char, safety=0.8) -> int` — 议 5-乙.** Pure-arithmetic helper for sizing input chunks against the model's output token budget: `safe_chars = int(max_tokens * safety / tokens_per_char)`, clamped to `>= 1`. **No upstream model table, no implicit ratio.** `tokens_per_char` is a combined empirical metric (model token density × prompt output-expansion factor) the caller measures from real runs: `mean(meta.usage["completion_tokens"] / len(chunk))`. This matches the "mechanism not policy" pattern already in use by `HeadRule` (no default rules upstream), `TAXONOMY_GENERATOR` (None upstream), and `ChunkCache` (opaque cid semantics). Rationale: model ratios rot with every new model release and vary per prompt family; a table in upstream would put rot maintenance on llmbase and still be wrong for anyone's specific prompt. Siwen's 2026-04-21 post-mortem numbers check out: `reasoning_budget(32000, 17) == 1505 ≈ 1500` (their production target).
- **`docs/pipelines.md` — new "Choosing the `cid`" + "Sizing chunks" pattern sections (议 5-戊).** The `cid` guidance directly addresses the 2026-04-21 post-mortem symptom where a downstream had bound `cid = f"{prompt_name}:{chunk_index}/{CHUNK_MAX}"` — retuning `CHUNK_MAX` shifted every cid and invalidated the whole cache for drift-free chunks. Rule: if a downstream knob shifts your cid, you have the wrong cid. Bind cid to content identity (H3 title, 本位 id), never to input-size knobs. `content_hash` already handles content drift at a fixed slot. The "Sizing chunks" section shows the `chat_with_meta` + `reasoning_budget` composition that would have caught the 11-hour loss at the point of failure.
- **23 new tests** in `tests/test_llm_meta.py`: happy path + finish_reason propagation, length-cut surfaces `truncated=True` without raising, reasoning_tokens flattening from OpenAI nested shape (and `None` fallback when provider omits it), fallback bookkeeping (`meta.model` = fallback model, `meta.attempts` across the retry + fallback tree), empty-exhaustion preserves silent-empty + last-observed meta, last-seen semantics when retries switch between stop/length, api_key plumbing + no-leak regression on `repr(meta)`, `chat()` still returns bare `str` (v0.7.x regression), `reasoning_budget` formula + boundary conditions + ValueError on each bad-input axis + purity guarantee.

### Internal
- **Zero version-line holes** introduced — 0.7.8 lands cleanly on top of 0.7.7. Historical hole at 0.7.0 (reserved for X-LLM-Key which shipped as 0.7.4) stays as documented.

### Non-goals (explicit deferrals from siwen's 5th batch, 2026-04-21)
- **议 5-丙 `tools.split.aggregate_and_fallback`** — deliberately NOT shipped. Aggregate-and-fallback is heuristic; the "upstream stays parse-only" discipline that `split_by_heading` established (v0.7.6) matters more than the convenience. Let the pattern mature downstream for 2-3 months across siwen wenguan + whoever else adopts it; re-evaluate for upstream when the heuristics converge.
- **议 5-丁 "apply run_stage to wenguan_book"** — downstream integration task for siwen, not an llmbase change. The 0.7.7 primitives are already shipped.

## [0.7.7] — 2026-04-21

### Added
- **`tools/pipeline/` — composable primitives for multi-stage LLM workflows (议 D from siwen's 2026-04-20 third-batch proposal; converged with siwen through four spec rounds before implementation).** The upstream primitive layer for downstream pipelines that stitch together ingest → split → LLM → normalize → sync. No framework, no DAG, no scheduling, no retry policy — just three primitives and one driver. Siwen's 5-stage wenguan pipeline (62 太虛 books + 14 判教原經 + forthcoming canons) is the primary exercise target.
  - **Three laws (documented in `tools/pipeline/__init__.py` and each submodule):**
    1. **Log is truth.** Append-only JSONL at `{base}/.pipeline/{stage}/{sha256(key)}.jsonl`, one per (stage, key), never rewritten. `fcntl.LOCK_EX` on every append; torn writes self-heal on the next append (emit a leading `\n` when the prior tail lacks one, so a crashed appender's partial line is skipped by `iter_events` instead of being glued to the next event); readers decode UTF-8 per-line with byte-tolerant error handling so a torn multi-byte sequence does not poison iteration.
    2. **State is view.** `rebuild_state(base, stage, key)` walks the log and folds it into `StageState` per a stable contract (last terminal in the last round wins; attempts = count of start events; meta is last round only; artifacts union across all rounds, set-deduped). No `state.json` on disk — cache would drift on crash.
    3. **Driver guarantees terminal.** `run_stage(base, stage, key, *, ttl, meta_init)` contextmanager writes exactly one of `ok` / `failed` / `partial` on every exit path, and the next acquire after a SIGKILL writes `interrupted` before breaking the stale lock. No run is ever stuck `running` forever.
  - **Strict TTL semantics on `StageLock`:** TTL is informational (recorded in the pidfile for operator visibility), never enforcing. Lock is broken only when (a) pidfile missing/malformed/empty/missing required fields, (b) pidfile host differs from current host, or (c) pidfile PID is not alive (`os.kill(pid, 0)` raises `ProcessLookupError`). A live PID on the same host is **never** auto-broken, even decades past its recorded TTL — that's a hung process holding real resources (LLM session, file handle) and requires operator `force_break()` after `ps -fp <pid>`.
  - **`StagePartialExit` + `ctx.mark_partial(reason)`:** two paths for the wenguan reality where an LLM quota runs out at chunk 50/62 — both end the run as `partial` (not `failed`, not `ok`), so the next acquire can resume from cached chunks. Partial is a swallowed exit: the `with` block returns normally; callers detect via `rebuild_state(...).status == "partial"`.
  - **Reserved event names refused by `ctx.log()`:** `start`, `ok`, `failed`, `partial`, `interrupted`, `artifact`, `meta_update`. Downstream that names a custom event one of these gets a `ValueError` instead of silently masquerading as a driver signal. Prefix with `chunk_` / `cache_` / whatever to avoid the namespace.
  - **Opaque by design:** keys and meta dicts are free-form — any string / dict the caller owns. Stage names are caller-chosen within a safe charset (`[A-Za-z0-9_][A-Za-z0-9_.-]*`) since they land on disk as directory segments; the regex rejects path separators, leading dots, NUL, CJK, and whitespace, but covers realistic stage vocabularies (`wenguan`, `sync_partial`, `ingest-v2`, `s.1`). No enum on `stage`, no `Literal` narrowing on `status` (accepts any string the rebuild algorithm emits). Downstream composes its own `STAGES = [...]` and handlers without upstream churn.
  - **80 tests** covering: happy path / pending / reserved rejection / partial via mark / partial via exception / failed re-raise / `BaseException` still terminal / err truncation / artifact dedup within and across rounds / meta last-round-only semantics / meta_update reserved-key injection rejected / multi-round rebuild contract / `running` status when last round unterminated / full `interrupted` recovery round-trip (stale pidfile + log replay + acquire + new run) / `interrupted` ordered before new round's `start` / release-race no spurious `interrupted` / live-pid TTL lock never broken / cross-host broken / dead-pid broken / 9 parametrized malformed-pidfile cases (including `pid: true` bool-subclass and oversized pid triggering `OverflowError`) / non-UTF-8 pidfile stale / release idempotency / stage name path-traversal rejection (10 parametrized cases) / append timestamp authority (server `ts` overwrites caller) / torn-line skip + self-heal on next append / torn multi-byte UTF-8 skip / binary garbage line skip / `iter_events` non-dict JSON skip / `rebuild_state` tolerates non-dict `meta_init` / blank-line skip / `tail(limit)` / `multiprocessing.Pool(4) × 100` events concurrent-append no-interleave / `multiprocessing.Pool(8)` stale-break no simultaneous holder overlap (O_EXCL marker) / key hashed to full 64-hex sha256 / filesystem-unsafe key contained / stage×key isolation.
  - **Full 64-hex sha256 for on-disk key encoding**, matching `chunk_cache`'s v0.7.5 precedent. Early design proposed 16-hex for shorter directory listings; Codex HIGH (round 7) reclassified the failure mode as semantic state contamination (two colliding keys share a JSONL file; `rebuild_state` returns a mash-up history for either) rather than the narrower "spurious `StageBusyError`" I originally justified. Full hash costs nothing on APFS/ext4 and eliminates the window entirely.
  - **Security / robustness hardening across 8 Codex review rounds.** Every fix ships with a regression test:
    - **Atomic pidfile publication via tempfile + `os.link`** — eliminates the zero-byte window between `O_CREAT|O_EXCL` and content write, during which a racing acquirer could mis-classify the file as malformed-stale and break a still-initializing peer's lock.
    - **Stale-break serialization via dedicated `fcntl.flock` mutex** — only one process at a time is inside `_break_stale`, and it re-reads the pidfile under the mutex before deciding to unlink. Prevents the "slow racer unlinks the fresh winner's pidfile" class of bugs.
    - **Release-race disambiguation** — when `os.link` hits `FileExistsError` but a subsequent read gets `FileNotFoundError`, the loop retries the link silently instead of logging a spurious `interrupted` that would corrupt the prior run's `ok` terminal.
    - **Stage path-traversal guard** — stage names are validated against `[A-Za-z0-9_][A-Za-z0-9_.-]*`; separators, dots, NULs, CJK all rejected before touching the filesystem.
    - **`meta_update` kwargs can't inject reserved event names** — `event=` and `ts=` are refused as kwargs; defense-in-depth places the canonical `event` key after the spread so the log record is always `event=meta_update` regardless.
    - **`_is_stale` rejects `bool` pids** via `type(pid) is int` (`isinstance(True, int) == True` would otherwise probe PID 1 / init and lock the slot forever) and catches `OverflowError` from oversized pids.
    - **UTF-8 decode errors on torn writes are swallowed per line** — `iter_events` reads bytes and decodes line-by-line; a torn multi-byte CJK sequence in the log no longer aborts reconstruction. Pidfile reads go through a shared byte-tolerant helper so non-UTF-8 garbage in a lockfile classifies as malformed, not an uncaught exception.
    - **`rebuild_state` tolerates malformed `meta_init`** — if a hand-edited log line puts a non-dict in `meta_init`, rebuild degrades to empty meta instead of raising.
  - **Single-host scope is explicit.** Mutual exclusion relies on `socket.gethostname()` + `os.kill(pid, 0)`, both presupposing the `base_dir` filesystem belongs to one machine. On shared storage (NFS / CIFS / clustered FS) with multiple writing hosts a live holder on host A is classified stale by host B. Wrap this layer in an external cross-host mutex (etcd / consul / advisory NFSv4 flock) if you need multi-host semantics.

## [0.7.6] — 2026-04-20

### Added
- **`tools/split.py::split_by_heading(body, level) -> list[Section]` — 议 E.** Flat-section primitive for pipeline consumption: cuts a Markdown body at every ATX heading of the given level, returning a `list[Section]` with `level`, `title`, `header_line`, `start`, `end`, `content` fields. Strict primitive — **parse only**, no heuristics. No `is_single_book_bian`, no `merge_tiny_items`, no `strip_trailing_next_title` in upstream. Downstream (siwen 太虛, CBETA, …) composes `split_<corpus>.py` on top.
  - Rules (siwen's 议 E spec): only `level`-exact headings are split points; next same-or-higher-level heading ends a section (`#` ends a `##` section, `###` stays inside); fenced code blocks skipped (reuses `sections._iter_headings`); CommonMark §4.2 0-3 space indent tolerated; preface before first matched heading is **not** returned (downstream takes `body[:sections[0].start]`); no matching heading ⇒ `[]`.
  - Contrast with `tools/sections.py`: that module produces a nested tree with stable anchors for TOC / section-API use. This one is flat, single-level, sized for one-chunk-per-LLM-call pipeline chunking. No regex duplication — fence state machine reused.
  - 26 tests covering preface slicing, fenced pseudo-headings, 0-3 / 4+ indent boundary, CRLF, level=1/3/6, realistic 太虛 編/章/甲乙 structure.

## [0.7.5] — 2026-04-20

### Added
- **`tools/chunk_cache.py`: `ChunkCache(base)` — content-hash-validated cache for pipeline chunks (议 C from siwen's 2026-04-20 third-batch proposal, refined to the "primitive, no stage knowledge" form siwen and I converged on).** Fixes a concrete corruption seen in siwen wenguan the same day: chunks cached by positional key (`chunks/{idx:02d}.md`) were served stale when the splitter's boundaries shifted — 3 books (法華、仁王、宗體論) stitched together outputs for chunks that no longer existed. Keying by `(cid, content_hash)` means any content change at a slot produces a miss and forces recompute.
  - **API** (stable contract, matches siwen's refined spec):
    - `get(cid, content_hash) -> str | None` — hit requires both to match; content-hash mismatch is a miss (the whole point).
    - `put(cid, content_hash, output) -> None` — atomic via `atomic_write_text` → tempfile + POSIX rename; concurrent readers never see a torn file.
    - `clear(cid) -> None` — drop every stored hash for that cid; idempotent on unknown cids.
  - **No stage / domain knowledge in upstream**: `cid` and `content_hash` are opaque strings supplied by the caller. Downstream picks the strategy — siwen wenguan hashes chunk text for `content_hash` and uses the chunk's H3 title for `cid`; other pipelines could use line-range or slug+idx, whatever fingerprints their slot + content. This parallels `normalize_heads`'s rule-pack contract.
  - **Filesystem safety**: cids and content hashes are sha256-prefixed before hitting disk, so `../../etc/passwd` or CJK slashes as cids can't escape the cache directory.
  - Reusable `atomic_write_text` added to `tools/atomic.py` alongside the existing `atomic_write_json`.

## [0.7.4] — 2026-04-20

### Added
- **Per-request LLM API key override via `X-LLM-Key` header** on `/api/ask` (议 B from siwen's 2026-04-16 request; the feature originally slated for v0.7.0 — that version-line hole stays reserved historical). Callers who need multi-tenant / persona-switching (e.g. siwen's key1/key2 dual-identity 杳眇 文官 pipeline) can now pin a per-request credential without bypassing `/api/ask` and losing RAG, `file_back`, and job_lock.
  - **`tools/llm.py:get_client(api_key=None)`** — `None` returns the module-level singleton (cached), a string value returns a **fresh un-cached** client. Un-cached by design: mixing a caller-supplied key into the singleton would leak across subsequent requests.
  - **`chat()` / `chat_with_context()` / `query()` / `query_with_search()` / `_op_ask()`** gain `api_key: str | None = None` (forwarded through the full plumbing; omitted callers are unaffected).
  - **`kb_ask` op schema** declares `api_key` with `"writeOnly": true` so it never echoes in CLI op listings / MCP tool descriptions.
- **Non-negotiable security posture** (gate for the release):
  - HTTP **header-only**: `X-LLM-Key`. The request body is rejected with `400` if any key-bearing field is present — matched case-insensitively and with separator-normalization, so `api_key` / `apiKey` / `API-KEY` / `x-llm-key` / `openai_api_key` / `llm_key` all hit the same gate. Rationale: request bodies appear in proxy / WAF / access logs far more often than headers.
  - **Auth-gated on public deployments**: when `LLMBASE_API_SECRET` is set, `X-LLM-Key` requires `Authorization: Bearer <secret>` (same strong-auth gate as `model` override — cookie auth is **insufficient**, preventing drive-by browser visitors from burning the operator's key). When `LLMBASE_API_SECRET` is unset (local dev), the header is honoured without auth, matching `/api/ingest`.
  - **Key never logged**: `_redact_key` scrubs any literal occurrence of the per-request key from error strings before they reach `logger.debug` / `logger.warning`, and the final-retry exception is wrapped in a redacted `RuntimeError` so the key can't land in a caller traceback or HTTP 500 body.
  - **Key never in `outputs/`**: `_file_output` only takes `question / answer / format / cfg` — the key never reaches that call path.
  - **Response body never echoes the key** (regression test asserts this verbatim).
  - Promote-judge still uses the module singleton — meta-eval must be insulated from per-query keys (same rationale as the existing `model` carve-out).

### Changed (security hardening)
- **`Authorization` header now requires the literal `Bearer ` scheme.** The prior `.replace("Bearer ", "")` pattern silently accepted `Authorization: <secret>` with no scheme (a HIGH auth-bypass caught during X-LLM-Key review). This affects every endpoint protected by `require_auth` as well as the `/api/ask` strong-auth gate used by `model` override. **Breaking change for callers that sent the raw secret without `Bearer `** — the canonical form `Authorization: Bearer <secret>` has always been the documented contract, so legitimate clients are unaffected. If you see new 401s after upgrading, check your client emits `Authorization: Bearer`.

## [0.7.3] — 2026-04-19

### Added
- **`tools/normalize.py`** — two CommonMark-safe pre-process passes for classical-text corpora, upstreamed from siwen's 太虛大師全書 post-process scripts (議 A from 2026-04-19 third-batch proposal). Both passes skip fenced code blocks, indented code blocks, ATX headings (including empty `#`/`##`), list items (including multi-paragraph), blockquotes, table rows, thematic breaks, link reference definitions, and type 1-6 HTML block starters — only body paragraphs are touched. CRLF and mixed line endings preserved verbatim at EOF.
  - **`normalize_paragraphs(body)`** — merges a line into its predecessor when the predecessor doesn't end (after stripping trailing `CLOSING_WRAPPERS`) in a `SENTENCE_TERMINATORS` character. 古籍 OCR / web-scrape often splits sentences on visual column breaks; this reverses that without flattening real paragraph boundaries. Both character sets are module-level constants (customization contract) so downstream corpora can extend them.
  - **`normalize_heads(body, rules)`** — rewrites ATX heading levels when the title matches a regex in `rules` (first match wins). `HeadRule` is a `TypedDict{pattern, level}`; `rules == []` is a no-op (the upstream default). Downstream ships its own rule packs — e.g. siwen's 太虛 pack maps `^第[一二三…]+[章編篇卷]` → level 2, `^[甲乙丙…]、` → level 3.
  - **Empirical baseline** (siwen 太虛 62 books): ~1,500 head re-levels, ~14,000 paragraph merges. Library-only — no pipeline integration; callers invoke before `compile`. Fence + heading regexes reused from `tools/sections.py` (no duplication).

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
