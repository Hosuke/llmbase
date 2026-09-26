# Python compile compatibility boundary

The native candidate keeps `llmwiki.compile` and its existing public/private
helper names. Default parsing, section splitting/assembly, document reads,
deduplication/merge decisions, compiled writes, legacy raw-state writes, and
index/backlink rebuilds execute in Rust. The Python layer remains the live
extension host for prompt formatting, model overrides, hooks, and taxonomy
callbacks; it is not presented as a completely removed runtime.

## Pure helpers and live settings

`sanitize_slug`, `_parse_compile_response`, `_parse_article_block`,
`_parse_update_block`, `_split_sections`, and `_assemble_sections` invoke the
local PyO3 pure transformation bridge. These operations do not load library
configuration, create directories, open SQLite, or perform filesystem I/O.
Tagged `===ARTICLE===` / `===UPDATE===` response parsing retains the legacy
list/dict shapes and ordering. This compatibility dialect is separate from the
strict JSON response schema of the new native compile API.

`DEFAULT_SECTION_HEADERS` remains the distinct identity anchor.
`SECTION_HEADERS` assignments are read for each call. Config-driven compile
uses `get_section_headers(cfg)`, so runtime assignment overrides still take
priority over named config profiles. One empty header preserves bare body text.
The shared `LanguageProfile` performs section parsing, assembly, and the strict
greater-than-1.2 merge rule using Unicode scalar counts.

`SYSTEM_PROMPT`, `COMPILE_USER_PROMPT`, and `COMPILE_ARTICLE_FORMAT` are resolved
at each compile invocation. An overridden `compile.chat` actually executes in
Python. The unchanged default uses the native HTTP model facade with metadata
so provider truncation can be rejected. Empty/unparseable/all-unusable output
does not mark a raw source successfully compiled; this intentionally fixes the
legacy empty-success behavior. Already produced articles from an interrupted
multi-article/source batch remain visible and reviewable; no batch-wide atomic
filesystem transaction is claimed.

## Native reads, writes, and exact source retention

`_find_uncompiled` retains Path results and the legacy one-uncompiled-document
per source-directory selection. `_list_existing_concepts` retains its list of
`slug (title)` strings. Native read helpers avoid creating a SQLite index merely
to satisfy those helper contracts. Source metadata, body, attachments, original
hashes, and existing compile receipts are read by Rust.

`_write_article` returns a Path or None and mutates the input slug to its sanitized
form as before. `_merge_into` returns None. Existing title/unknown frontmatter is
retained, tags are unioned only when the merge changes content or source support,
and source refs deduplicate by the legacy plugin/url/work_id/title key. An
unchanged merge leaves the entire existing file byte-for-byte unchanged.
Native alias resolution and the compatibility CJK title/substring rule select
existing targets; ambiguous native alias matches fail rather than guessing.

Every default mutation enters a synchronous shared native writer session and
the source or fixed-metadata write journal. When compilation has an original raw
input, its captured hash/revision guards derived writes, and private/internal
input restrictions propagate into the compiled article. Model slugs cannot
select arbitrary filesystem paths; only scoped configured roots and validated
single-component names reach storage.

The established Python compile contract updates raw `compiled`/`compiled_at`
frontmatter and `compiled_sources.json`. That side effect is retained explicitly
as journaled source revisions. The raw body slice remains byte-for-byte exact,
including CRLF, leading/trailing whitespace, combining marks, and non-BMP text;
the full pre-write original bytes and metadata remain in retained revisions.
The new `Library::compile_source` API has a different, documented contract: it
does not alter any raw bytes and stores compile status separately.

Raw flag changes and the compiled-source log are independently recoverable
journaled writes, not one cross-file atomic batch. A crash between those writes
can require reconciling the log with the retained source revision. The facade
persists each successful source receipt instead of waiting until the full batch
ends. Source changes conflict before status marking.

## Hooks and public projections

`before_compile`, `compiled`, `index_rebuilt`, and `after_compile_batch` still
invoke current Python hook handlers with the established payloads. Current
taxonomy assignment callbacks are also called. Their execution remains an
explicit Python extension boundary, not a fictitious native receipt.

Rebuild operations reuse native maintenance snapshots and projection journals.
Index results retain the legacy list fields, and unresolved backlink names are
preserved. Public static projections deliberately exclude private/internal
sources and recognizable private link targets; this is an intentional privacy
exception to the old all-files projection. Standalone backlinks rebuild affects
that artifact alone. Only a full rebuild updates the full projection manifest.

## Verification

Rust fixtures cover tagged parsing, bare sections, read-without-index behavior,
strict merge thresholds, exact unchanged bytes, source/tag metadata, CJK alias
reuse, raw-body/revision retention, source compare-and-swap, private derived
writes, and public rebuilds with unresolved links. Installed-wheel fixtures trap
legacy Python implementations/frontmatter file reads and execute the actual
native adapter against a temporary HTTP provider, then verify source metadata,
native SQLite revisions, receipts, runtime prompts, and hook calls.

```sh
cargo test -p llmbase-core --test compat_compile
LLMBASE_REQUIRE_NATIVE=1 python -m pytest --import-mode=importlib \
  tests/rust_migration/test_native_compile.py tests/test_compile.py
```

The source checkout's explicit unbuilt-development fallback remains available;
candidate installations fail if their extension is absent or fails. Fixture
verification is not a claim about real-provider article quality.
