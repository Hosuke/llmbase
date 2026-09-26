# Native compilation, review, and apply

`Library::compile_source` executes the native HTTP generation adapter and saves
reviewable drafts in SQLite. `Library::apply_draft` separately applies exactly
the reviewed bytes through the shared compare-and-swap write-intent journal.
Compilation never rewrites a raw document or places a compile-status field into
its frontmatter. Newly accepted compiled files become retained knowledge content,
not disposable vector/index cache.

## Interface

```rust
// profile = CompileProfile::from_config(&library.config.settings)?;
// Python facade instead passes its current effective serialized override.
// report = library.compile_source(&client, &source_id, &profile, cancel).await?;
// draft = library.draft(&report.draft_ids[0])?;
// Review draft.content and draft.base_hash before calling explicit apply:
// accepted = library.apply_draft(&draft.id, draft.base_hash.as_deref())?;
```

`CompileReport` includes job_id, draft_ids, persisted JobStatus, cached flag, and
per-article validation errors. Fully successful receipts reuse their persisted
drafts without calling the model again. Partial receipts can be retried; existing
deterministically identified drafts are preserved while valid missing articles
can be added. The cache identity includes library/source identity, original
revision/hash, complete effective profile/prompts, configured endpoint, and model.
Operators can change profile revision to intentionally request regeneration.

`CompileDraft` contains source revision/hash, target relative path, reviewed
target base hash, complete candidate file bytes, candidate content hash, effective
profile, producing job ID, and draft/applied status. `draft()` is a privileged
core primitive; transports must apply the same library/private-read capability
gate as other nonpublic content reads.

## Language, prompts, and extensions

Native unconfigured libraries default to one English bare-body section. Existing
`languages.default` / named profiles remain authoritative when configured, so the
repository's trilingual profile is retained. `LanguageProfile` preserves section
codes, keys, headers, extra display fields, views, and default languages. One
section with an empty header preserves a bare-body article; no language heading
is invented. English is an engineering default, not a rule to translate original
sources or discard CJK/non-BMP content.

A passed `CompileProfile` is effective for that invocation and is not globally
cached. It contains language profile, system/user prompts, generation budget,
optional model, profile revision, and required plugin capabilities. The Python
facade must resolve current runtime overrides before passing it. Prompt variables
are substituted once from the original template, so source text containing a
placeholder is never recursively interpreted as template code.

Profiles declaring required callbacks/plugin capabilities fail explicitly in the
standalone native path. The Python extension host must execute real callbacks and
pass supported settings/results through its documented boundary; this module
does not pretend to execute Python callables or silently ignore a requirement.

## Validation and merge

Provider output must be strict JSON with 1..100 `articles`. Each article needs
slug, title, summary, tags, content, and optional aliases. Empty, malformed,
all-invalid, and provider-truncated output fail; no empty-success compile record
is created. Slugs may contain Unicode but must be a safe single filename
component, never a path, control sequence, or traversal segment.

Exact compiled slug or normalized explicit alias/title match resolves an existing
target. Ambiguous matches are conflicts, not a guess about which existing article
to overwrite. The native path does not infer identity from approximate CJK title
substrings; any legacy facade-specific heuristic requires its own parity adapter.
Language sections merge using the existing rule: replace nonempty old content
only if the new content is strictly more than 1.2 times as long, measured in
Unicode scalar values; add missing sections. Equal 1.2 length keeps the old
section. Existing preamble and unaffected sections remain.

Unknown target frontmatter is preserved. Tags/aliases are unioned, source support
records carry original source metadata plus library/source/revision/hash, created
time is retained, and updated time is recorded. Original sources and their
supporting revisions remain separate from generated explanation. Private/internal
source visibility escalates a merged target's restriction; merging private text
into an existing public article cannot publish the private content.

## Jobs, failure, and recovery

Compilation enqueues its own durable job and claims that exact ID through the
shared fenced-lease implementation. It cannot steal another queued operation.
The lease renews every 15 seconds during generation and between article drafts.
Model timeout/cancellation/error becomes failed/cancelled status; a batch with
some valid drafts and some invalid articles persists partial state and the good
review artifacts. Successful draft generation is job success, not implicit
authorization to apply those drafts.

Each draft is durable before its ID is returned. Draft and receipt tables are
authoritative review/workflow state and must be included in backup/restore. They
are not safe to drop with a derived index. Source hash/profile receipts avoid
unnecessary successful calls; a crash after provider success but before draft
persistence can require another paid call. No exactly-once provider claim is made.

Apply validates the supplied expected hash against the reviewed base and calls
`write_source_guarded("compiled", ...)`. Under the shared writer lock it checks
both the indexed source revision/hash and the current raw file bytes before
recording the write intent. Unindexed raw edits, human target edits, and source
revision changes conflict. The journal handles file
and SQLite recovery. A retry recognizes identical accepted file bytes and repairs
an interrupted draft-status receipt; changed accepted bytes are never overwritten
as a supposed retry. Each draft apply is atomic through the journal; a multi-draft
batch is a sequence of explicitly reviewable applies, not a claimed filesystem
transaction spanning all articles.

## Verification

Existing descriptions are limited by the raw source's visibility: public sources
receive public context, internal sources receive public/internal context, and
private sources may receive all authorized context. The prompt includes at most
100 descriptions. Each resulting draft seals those consulted source revisions
and hashes; its own target uses the existing reviewed target hash. Publication
conflicts if a consulted source changes or becomes private. The cache key includes
the context policy version. A same-batch change to another draft's consulted
article can conservatively produce a partial result rather than silently reuse
changed context.

The latest focused fixture run exited 0: 13/13 integration tests passed in 0.23 s
(`/tmp/llmbase-workflow-privacy.log`, `/tmp/llmbase-workflow-privacy.exit`). Tests execute
the real model HTTP adapter and cover source->draft->review->apply->evidence,
raw-byte invariance, reopen/dedup, exact job claiming, malformed/empty/truncated
failure states, partial retry, target/source conflicts, alias and unknown metadata
preservation, original source support, private-merge visibility, English native
default, configured trilingual sections, non-BMP scalar length, and bare-body
wenyan override. Application integration checks each required capability and
cross-library denial before any provider call, config-derived effective language
settings, and explicit application-level draft apply. The run includes creation/
update timestamps and the guarded raw-file checks.
The added privacy tests verify excluded private/internal descriptions, excluded
private aliases, and rejection when a consulted description becomes private.

```sh
cargo test -p llmbase-core --test compile
```

Real-provider compilation quality and downstream Python callback/hook parity are
separate validation gates; this fixture suite does not establish those results.
