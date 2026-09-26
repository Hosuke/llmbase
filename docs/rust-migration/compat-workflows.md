# Native accepted workflow hosts

`compat_workflows::query(config, client, args, cancel, runtime)` hosts the shared
compatibility query state machine with real `llmbase_model::Client::complete`
calls. It needs no Python interpreter, callback process, or mock provider.
Applications run it in a blocking storage worker and pass their Tokio runtime
handle. Model credentials remain inside `Client`; request arguments do not
configure arbitrary provider endpoints.

The host supports basic/deep retrieval, selector, answer, optional note filing,
and optional accepted promotion. Tone/format settings, configured language
profiles, and bounded promotion examples are assembled natively. Configured
`query.system_prompt`, `query.promote_system_prompt`, and
`query.tone_instructions` provide serializable customization. A declared required
callback capability fails before filesystem/model work. English is the native
default; configured Chinese/wenyan profiles preserve original evidence text.

The result retains the compatibility value envelope and native sequence receipt.
Its model boundary names `llmbase_model::Client::complete`; each actual call adds
usage, finish reason, truncation, model, stage, and attempt metadata. Cancellation
is passed to each provider request and checked between stages. Existing query
write guards and public source filtering apply unchanged.

`query_with_clients` additionally accepts the original operator client and a
scoped answer client. Per-request model/key overrides affect only selector and
answer calls; the promotion judge retains the original client. Applications
authorize first, remove credentials from operation arguments, and create the
scoped client with `Client::scoped_overrides`. That method permits no endpoint
override and clears operator fallback models when a model is explicitly selected.

`compat_workflows::compile` accepts `mode: new|all`, an optional batch size, and an
optional serialized `CompileProfile`. It selects raw sources through shared
indexed storage, generates durable review artifacts with the native
strict JSON compiler, and explicitly applies each accepted draft with its exact
reviewed target hash. Only a fully accepted source receives the legacy compiled
flag, timestamp, and source-key receipt. Those metadata writes retain the exact
raw body and the prior complete source revision. The structured provider dialect
is the native compiler's `articles` schema; the Python callback facade retains
its separate tagged response dialect.

The response includes `articles_created`, `articles`, `sources_processed`, job
IDs, errors, and a status. Article paths are relative to the library base (or a
filename for an external configured root). Empty selection reports `no_changes`.
Invalid/empty extraction reports failure without marking the source compiled.
Accepted articles before a later failure remain present and are reported as
`partial`. Durable source jobs and draft statuses make intermediate artifacts
inspectable. No atomic transaction across an entire multi-source batch is claimed.

Native `all` resets compiled flags and bypasses existing source-key receipts, so
it actually requests fresh extraction. This deliberately corrects the old
Python behavior, where retained `compiled_sources.json` receipts could suppress
the requested recompilation. The Python compatibility facade retains its legacy
behavior; the Rust accepted workflow makes the corrected force behavior explicit.

Native selection includes every indexed raw Markdown source, sorted by relative
path: top-level files and multiple extracted chunks in one directory are eligible.
Preserved `.original` assets are excluded by indexing. `new` filters each file's
compiled metadata; the native source/revision cache remains authoritative for
retries. A URL-wide `compiled_sources.json` receipt does not suppress another
uncompiled chunk from the same original document. The host still writes those
legacy receipts for compatibility. `all` resets every indexed raw source through
the shared guarded metadata writer, including top-level files. The existing
batch limit remains in effect. These native corrections leave Python's historical
directory selection and source-key skip behavior unchanged.

Existing article descriptions are restricted to the raw source's visibility:
public sources receive public descriptions, internal sources receive public or
internal descriptions, and private sources may receive all authorized context.
At most 100 descriptions enter the prompt. Their revisions/hashes are sealed
with the draft and rechecked at apply; the target itself is covered by its
reviewed target hash. A consulted article becoming private blocks publication.
Updating another draft's consulted article in the same batch can conservatively
produce a conflict and partial result; safety takes priority over an automatic
retry based on changed context. The context policy changes the compile cache key.

The host is an application building block: transports must authorize model,
read, write, and private-source access as appropriate before calling it. Unlike
public query retrieval, compilation may merge private existing articles and
therefore requires the same private capability as the native compiler.

The loopback HTTP tests exercise native selector/answer/judge calls, accepted
fileback and promotion, configured language and Marp prompts, source-to-draft-to-
apply-to-mark behavior, exact CRLF raw body retention, idempotent skip, partial
failure, empty extraction, and pre-cancelled/unsupported-callback zero-call gates.
An additional fixture covers same-directory chunks sharing an existing source-key
receipt, a previously compiled top-level file, idempotent `new`, complete `all`
selection within its batch limit, and unchanged original PDF/Markdown assets.
