# Autonomous Learning

LLMBase can learn on its own. Deploy it, configure a data source, and the worker will continuously ingest, compile, and maintain the knowledge base.

## How It Works

```
Worker Loop (runs alongside web server):
  every 6h  → learn: ingest batch from data source
  every 1h  → compile: process new raw docs into wiki
  every 12h → taxonomy: regenerate categories + guided reading
  every 24h → health: auto-fix pipeline (7 steps)
```

## Enable the Worker

```yaml
# config.yaml
worker:
  enabled: true
  learn_source: cbeta          # or: wikisource, both
  learn_interval_hours: 6
  learn_batch_size: 10
  compile_interval_hours: 1
  taxonomy_interval_hours: 12
  health_check_interval_hours: 24
```

## Data Source Plugins

### CBETA (Buddhist Canon)

```bash
# Manual
llmbase ingest cbeta-learn --batch 10
llmbase ingest cbeta-work T0235        # Specific text

# Automatic (worker)
worker:
  learn_source: cbeta
```

4,868 works, 223 million characters. Progressive learning — each run picks up where the last left off.

### Wikisource

```bash
llmbase ingest wikisource-learn --batch 5
```

Chinese classics from zh.wikisource.org: Confucian, Daoist, Legalist, Military texts.

### ctext.org

```bash
llmbase ingest ctext-book 论语 /analects/zh
llmbase ingest ctext-catalog confucianism
```

### Custom Sources

Any URL or local file:

```bash
llmbase ingest url https://example.com/article
llmbase ingest pdf ./book.pdf
llmbase ingest dir ./my-documents/
```

## What Happens During Compilation

1. LLM reads raw document
2. Extracts 1-5 key concepts
3. For each concept:
   - Checks if article already exists (3-layer dedup: slug, alias, CJK substring)
   - If exists → merges new content (叠加进化)
   - If new → creates trilingual article (EN/中/日)
4. Rebuilds index, aliases, backlinks

## Monitoring

- Dashboard shows article count, link count, health score
- `/health` page shows last check results
- Worker logs: `[learn]`, `[compile]`, `[taxonomy]`, `[health]`
- `wiki/_meta/health.json` — last health report
