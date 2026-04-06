# Reference Sources

Articles can show verifiable citations linking back to original texts. This is powered by a pluggable reference source system.

## How It Works

```
1. Raw doc is ingested with source metadata:
   source: https://cbetaonline.dila.edu.tw/zh/T0235
   type: buddhist_sutra

2. During compilation, source metadata flows to article:
   sources:
     - plugin: cbeta
       url: https://cbetaonline.dila.edu.tw/zh/T0235
       title: Heart Sutra

3. Article page shows "Sources" section:
   [cbeta] Heart Sutra (T0235) → link to CBETA Online
```

## Viewing Sources

- **Web UI**: Sources appear at the bottom of each article page
- **API**: `GET /api/articles/<slug>` includes `sources[]` in response
- **Plugin list**: `GET /api/refs/plugins`

## Built-in Plugins

| Plugin | Coverage | Example URL |
|--------|----------|-------------|
| CBETA | Buddhist canon | `cbetaonline.dila.edu.tw/zh/T0235` |
| Wikisource | Classical texts | `zh.wikisource.org/wiki/論語/學而` |
| ctext.org | Chinese classics | `ctext.org/analects/xue-er/zh` |

## Adding Your Own

See [Plugin Development Guide](plugin-development.md).

## Source Merging

When an article is compiled from multiple raw documents, sources are merged (deduplicated by `(plugin, url, work_id, title)` key). This means an article about "emptiness" compiled from both a CBETA sutra and a Wikisource text will show both source citations.
