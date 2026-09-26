# Native ingestion and parser processes

`ingest file` and `ingest dir` read user-selected local files in the CLI adapter.
Text bytes pass to Application `ingest_text` as `{filename, content,
expected_hash?}`. The library creates a safe per-source directory and journals
the accepted original text without rewriting frontmatter or line endings.
Non-Markdown UTF-8 text uses `index.md` so it can be compiled directly. The
source revision retains the exact input bytes. Equal bytes are an idempotent
replay; a different existing target requires its reviewed hash.

Remote `kb_ingest` accepts a public HTTP URL or an existing file confined to the
configured library base. It cannot read arbitrary server files. HTTP upload
sends bytes/filename; the application does not interpret that filename as a
server path. Local directory ingestion is one-level, sorted, and bounded to1000
supported files. Earlier accepted files remain visible in a partial result if a
later file fails.

URL fetching uses the native DNS/socket/redirect policy in `python-fetch.md`.
Plain UTF-8 text is saved with acquisition metadata and its byte-exact original.
HTML and PDF parsing are explicit processes, using the Python package's parser
module when configured. These processes perform parsing only: no network,
filesystem persistence, indexing, model call or job management.

```yaml
plugins:
  process_adapters:
    html:
      command: [/absolute/candidate/bin/python, -m, llmwiki.process_adapter, html]
      timeout_seconds: 60
      max_output_bytes: 16777216
    pdf:
      command: [/absolute/candidate/bin/python, -m, llmwiki.process_adapter, pdf]
      timeout_seconds: 60
      max_output_bytes: 16777216
```

Install the compatibility package with its `pdf` extra for PDF parsing. Missing
parser configuration/dependencies are explicit capability/process failures.
`ingest browse` requires a configured browser adapter; the parser above does not
pretend to execute browser JavaScript. Existing corpus connectors remain
available in the Python extension host.

The process protocol adds optional `assets` alongside required `documents`:

```json
{
  "protocol_version": 1,
  "assets": [{"path": "paper/source.original", "content_bytes": [0, 255]}],
  "documents": [{"path": "paper/part-1.md", "content": "Parsed text"}]
}
```

Opaque original filenames end in `.original`, are bounded to32MiB each, and
remain outside the text index. Application `ingest_asset` records their exact
bytes and compare-and-swap history in durable `write_intents`; recovery applies
asset-only transactions without forcing an unrelated Markdown index rebuild.
Their files and database history belong in backups. Public text-read endpoints
do not serve arbitrary original assets.

Parser metadata records `original_file` relative to the parsed document's
directory, `original_sha256`, and `original_media_type`. PDF chunks share one
original rather than duplicating it. Parser output is bounded to64 documents and
64 originals, plus the configured total output byte limit. JSON byte arrays count
toward process input/output bounds; a binary's maximum effective size may be
smaller than the per-asset bound. Image links in the basic HTML process are
resolved to absolute URLs; this parser does not download image assets. The
legacy Python URL connector retains its separately tested image-download path.

Acceptance is journaled per file. An original accepted before a later document
conflict remains stored, and the process job reports partial; this is not an
all-files filesystem transaction. Failed/empty parsing is never reported as a
successful empty ingestion. Original asset hashes establish byte identity;
parsed PDF text is a transcription layer, not a byte-for-byte representation of
the original PDF.
