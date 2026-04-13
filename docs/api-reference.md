# API Reference

All endpoints are served by the web server (`llmbase web`). Read endpoints are generally open; write endpoints and sensitive data (trails, health fixes) require auth in cloud deployments.

## Authentication

```
Authorization: Bearer <LLMBASE_API_SECRET>
```

Local dev: no auth needed. Cloud: auto-generated or set via env var.

## Articles

### List Articles
```
GET /api/articles
→ { "articles": [{ "slug", "title", "summary", "tags" }] }
```

### Get Article
```
GET /api/articles/<slug>
→ { "slug", "title", "summary", "tags", "content", "sources", "backlinks" }
```

Supports alias resolution: `/api/articles/参禅` → resolves to `can-chan`.

### Delete Article (auth required)
```
DELETE /api/articles/<slug>
→ { "status": "ok", "deleted": "slug" }
```

## Query

### Ask (Deep Research)
```
POST /api/ask
{ "question": "What is X?", "deep": true, "tone": "wenyan", "file_back": true }
→ { "answer": "...", "consulted": [{"slug", "title"}], "output_path": "wiki/outputs/..." }
```

Tones: `default`, `caveman`, `wenyan`, `scholar`, `eli5`

`output_path` is returned only when `file_back=true`. It is project-root
relative when the configured outputs dir lies under the project (the
common case), and falls back to a bare filename if the outputs dir is
configured outside the project base. Either way it is never an absolute
filesystem path, so it can be safely shown to clients. Use it to link a
research trail to its filed answer without fuzzy-matching by title.

### Search
```
GET /api/search?q=keyword&top_k=10
→ { "results": [{ "slug", "title", "score", "snippet" }] }
```

## Knowledge Structure

### Taxonomy
```
GET /api/taxonomy?lang=zh
→ { "categories": [{ "id", "label", "count", "total", "articles", "children" }] }
```

### Aliases
```
GET /api/aliases
→ { "aliases": { "参禅": "can-chan", "can-chan": "can-chan" } }
```

### Guided Reading (导读)
```
GET /api/xici?lang=zh
→ { "text": "...", "themes": [...], "lang": "zh", "generated_at": "..." }

POST /api/xici/generate  (auth required)
{ "lang": "zh" }
→ { "text": "...", "themes": [...] }
```

## Entities (opt-in)

```
GET /api/entities
→ { "people": [...], "events": [...], "places": [...] }

POST /api/entities/extract  (auth required)
→ { "people": [...], "events": [...], "places": [...] }
```

## Research Trails (auth required)

```
GET /api/trails
→ { "trails": [{ "id", "name", "steps": [{ "type", "slug", "question", "ts" }] }] }

POST /api/trails
{ "trail_id": null, "step": { "type": "query", "question": "..." }, "name": "My Trail" }
→ { "trail": { ... } }

POST /api/trails/<id>/delete
→ { "status": "ok" }
```

## Health & Repair

### Lint Check
```
POST /api/lint
{ "deep": false }
→ { "results": { "structural", "broken_links", "orphans", "missing_metadata", "dirty_tags", "duplicates", "stubs", "uncategorized", "total_issues" } }
```

### Auto-Fix (auth required, runs in background)
```
POST /api/lint/fix
→ { "status": "started", "message": "..." }
```

### Health Report
```
GET /api/health
→ { "report": { "checked_at", "results", "fixes_applied" } }
```

### Clean Garbage (auth required)
```
POST /api/wiki/clean
→ { "removed": 5, "slugs": ["slug1", "slug2"] }
```

## Raw Sources

```
GET /api/sources
→ { "documents": [{ "path", "title", "type", "compiled", "ingested_at", ...all frontmatter fields }] }

GET /api/sources/<slug>
→ { "slug", "title", "type", "compiled", "content", "metadata" }
Content capped by config: sources.max_content_chars (default 50K, max 500K)
```

## Ingest & Compile (auth required)

```
POST /api/ingest
{ "source": "https://example.com/article" }

POST /api/upload
multipart/form-data with file

POST /api/compile
→ { "articles_created": 3 }

POST /api/index/rebuild
→ { "article_count": 250 }
```

## Reference Sources

```
GET /api/refs/plugins
→ { "plugins": [{ "id": "cbeta", "name": { "en", "zh", "ja" } }] }
```

## Stats

```
GET /api/stats
→ { "raw_count", "article_count", "output_count", "total_words", "link_count", "health_score" }
```
