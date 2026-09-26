# Native entity generation

`Library::generate_entities(client, request, access_private, cancel)` generates
people, events, and places through the actual Rust HTTP model adapter. It shares
the maintenance generation job leases, heartbeat, cancellation, source snapshot
validation, and recoverable fixed-artifact publication machinery. Credentials
remain inside `Client` and never enter a profile, job argument, or artifact.

The operation honors `entities.enabled`. Without that setting it returns the
current entity projection with `status: disabled`, makes no provider request, and
does not claim extraction. An authorized explicit `force` overrides the setting.
No articles returns `no_changes`, with `generated: false` and no fabricated model
success. Required Python/plugin capabilities fail explicitly before provider use.

`EntitiesRequest` exposes system/user prompt templates, model/budgets, batch size,
and article formatting. The default `auto` format preserves the legacy choice:
summaries for up to 80 articles, titles plus up to three tags for larger libraries.
`summary` and `tags` are explicit alternatives. Prompts are divided into bounded
batches, with a separate hard byte limit for each prompt. A single article that
cannot fit fails rather than silently dropping source metadata.

Responses require the three category arrays, bounded text fields, at least one
name or original-script name per entity, and supported article references from
the supplied batch. Coordinates must be finite latitude/longitude within their
respective ranges, or null. Malformed JSON, empty/truncated content, an overall
empty extraction, unknown references, and invalid coordinates fail without
replacing a previous artifact. A failure in a later batch likewise leaves the
previous accepted artifact intact and records a failed job.

Deduplication preserves the legacy transitive rule: lowercase `name` or exact
original-script `name_local` joins entities. It unions article references and
keeps longer dates/date/role/description text by Unicode scalar count. The first
entity's other fields remain authoritative. Original CJK and supplementary-plane
characters are preserved. Entity references record supplied support identifiers;
the implementation does not establish statement-level semantic entailment.

Default public extraction reads only current public articles and writes
`entities.json`. Private-input extraction requires trusted private access and
writes the separate `entities.private.json`. Public readers never load the
private artifact; authorized `Library::entities(true)` prefers it. The artifact
records consulted source revisions/hashes and a snapshot fingerprint. Publication
checks all current source bytes and the previous artifact under the writer lock.
An unindexed privacy edit, source addition/deletion/change, or concurrent accepted
artifact edit produces a conflict instead of overwriting new state.

Public reads also recheck every consulted source, including sources absent from
the generated entity's `articles` references. Revoking one such source withholds
the whole aggregate. A live, unindexed private edit is hidden by the public source
guard (`NotFound`); an indexed revocation produces an aggregate `Conflict`.
This prevents uncited private context from remaining visible through an entity
description that references only another, still-public article.

The existing Python `ENTITY_ARTICLE_FORMATTER`, prompt globals, hooks, and helper
functions are unchanged. They remain executable Python extension semantics; this
native API does not silently serialize or relabel a Python callable. A future
facade host must invoke a supplied formatter explicitly or declare the capability
unsupported. Native applications can use the serializable formatting/profile
fields directly without Python.

The fixture suite uses ephemeral loopback HTTP providers and checks real request
prompts/metadata, Unicode entities, all three schemas, transitive dedup across
batches, public/private artifacts and readers, disabled/empty behavior, malformed
and truncated responses, coordinate/reference rejection, cancellation, callback
capability failure, concurrent source/artifact edits, and failure after a completed
batch. These fixtures demonstrate execution and validation, not extraction quality
on a paid provider. All eight original entity cases passed in the v13 integration
batch. The ninth case, revoking an uncited consulted source, passed its focused
rerun (`.rust-migration-runs/entities-revocation-v2.log`, exit 0).
