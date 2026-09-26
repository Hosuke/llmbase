# Native lexical/vector retrieval

The lexical baseline remains `Library::search` over original raw, compiled, or
note passages. `vector::Library` extension methods add real configurable HTTP
embeddings, durable cache, atomic index generations, and cosine/lexical reciprocal
rank fusion (RRF). No lexical-only response is labelled hybrid.

## API and identity

```rust
use llmbase_core::vector::EmbeddingSpec;
use llmbase_model::CancellationToken;

// client: llmbase_model::Client from trusted operator configuration
// library: llmbase_core::Library already synced from the source files
// let spec = EmbeddingSpec::new(&client, "operator-vector-model", "revision-1", 768);
// library.build_vectors(&client, &spec, CancellationToken::new()).await?;
// let hits = library.search_hybrid("question", 10, Some("raw"), false,
//     Some(&client), CancellationToken::new()).await?;
```

`EmbeddingSpec` records canonical credential-free provider endpoint, requested
model, operator-declared model revision, and dimension. The endpoint must equal
`Client::endpoint_identity()`. A model alias can change its implementation without
changing its name; the operator must update `revision` in that case. The adapter
does not claim it can independently verify provider weights or alias stability.

Cache primary key: `(library_id, hash(full serialized spec), hash(exact input
bytes))`. Float vectors use little-endian f32 bytes and validated dimensions,
finite values, and nonzero norm. Identical text within one library/spec can share
cached vectors. Different libraries, model revisions, dimensions, or provider
identities cannot share cache entries. Embedding input is original passage text;
retrieval normalization never overwrites original bytes.

Generation identity adds a hash of the entire active passage snapshot, including
passage/source IDs, source revision and hash, parser/chunker fingerprint, offsets,
source kind, visibility, and original quote. This ensures source, parser, or
visibility changes invalidate the generation even if some exact input vectors
can safely be reused. Requested output dimensions are sent to the real adapter.

## Durable state and activation

Four SQLite tables hold derived state:

| Table | Purpose and authority |
| --- | --- |
| `embedding_cache` | Successfully validated and persisted provider vectors; reusable derived data |
| `vector_generations` | Full spec, snapshot hash, passage count, building/ready/failed state |
| `vector_members` | Exact passage membership and vector cache keys for a generation |
| `vector_active` | One atomically selected ready generation per library |

Build reads a complete source snapshot, finds durable cached inputs, and calls the
HTTP embedding adapter in batches of at most 32 unique uncached inputs. Each
successful batch is validated and committed before the next provider call. On
failure/cancellation, previously committed cache batches survive and the former
active generation stays selected. The same build can resume by reusing those
batches. Source files are not modified.

Activation runs in one SQLite immediate transaction. It re-reads the complete
source snapshot, rejects concurrent index changes, validates every cache vector
and every membership row, marks the generation ready, and switches the active
pointer. Readers never see half the generation. Concurrent builders can repeat a
provider call, but cache and membership primary keys prevent duplicate persistent
entries. A provider success followed by a crash before cache commit can be billed
again on resume; no cross-provider exactly-once claim is made.

Generation/cache rows are derived data and can be rebuilt from retained source
revisions and model access. Retaining them in a database backup saves provider
cost; they do not replace source/content/revision/job backup obligations.

## Search and isolation

`search_hybrid(query, top_k, kind, include_private, Option<&Client>, cancel)` returns
the existing `SearchResult`. Missing client, absent generation, or a stale source
snapshot returns lexical results with an explicit `degradation` reason. A
configured provider failure remains a typed model error. Invalid endpoint/spec
combinations are rejected instead of mixing incompatible vectors.

For a current generation, query embedding uses the same model spec and cache.
Provider I/O occurs before the local SQLite read transaction. The transaction
then revalidates active generation/source snapshot and computes cosine scores.
If an index changed during the query's provider call, the result explicitly
degrades to current lexical results; it never compares the old query vector with
an incompatible newly active model.

Up to 100 lexical and 100 semantic candidates are independently ranked; passage
scores combine as `sum(1 / (60 + one_based_rank))`. The final list is capped at
`min(top_k, 100)` with deterministic passage-ID tie-breaking. Semantic scoring
scans the selected generation, a deliberately simple exact-vector baseline.
Generation limits are 100,000 passages, 128 MiB original passage text, and 32,768
dimensions. These are explicit capacity limits, not measured throughput claims.
No ANN, query expansion, or reranker is hidden behind the hybrid label.

Both channels filter library scope, active passages, deleted sources, source kind,
and visibility. Public calls exclude private and internal sources. Stale vectors
remain a reusable cache only: deleted/inactive passages cannot appear in a search.
Every hit carries the existing original-byte evidence schema and resolves through
`Library::resolve_evidence`.

## Verification and remaining evaluation

`crates/llmbase-core/tests/vector.rs` uses the production HTTP adapter against an
ephemeral deterministic fixture provider. It verifies persistent reuse after
reopening the database, incremental edits, model/revision/dimension isolation,
failed second-batch resume, cancellation, concurrent source writes before
activation, actual semantic candidates in RRF output, query-vector reuse,
private/internal/library boundaries, deletion, and explicit lexical degradation.

Initial combined core/vector/jobs run exited 0: 7 vector integration tests passed.
An additional review added a transactional fast-path recheck for active-generation
reuse and preserved ready state during concurrent generation creation. Focused
follow-up focused test exited 0: 7/7 passed in 0.16 s
(`/tmp/llmbase-vector-review.log`). Initial clippy found one model stylistic lint,
which was fixed; the next attempt was stopped by `--locked` while other agents
added dependencies, so final clippy remains a workspace verification gate. Re-run:

```sh
cargo test --locked -p llmbase-core --test vector
cargo clippy --locked -p llmbase-core -p llmbase-model --all-targets -- -D warnings
```

The fixture vectors prove vector computation, fusion, cache, persistence, and
isolation mechanics. They **do not measure semantic quality**. Real-provider
semantic/hybrid Recall@k, MRR/nDCG, lexical ablation, cold/warm p50/p95, RSS, index
size, and provider cost remain **unverified** until the separate fixed evaluation
suite is run. No improvement over lexical quality or latency is claimed here.
