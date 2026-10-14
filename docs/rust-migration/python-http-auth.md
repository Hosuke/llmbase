# Flask principals and the visitor-cookie correction

The Python Flask factories remain extension adapters. They now use a shared
explicit principal with `read`, `private`, `model`, `write` and `operator` scopes.
`LLMBASE_API_SECRET` is an operator credential and grants all scopes when supplied
as an actual `Authorization: Bearer ...` header. Reading the SPA may still mint
the established `llmbase_auth` visitor cookie, but that cookie grants no model,
write, private or operator authority. The helper `derive_session_token` remains
callable; its output is no longer an authorization token.

Both factories fail closed for model/write access with an empty secret. An
embedding host may explicitly set
`app.config['llmbase']['allow_unauthenticated_local'] = True`, or set
`LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED=1` before creating the app. This exception
applies only to loopback request peers without proxy-forwarding headers and only
while the operator secret is empty. It is not enabled by `TESTING` or
`test_client()`. Hosts using this development exception must bind to loopback;
it is not a public deployment mode.

For narrower credentials an embedding host can populate the in-memory mapping
`app.config['llmbase']['token_scopes'] = {token: ('read', 'model')}`. No token is
included in the request principal or error body. Unauthenticated reads of public
data remain available. Missing authority returns 401; a recognized token lacking
required scopes returns 403. Credential-bearing responses are `private,
no-store`, with `Vary: Authorization, Cookie`.

All `/api/ask` model calls require `model`, including default-model requests.
The legacy default `file_back=True` additionally requires `write`, as does
promotion. Compile, entity/xici generation and automatic lint repair require
both model and write scopes. The agent factory applies the same checks to legacy
routes and the generic operation dispatcher. Unknown dynamic handlers require
operator authority unless the host supplies explicit `Operation.required_scopes`
metadata. Their `category='read'` or `writes=False` flags alone are insufficient.
Known argument-driven writes still add `write` even with explicit scope metadata.
HTTP ingestion of arbitrary server-local paths additionally requires operator
authority; a write-only grant does not authorize reading server files. Uploaded
request bytes and public-web URLs remain separate write-authorized paths.
Existing live registries, decorators, Flask view replacement and before/after
hooks remain available; arbitrary replacement handlers retain responsibility for
their declared capability and data boundaries.

The default legacy read handlers now obtain a current native filesystem
visibility snapshot without indexing or creating a database. `private: true` and
any non-public `visibility` hide a source from visitor lists, direct article/raw
reads, search, aliases, backlinks and whole-wiki export. The visibility snapshot
reads frontmatter instead of entire Markdown bodies. It retains a 20,000-file
limit, a 1 MiB limit per header, and a 64 MiB total read budget (including failed
parses and at most 1 KiB of buffered read-ahead per file). A body larger than
64 MiB is therefore not itself a visibility error. Headerless files need only a
4–5-byte probe. Framing matches `Document::parse` exactly; this is not a new
normalization of BOM, whitespace or Python's different delimiter rules.

Unreadable, malformed, unterminated or oversized headers are not classified
public; symlink/root violations and unavailable snapshots fail closed. Body
UTF-8 validity and full-document read limits belong to the downstream content
reader, not this visibility projection. Private reads still require the separate
`private` scope. Snapshot entries expose `metadata_hash` (SHA-256 of the framed
header, or the empty byte string for a headerless document), replacing their old
full-document `content_hash`. The public lite index ETag includes these current
metadata hashes, so same-size/mtime privacy edits invalidate old public 304s
without index rebuilding. Body-only edits need not invalidate the lite response,
whose titles and slugs come from `index.json`.

The full web, agent and search-only Flask factories scope default retrieval
before tokenization and ranking. A context-local public scope becomes
`public_only=true` in both native snapshot and ranking requests. Private documents
cannot influence IDF, consume result slots or reach a live Python tokenizer.
Nested scopes cannot widen an existing public scope. Programmatic search keeps
the legacy all-library default, and Rust receipts distinguish `visibility: public`
from `visibility: all`. Arbitrary replacement handlers remain trusted extensions.
The search-only HTML client renders returned titles/summaries/snippets with DOM
`textContent`; ingested text is not interpolated into `innerHTML`.

Persisted legacy taxonomy, entity, xici, health and export aggregates lack
per-field source provenance. When the library contains non-public documents,
these opaque aggregates require a private principal. This is a deliberate
availability restriction rather than an assertion that an old mixed aggregate
has been safely filtered. The native indexed HTTP service has separate scoped
projection logic. Custom Siwen routes remain downstream extension code. The v10
offline bake probe uncovered a separate compatibility blocker: `/api/trails`
required private authority even for an absent artifact, while Siwen's fixed bake
manifest requests it anonymously. In v11, native reads return a public
`{"trails":[]}` only when the artifact is absent or its actual trail array is
empty. Extra opaque fields are not returned. Nonempty trails still require
private authority; malformed JSON is an error rather than a fabricated empty
list. Research steps have no public provenance contract.

The old regression asserting that a visitor cookie could promote an answer was
replaced with a denial assertion and a dispatch-must-not-run trap. Existing local
model/format tests now opt into local mode explicitly. New tests cover strict
defaults, actual SPA cookie issuance, scoped token denial, proxy/non-loopback
rejection, dynamic operation grants, mixed-visibility reads and ETag invalidation.
Installed v10 validation passed 569 Python tests (including seven new auth tests)
and 89 unchanged tests from an isolated Siwen copy. The independent populated
Siwen bake proof stopped at `/api/trails` with HTTP 401 and is not a passing bake
claim. Installed hashes and full-suite evidence are in
[`python-native-candidate-v10.json`](results/python-native-candidate-v10.json).

The rebuilt v11 candidate passed the full 569-test suite and unchanged 89-test
Siwen subset. The populated offline Siwen proof now passed all 22 required GET
endpoints and baked 133 JSON / 112 HTML files with no skips. Original downstream
hashes/status and raw bytes remained unchanged. Evidence:
[`python-native-candidate-v11.json`](results/python-native-candidate-v11.json),
[`siwen-candidate-v11.json`](results/siwen-candidate-v11.json).

Reviewed host projections can now use the explicit app-scoped
[public read projection v1 contract](public-read-projections.md). It skips the
legacy snapshot/opaque gate only for exact registered read pipelines, retains
scope checks and fails closed if the registered composition changes. Unregistered
opaque aggregates retain the policy described above.
