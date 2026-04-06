# Plugin Development

LLMBase has two plugin systems: **data source plugins** (for ingesting content) and **reference source plugins** (for citations).

## Reference Source Plugins (`tools/refs/`)

Reference plugins provide verifiable citation links. When an article is compiled from a source (e.g., CBETA), the citation appears at the bottom of the article page.

### Create a Plugin

Drop a `.py` file in `tools/refs/`:

```python
# tools/refs/arxiv.py
PLUGIN_ID = "arxiv"
PLUGIN_NAME = {"en": "arXiv", "zh": "arXiv", "ja": "arXiv"}

def get_source_url(source: dict) -> str:
    """Build permalink from source metadata."""
    paper_id = source.get("paper_id", "")
    return f"https://arxiv.org/abs/{paper_id}" if paper_id else source.get("url", "")
```

**Required:**
- `PLUGIN_ID: str` — unique identifier
- `PLUGIN_NAME: dict` — trilingual display name `{"en", "zh", "ja"}`
- `get_source_url(source: dict) -> str` — build permalink URL

**Auto-discovery:** Any `.py` file in `tools/refs/` with a `PLUGIN_ID` is automatically registered. No configuration needed.

### How Sources Flow Through the System

```
raw doc (source: "https://arxiv.org/abs/2301.00001", type: "arxiv")
  ↓ compile
article frontmatter:
  sources:
    - plugin: arxiv
      url: https://arxiv.org/abs/2301.00001
      title: "Paper Title"
  ↓ web API
/api/articles/slug → sources[] in response
  ↓ frontend
Article page → "Sources" section with [arxiv] badge + link
```

### Existing Plugins

| Plugin | ID | Source |
|--------|----|--------|
| CBETA | `cbeta` | Buddhist canon online |
| Wikisource | `wikisource` | zh.wikisource.org |
| ctext.org | `ctext` | Chinese Text Project |

### API

```
GET /api/refs/plugins → [{"id": "cbeta", "name": {"en": "...", "zh": "..."}}]
```

## Data Source Plugins

Data source plugins handle ingestion. They live in `tools/` as standalone modules.

### Structure

```python
# tools/my_source.py

def ingest_work(work_id: str, base_dir: Path) -> Path:
    """Ingest a single work. Returns path to raw doc."""
    # Fetch content, save to raw/{slug}/index.md with frontmatter:
    # - source: URL
    # - type: "my_source"  (maps to ref plugin ID)
    # - compiled: False
    ...

def learn(batch_size: int, base_dir: Path) -> list[str]:
    """Progressive learning — ingest a batch of new works."""
    ...

def status(base_dir: Path) -> dict:
    """Report learning progress."""
    ...
```

### Register with Worker

In `tools/worker.py`, add to `_task_learn()`:

```python
elif source == "my_source":
    from .my_source import learn
    results = learn(batch_size=batch_size, base_dir=base)
```

### Register with CLI

In `tools/cli.py`, add ingest subcommands.
