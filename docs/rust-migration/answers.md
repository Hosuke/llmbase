# Explicit answer persistence and promotion

Normal native ask and its streaming form are read-only. `Library::save_answer`
explicitly accepts an answer submission and creates a note; promotion produces a
reviewable compiled draft, and `Library::apply_draft` is a separate action.
The HTTP model adapter is the production generation path throughout.

## Domain interface

`save_answer(&AskResponse, &SaveAnswerRequest, include_private)` stores a note and
returns `SavedAnswer`. The request includes the exact library ID, question,
optional title, and visibility, which defaults to private. The boolean access
argument comes from the trusted caller's capabilities, not client JSON.
`saved_answer(id, include_private)` reloads the accepted snapshot and resolves
its original evidence again, returning current stale/deleted indicators.

`promote_answer(&Client, id, &CompileProfile, CancellationToken)` uses the default
conservative promotion prompt. `promote_answer_with_profile` accepts a
`PromotionProfile` containing compile settings and effective system/user prompt
overrides. Native defaults use English without a domain persona; configured
language profiles remain authoritative. Required unsupported callback capabilities
fail explicitly through the compile profile validator.

`PromotionReport` includes a durable job ID, boolean promotion decision, reason,
optional draft ID, and cache flag. `promote: true` means a draft exists; it never
means the draft was accepted. An explicit negative decision is a successful judge
result with a reason and no draft.

## Evidence, visibility, and provenance

Save requires exact request/response/current library agreement. Every candidate
is verified through `resolve_evidence`, including retained revision/hash, exact
UTF-8 byte offsets and quote, and passage identity. Context items must be unique
labeled candidates. Used evidence must match a context item and an actual answer
marker. A marker is a citation claim by the model, not proof of entailment.

Visibility must be at least as restrictive as every candidate, including uncited
context sent to the model. Both the current source and the referenced historical
revision count; `private: true` takes precedence over `visibility: public`.
Changing a source to public cannot declassify its private historical revision.
Cross-library or inaccessible citations are rejected before writing.
Current source guards are captured before validation and checked again under the
writer lock, including live disk hashes. Concurrent indexed changes and unindexed
privacy/content edits cannot slip between validation and note creation.

The note retains the submitted generation text/model/usage/finish/truncation,
request/session identity, retrieved candidates, exact original evidence, and
retrieval degradation. Its provenance is explicitly
`accepted_submission_not_provider_attestation`: accepting user-supplied JSON is
not a cryptographic receipt proving a particular provider produced that answer.
Credentials are never included. Note paths are generated from a deterministic
content identity; neither model output nor user titles select filesystem paths.

Promotion retains the note's original evidence snapshot as source-support
metadata, adds freshly resolved stale/deleted status and actual promotion
provider/model/usage/finish metadata, and carries the stricter current restriction
into the proposed article. The note and raw sources remain unchanged. Current
evidence guards are captured before final validation and model execution. The
draft and its sealed guard set are persisted in the same SQLite transaction.

## Persistence and failure behavior

Accepted notes are written through the shared atomic file/SQLite journal. The
`saved_answers` table binds their exact source revision and hash to the accepted
payload. Repeated identical submissions reuse that identity. A note edit,
including an unindexed disk edit, conflicts with retrieval/promotion until a new
answer revision is explicitly accepted. The note's full provenance frontmatter
also preserves content if a database receipt must be recovered.

Promotion uses the durable fenced job implementation and renews its lease during
generation. Cancellation/timeout/malformed output produces cancelled/failed
status, never a successful empty article. Empty or provider-truncated saved
answers cannot be promoted. The judge must return strict JSON with a boolean
decision and bounded nonempty reason; a positive decision requires the same
validated article schema as compilation, including a safe single-component slug.

Successful judge results are cached by library/answer/note hash, fresh evidence
status, visibility, full effective profile/prompts, endpoint, and model. Drafts
reuse compilation's merge, source support, visibility, and review machinery.
Apply verifies the saved note and all captured evidence revisions/live disk
hashes under the writer lock, then uses the reviewed target compare-and-swap
hash. Changing an original source to private after generation therefore blocks
apply, even before indexing the edit. A missing promotion guard set fails closed
on both cached-result replay and apply. A changed note or edited target cannot
be silently overwritten.

`saved_answers`, `promotion_receipts`, compile drafts and their guard sets, jobs, and write intents are
authoritative workflow records and belong in backup/restore, not disposable index
cleanup. A process failure between a paid response and durable receipt may need a
new provider call; exactly-once external billing is not claimed.

## Verification

The offline suite uses real ephemeral HTTP endpoints through `Client.complete`.
The latest focused run exited 0 with 12/12 answer tests in 0.28 s and 10/10 compile tests
in 0.14 s (`/tmp/llmbase-promotion-guards.log`, `/tmp/llmbase-promotion-guards.exit`).
It checks explicit save/reopen/idempotence, exact evidence and scope failures,
private historical revisions and uncited context, stale evidence, unchanged raw
files, review-before-apply, positive/negative decisions, malformed/truncated/path
output, source guards, and preserved promotion metadata.
Cancellation during a live provider request and provider timeout also verify
persisted cancelled/failed job states and no compiled-file side effect.

```sh
cargo test -p llmbase-core --test answers --test compile
```

Fixture results do not establish real-provider judgment or answer quality.
