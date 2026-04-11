# LLMBase Project Guidelines

## Architecture
- Domain-agnostic: no hardcoded domains, all structure emerges from content via LLM
- Three-layer: raw/ → wiki/concepts/ → config.yaml (Karpathy pattern)
- Trilingual by default: EN / 中文 / 日本語
- All wiki-links use [[target]] syntax; resolved via alias map (aliases.json)

## Code Patterns
- LLM calls go through tools/llm.py:chat() — never call OpenAI directly
- Alias resolution via tools/resolve.py — always use resolve_link() for wiki-link targets
- Taxonomy is LLM-generated (not hardcoded) — tools/taxonomy.py
- Article slugs are pinyin/kebab-case; titles are bilingual "English / 中文"
- Never expose specific LLM provider names in public code or commits

## Customization Contract (for downstream projects)
Downstream projects can override module-level constants at import time to
customize behavior without forking functions. This is a **stable contract**.

| Module               | Constant                  | Purpose                                  |
|----------------------|---------------------------|------------------------------------------|
| tools/compile.py     | SYSTEM_PROMPT             | LLM system message for compilation       |
| tools/compile.py     | COMPILE_USER_PROMPT       | User prompt template ({title}, {content}, {existing}, {article_format}) |
| tools/compile.py     | COMPILE_ARTICLE_FORMAT    | Example article format in user prompt    |
| tools/compile.py     | SECTION_HEADERS           | Language sections for split/merge        |
| tools/taxonomy.py    | TAXONOMY_SYSTEM_PROMPT    | LLM system message for taxonomy          |
| tools/taxonomy.py    | TAXONOMY_LABEL_KEYS       | Language keys in label dicts             |
| tools/taxonomy.py    | TAXONOMY_GENERATOR        | Callable to replace LLM taxonomy (or None) |
| tools/lint/checks.py | ALLOW_CJK_SLUGS           | Accept CJK slugs as valid (bool)         |
| tools/lint/checks.py | SYSTEM_PROMPT             | LLM system for deep lint                 |
| tools/lint/fixes.py  | STUB_SYSTEM_PROMPT        | LLM system for stub generation           |
| tools/query.py       | SYSTEM_PROMPT             | LLM system message for Q&A               |
| tools/query.py       | TONE_INSTRUCTIONS         | Dict of tone_id → instruction string     |
| tools/xici.py        | XICI_SYSTEM_PROMPT        | LLM system for guided introduction       |
| tools/xici.py        | LANG_STYLES               | Dict of lang → style instruction         |
| tools/entities.py    | ENTITY_SYSTEM_PROMPT      | LLM system for entity extraction         |
| tools/entities.py    | ENTITY_PROMPT             | User prompt template for entities        |
| tools/entities.py    | ENTITY_ARTICLE_FORMATTER  | Callable to format article list for LLM  |
| tools/export.py      | (uses SECTION_HEADERS)    | Language sections from compile module    |
| tools/web.py         | derive_session_token()    | Public function: secret → cookie token   |

## Lifecycle Hooks (tools/hooks.py)
Downstream registers callbacks via `tools.hooks.register(event, callback)`.

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

## Auto-Fix Pipeline (tools/lint.py:auto_fix)
1. clean_garbage() — remove template stubs
2. fix metadata — LLM generates missing summary/tags
3. fix_broken_links() — alias-aware, only stubs for truly missing concepts
4. merge_duplicates() — LLM confirms, content 叠加进化
5. fix_uncategorized() — regenerate taxonomy

## Key Files
- tools/resolve.py — alias system (central to all link resolution)
- tools/xici.py — guided introduction generation
- tools/taxonomy.py — emergent LLM-generated categories
- wiki/_meta/ — aliases.json, taxonomy.json, health.json, backlinks.json

## Commit Process (MANDATORY)
Before EVERY git commit, you MUST:
1. Run `cd frontend && npx tsc --noEmit` — TypeScript check
2. Run `python -c "from tools.lint import lint; print('OK')"` — Python import check
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
- Python import check: `python -c "from tools.lint import lint; print('OK')"`
- Lint check: `python llmbase.py lint check`
- Build: `cd frontend && npx vite build`
