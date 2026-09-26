# Native library architecture and authority

Implementation is a modular monolith. `llmbase-core::Application` owns scope and capability checks; the native CLI, Axum HTTP surfaces, official `rmcp` stdio server and PyO3 facade dispatch into it. `llmbase-model` owns HTTP generation/embedding protocols, bounded response parsing, deadlines, usage, retries and cancellation. The Python extension is a separate workspace crate: neither the native CLI nor core links to an interpreter.

English is the engineering and new native answer default. Language sections, prompts, classification and text policies are runtime profile/extension inputs. Existing configured trilingual profiles and legacy Python override precedence remain obligations. Language is never inferred from a source's filename as an authorization or storage policy.

## Authority and retention

| Data | Authority | Backup/rebuild policy |
|---|---|---|
| Original source/attachment bytes and accepted concepts/notes | Configured files | Copy byte-for-byte; never rewrite originals during parsing/normalization/compilation |
| Captured revisions, tombstones, write intents, jobs/events, drafts/acceptance and migration receipts | SQLite and durable receipt files | Mandatory backup; cannot be recovered merely by rescanning latest files |
| Human taxonomy/overrides/relations and existing metadata files | Existing configured files, with future explicit records for new relations | Copy and preserve, never treat as model cache |
| Active passages/FTS/alias projection | Derived SQLite tables | Rebuild from captured/file content using recorded parser/chunker fingerprint |
| Embedding cache and generations | Derived, durably reusable SQLite tables | Rebuildable but successful persisted vectors reused; model/provider/revision/dimension must match |
| Existing `.pipeline` JSONL and `.chunk_cache` | Existing extension workflow contracts | Preserve exact files during copy/backup; native ledger-to-legacy integration remains pending |

Source IDs are durable within a bound `library_id`; aliases never search a different database. Same-name ambiguity is an error requiring a source ID. The one-library-per-database binding rejects opening an existing database under another library ID. This binding is distinct from caller access grants.

## Evidence

Evidence stores library/source/revision/content hash/passage ID, exact quotation, original UTF-8 **byte** `[start,end)` offsets into the entire original file (including frontmatter), plus available public source URL and heading locator. Unicode normalization/OpenCC affects only indexed representations. Evidence checks the captured original, never a normalized string; old revisions remain addressable after changes/deletion and explicitly report stale/deleted state. Scalar/byte conversion helpers reject invalid boundaries.

Search candidates, context evidence and model citation markers are separate fields. Citation resolution proves source location, not semantic entailment of an answer. `[E1]` marker use is recorded; unknown markers are surfaced. Neither consulted sources nor heartbeat/progress events count as generated answer tokens.

## Writes, recovery and concurrency

Cooperative file writers use the stable `wiki/_meta/llmbase.writer.lock` (or configured metadata root) OS advisory lock. Lock ownership is kernel state, not PID-file contents; process death releases it and PID reuse cannot inherit it. Candidate Python write paths must join this protocol before write parity is claimed. Old unmodified Python writers do not join and must not concurrently mutate a migrated library.

An accepted file mutation first commits a write intent containing operation, expected original hash and accepted bytes to SQLite. It then stages/fsyncs a uniquely created file beside the destination, atomically renames and fsyncs the parent. Delete/rename verifies the expected hash; resume accepts already-installed exact bytes. A transaction publishes the full source/index snapshot; marking the intent applied happens afterwards. Reopen replays pending intents, so a crash on either side of file installation is idempotent. An unrelated external edit causes an explicit conflict and is preserved. Conflict-resolution operator commands and destructive crash testing remain to be completed.

FTS updates and tombstones publish in one transaction. Old captured revisions/passages remain for evidence but deleted sources disappear from active search. Changing the parser/chunker fingerprint rebuilds affected derived passages without inventing a new original revision when bytes did not change.

Vector generation activation compares a complete source/passage snapshot in an immediate transaction. A changed source aborts activation; readers do not mix generations. Successful batches persist cache entries before the next batch. A crash after a provider response but before local cache commit may repeat that request: there is **no cross-provider exactly-once guarantee**. Search over a stale/missing/incompatible vector generation explicitly degrades to lexical.

## Jobs and model boundaries

Jobs persist queued/running/succeeded/failed/partial/cancelled/interrupted state, attempts, retry limit, availability time, lease expiry and events. A random lease token fences old workers independently of their PID. Expired leases become interrupted; retry is explicit with bounded exponential backoff. Cancellation invalidates the lease. Credential-like argument keys are rejected; model credentials are supplied at execution time, never stored in job configuration.

The HTTP adapter consumes real provider deltas and supports timeout, cancellation and stream truncation. Midstream failures terminate with an error and do not retry an already-partially-delivered answer. Configured non-streaming providers report unavailable capability. Fixture HTTP servers are test infrastructure; the production adapter is not a stub.

## Migration and rollback

`migration::inspect` only reads the source and reports all mapped file hashes/bytes, native DB files, unknown metadata keys, unreadable text and concept identity conflicts. Copy-first migration creates an owned destination with a resumable receipt, preserves original configuration separately, rewrites only destination paths, snapshots SQLite with its backup API, indexes the destination and verifies hashes/integrity. Original source changes during copying reject acceptance.

Backups include deleted revisions, jobs, drafts and previous receipts, not just active Markdown. Immutable snapshot verification rejects altered files or an uncheckpointed WAL. Restore writes a fresh target. Rollback verifies the original retained library against its recorded manifest; it does not overwrite later edits or switch production. Legacy Python readability and full CLI rehearsal are separate gates still pending.

## Present limits

This describes current mechanisms, not completion of the migration goal. The parity matrix, candidate Siwen core-path tracing, all legacy maintenance operations/hooks, JSONL interoperation, full readonly/runtime policies, Linux release end-to-end run and final fault/security/quality gates remain open. Real-provider semantic quality is unverified; deterministic embeddings prove only vector arithmetic/fusion/cache/scope behavior.
