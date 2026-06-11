# LLMBase Project Guidelines

## Architecture
- Domain-agnostic: no hardcoded domains, all structure emerges from content via LLM
- Three-layer: raw/ → wiki/concepts/ → config.yaml (Karpathy pattern)
- Trilingual by default: EN / 中文 / 日本語
- All wiki-links use [[target]] syntax; resolved via alias map (aliases.json)

## Code Patterns
- LLM calls go through llmwiki/llm.py:chat() — never call OpenAI directly
- Alias resolution via llmwiki/resolve.py — always use resolve_link() for wiki-link targets
- Taxonomy is LLM-generated (not hardcoded) — llmwiki/taxonomy.py
- Article slugs are pinyin/kebab-case; titles are bilingual "English / 中文"
- Never expose specific LLM provider names in public code or commits

## Customization Contract (for downstream projects)
Downstream projects can override module-level constants at import time to
customize behavior without forking functions. This is a **stable contract**.

| Module               | Constant                  | Purpose                                  |
|----------------------|---------------------------|------------------------------------------|
| llmwiki/compile.py     | SYSTEM_PROMPT             | LLM system message for compilation       |
| llmwiki/compile.py     | COMPILE_USER_PROMPT       | User prompt template ({title}, {content}, {existing}, {article_format}) |
| llmwiki/compile.py     | COMPILE_ARTICLE_FORMAT    | Example article format in user prompt    |
| llmwiki/compile.py     | DEFAULT_SECTION_HEADERS    | Identity anchor for patch detection — never reassign this; override SECTION_HEADERS by ASSIGNMENT instead |
| llmwiki/compile.py     | SECTION_HEADERS           | Language sections for split/merge (import-time assignment overrides config `languages:`) |
| llmwiki/config.py     | get_language_profile(cfg)  | Resolve active language profile: import-time SECTION_HEADERS patch > config `languages:` > builtin trilingual. Returns normalized {sections, views, codes, default_lang, api_default_lang, single_section, bare_body} |
| llmwiki/config.py     | get_section_headers(cfg)   | [(key, header)] from the active profile — same precedence; consumers (compile/export/query/worker/web) derive from this |
| llmwiki/taxonomy.py    | TAXONOMY_SYSTEM_PROMPT    | LLM system message for taxonomy          |
| llmwiki/taxonomy.py    | TAXONOMY_LABEL_KEYS       | Language keys in label dicts             |
| llmwiki/taxonomy.py    | TAXONOMY_GENERATOR        | Callable to replace LLM taxonomy (or None) |
| llmwiki/lint/checks.py | ALLOW_CJK_SLUGS           | Accept CJK slugs as valid (bool)         |
| llmwiki/lint/checks.py | SYSTEM_PROMPT             | LLM system for deep lint                 |
| llmwiki/lint/fixes.py  | STUB_SYSTEM_PROMPT        | LLM system for stub generation           |
| llmwiki/search.py      | SEARCH_TOKENIZER          | Callable(text)->list[str] to replace tokenizer (or None) |
| llmwiki/search.py      | STOPWORDS / CJK_STOPWORDS | Stopword sets used by default tokenizer  |
| llmwiki/query.py       | SYSTEM_PROMPT             | LLM system message for Q&A               |
| llmwiki/query.py       | TONE_INSTRUCTIONS         | Dict of tone_id → instruction string     |
| llmwiki/query.py       | PROMOTE_SYSTEM_PROMPT     | LLM system for Q&A→concept promotion judge |
| llmwiki/query.py       | PROMOTE_CONTENT_EXAMPLE   | Content-schema hint for promote judge (None = auto-derive from SECTION_HEADERS) |
| llmwiki/query.py       | PROMOTE_TITLE_EXAMPLE     | Title-schema hint for promote judge (None = auto-derive from SECTION_HEADERS) |
| config.yaml          | languages.default / languages.profiles | Language profile contract (single source of truth). Single section with header "" = bare-body monolingual mode (first-class) |
| config.yaml          | query.prefilter_threshold | Above this many articles, TF-IDF prefilter the index before LLM selector (default 500) |
| config.yaml          | query.prefilter_top_k     | Number of candidates to keep after prefilter (default 200) |
| llmwiki/xici.py        | XICI_SYSTEM_PROMPT        | LLM system for guided introduction       |
| llmwiki/xici.py        | LANG_STYLES               | Dict of lang → style instruction         |
| llmwiki/entities.py    | ENTITY_SYSTEM_PROMPT      | LLM system for entity extraction         |
| llmwiki/entities.py    | ENTITY_PROMPT             | User prompt template for entities        |
| llmwiki/entities.py    | ENTITY_ARTICLE_FORMATTER  | Callable to format article list for LLM  |
| llmwiki/export.py      | (uses SECTION_HEADERS)    | Language sections from compile module    |
| llmwiki/normalize.py   | SENTENCE_TERMINATORS      | Line terminators for paragraph-merge (default CJK + ASCII) |
| llmwiki/normalize.py   | CLOSING_WRAPPERS          | Brackets/quotes that may follow a terminator before merge-check |
| llmwiki/chunk_cache.py | ChunkCache(base, subdir=) | Content-hash-validated (cid, content_hash)→output cache for pipelines |
| llmwiki/split.py       | split_by_heading(body, level) | Flat section parse — `list[Section]` at target ATX depth; no heuristics |
| llmwiki/pipeline/      | run_stage(base, stage, key, *, ttl, meta_init) | Contextmanager — guarantees `ok`/`failed`/`partial` terminal event on every exit; SIGKILL recovery via stale-lock `interrupted` |
| llmwiki/pipeline/      | rebuild_state(base, stage, key) → StageState | Derive status/attempts/started/finished/last_err/artifacts/meta from log (append-only JSONL is source of truth) |
| llmwiki/pipeline/      | StagePartialExit / ctx.mark_partial(reason) | Handler marks run `partial` (not `ok`/`failed`) — e.g. LLM quota at chunk 50/62; next run resumes from cache |
| llmwiki/pipeline/      | RESERVED_EVENTS | Event names refused by `ctx.log()` (start/ok/failed/partial/interrupted/artifact/meta_update) — prefix custom events with `chunk_` / `cache_` |
| llmwiki/llm.py         | chat_with_meta(prompt, ...) → (str, LLMMeta) | Rich-return chat with finish_reason / usage (incl. reasoning_tokens) / attempts. `meta.truncated` == length-cut; primitive does NOT raise — caller decides policy |
| llmwiki/llm.py         | reasoning_budget(max_tokens, tokens_per_char, safety=0.8) | Safe input chunk size (chars) for output token budget. No upstream model table — caller supplies `tokens_per_char` empirically |
| llmwiki/anchor.py      | locate_span(content, head, tail="", *, normalize="punct_spaces") → Anchor \| None | Span anchor: head+tail both matched → `Anchor(strategy="exact")`; head only → `"head_only"` (spans to content end); no head match → `None`. Offsets are ORIGINAL content indices (not normalized). Empty head raises `ValueError` |
| llmwiki/anchor.py      | normalize_text(s, level)  | Public for JS mirror. Levels: `none` / `punct` / `punct_spaces`. Charset documented in docstring — keep any JS port in lockstep |
| llmwiki/web.py         | derive_session_token()    | Public function: secret → cookie token   |
| llmwiki/web.py         | require_auth              | Module-level decorator for EXTRA_ROUTES  |
| llmwiki/web.py         | app.config["llmbase"]     | Runtime dict: base_dir, cfg, api_secret, session_token |
| llmwiki/web.py         | EXTRA_ROUTES              | List of (rule, handler, options) tuples  |
| llmwiki/web.py         | BEFORE/AFTER_REQUEST_HOOKS| Request middleware lists                  |
| llmwiki/web.py        | GET /api/languages         | Public endpoint returning the normalized language profile; frontend lang.tsx derives LANG options/markers from it; ?lang= on taxonomy/xici validates against profile codes |
| llmwiki/worker.py      | LEARN_SOURCES             | Dict of source_name → learn handler      |
| llmwiki/worker.py      | CUSTOM_JOBS               | List of custom background jobs           |
| llmwiki/worker.py      | register_learn_source()   | Register custom learn source handler     |
| llmwiki/worker.py      | register_job()            | Register custom background job           |
| llmwiki/operations.py  | register(Operation(...))  | Register custom op (auto-exposed via CLI/HTTP/MCP) |
| llmwiki/operations.py  | dispatch(name, base, args)| Programmatic op invocation (with write-lock) |
| config.yaml          | web.static_dir            | Custom frontend build path               |

## Lifecycle Hooks (llmwiki/hooks.py)
Downstream registers callbacks via `llmwiki.hooks.register(event, callback)`.

| Event                | Emitter          | Kwargs                                     |
|----------------------|------------------|--------------------------------------------|
| `ingested`           | ingest.py        | source, title, path, url?                  |
| `before_compile`     | compile.py       | batch_size, titles                         |
| `compiled`           | compile.py       | source, work_id, raw_type, title, metadata |
| `after_compile_batch`| compile.py       | count, articles                            |
| `index_rebuilt`      | compile.py       | article_count                              |
| `taxonomy_generated` | taxonomy.py      | category_count, article_count, generated   |
| `after_lint_check`   | lint/checks.py   | total_issues, results                      |
| `after_auto_fix`     | lint/fixes.py    | fix_count, fixes                           |
| `xici_generated`     | xici.py          | lang, article_count                        |
| `entity_extracted`   | entities.py      | people/events/places_count, article_count  |

## Auto-Fix Pipeline (llmwiki/lint.py:auto_fix)
1. clean_garbage() — remove template stubs
2. fix metadata — LLM generates missing summary/tags
3. fix_broken_links() — alias-aware, only stubs for truly missing concepts
4. merge_duplicates() — LLM confirms, content 叠加进化
5. fix_uncategorized() — regenerate taxonomy

## Key Files
- llmwiki/resolve.py — alias system (central to all link resolution)
- llmwiki/xici.py — guided introduction generation
- llmwiki/taxonomy.py — emergent LLM-generated categories
- wiki/_meta/ — aliases.json, taxonomy.json, health.json, backlinks.json

## Commit Process (MANDATORY)
Before EVERY git commit, you MUST:
1. Run `cd frontend && npx tsc --noEmit` — TypeScript check
2. Run `python -c "from llmwiki.lint import lint; print('OK')"` — Python import check
3. Run Codex review on staged changes and WAIT for the result:
   ```
   codex exec --sandbox read-only -C . \
     --output-last-message /tmp/codex-review-result.txt \
     "Review the staged git diff for bugs, security, edge cases. file:line format. Say LGTM if clean."
   ```
4. Read the Codex review output. If there are HIGH issues, fix them BEFORE committing.
5. Only then run `git commit`

Do NOT skip Codex review. Do NOT commit while Codex is still running.

## CI Process
- TypeScript check: `cd frontend && npx tsc --noEmit`
- Python import check: `python -c "from llmwiki.lint import lint; print('OK')"`
- Lint check: `python llmbase.py lint check`
- Build: `cd frontend && npx vite build`

## Release Process (MANDATORY when bumping `pyproject.toml` version)
A version bump is NOT a release until it lands on PyPI AND ClawHub. Git tag
alone is insufficient — `pip install llmwiki` reads PyPI. When the version
in `pyproject.toml` changes, you MUST complete ALL of:

1. Commit + push git (with matching `vX.Y.Z` tag)
2. Publish to PyPI:
   ```
   rm -rf dist/ build/ *.egg-info
   python -m build
   twine upload dist/llmwiki-X.Y.Z*        # needs PYPI token
   ```
3. Publish SKILL.md to ClawHub (if skills/llmwiki/SKILL.md changed or version bumped):
   ```
   npx clawhub@latest publish skills/llmwiki --version X.Y.Z --changelog "..."
   ```
4. Verify: `pip index versions llmwiki` shows the new version; ClawHub page
   at https://clawhub.ai/hosuke/llmwiki shows it.

Do NOT consider a release complete until all three surfaces (git tag, PyPI,
ClawHub) show the new version. If you only push git, say so explicitly —
don't imply the release is live.
