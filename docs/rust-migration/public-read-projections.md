# Flask public read projections v1

The optional Flask adapter supports explicit host-owned public projections.
This resolves the R2 integration boundary: a site can filter a mixed library's
entities/statistics/exports and wrap a bounded detail view without requesting
private authority or repeating the legacy whole-library visibility scan.

```python
from llmwiki.web import create_web_app, register_public_read_projection

app = create_web_app(base)
install_site_privacy_hooks(app)
install_site_views_and_wrappers(app)
# Register only after ALL before/after hooks and view wrappers are installed.
for endpoint in ("api_article", "api_entities", "api_stats", "api_export_article"):
    register_public_read_projection(app, endpoint, app.view_functions[endpoint])
```

Registration is an explicit attestation by the embedding host, not automatic
verification of a filter or a sandbox for untrusted Python. Review the complete
view and its before/after hooks first. The host must authorize current source
metadata, filter every returned field and related source, and preserve safe
alias/path/backlink behavior. Aggregate prose requires real public provenance;
deleting private names from an opaque artifact does not establish it. Callers
must test both private denials and positive public content in a mixed library.

The binding belongs to one app and records the exact view, route rules and ordered
before/after hook chains. Registration rejects duplicate endpoints, noncallables,
unknown/unsupported apps or endpoints, mutation/model routes, any read endpoint
with an additional unsafe-method rule, and setup after the first request. It
modifies the view only after validation succeeds. Later view/rule/hook changes
invalidate the registration and produce 503; rebuild and register a fresh app.
`functools.wraps`, function names and `__wrapped__` confer no authority.

Only the legacy visibility snapshot and mixed-aggregate private gate are skipped
for registered reads. The caller must still possess `read`; anonymous visitors
have that scope. No `private`, `model`, `write` or `operator` scope is granted.
Other endpoints retain their existing policies. Unregistered mixed opaque
aggregates remain closed, and unregistered wrapped detail views still scan.

Registered requests remove `If-None-Match` and `If-Modified-Since` before site
hooks. Responses are `no-store`, without ETag or Last-Modified. Credentialed
responses retain `private, no-store` and their Vary headers. An unsolicited 304
becomes 503. The adapter checks the binding and read authority again after the
mutable after-request pipeline, so a changed hook cannot rewrite a denial into
unfiltered success. This is a transitional freshness policy, not a shared cache.

Supported endpoints are the adapter's existing content reads: article/sections,
raw-source detail, articles/lite/sources, entities/stats/search/collections/aliases,
article/tag/graph exports, taxonomy/xici/trails/health and whole-wiki export.
Registration does not apply to the separate agent or search-only factories.

## Siwen / Taixu R3 handoff

The [adapter patch](results/c6-public-projections-2026-10-12/primary/siwen-public-read.patch)
targets both returns of `create_siwen_web_app` at Siwen `7198482`, including its
no-frontend-assets path. It registers
only the twelve routes whose current Siwen pipeline already checks or projects
public content. Opaque taxonomy/xici/trails/health/tag/graph/whole-wiki projections
are deliberately not asserted public. Apply the patch after reconciling any
concurrent site changes; all registrations must remain after the final hooks and
wrappers. Merely installing the new llmbase wheel does not opt a site in.

Siwen's `public: false` and permissive textual flag spellings remain its own
language/profile policy. This API does not silently change the Rust core's
`private` / `visibility` contract. The site pipeline must continue enforcing its
stricter policy before any public response.

R3 must use the same upstream commit and adapted site commit for all checks:
anonymous mixed-library entities/stats/public export, immediate private edits
including conditional requests, wrapped article snapshot count zero, real Siwen
and Taixu full bakes, private-sentinel absence and positive-content comparison.
The narrower model/write credentials required by Rust remain a separate migration
contract; do not turn on anonymous operator mode to make QA fixtures pass.
