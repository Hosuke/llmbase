# Rust extension API v1 (unreleased candidate)

Hosts compile trusted Rust extensions into their application and register them
with `Application::with_extensions`. The stock executable contains only built-ins;
there is no configuration-driven loader, dynamic ABI, hot loading or sandbox.
See the [external example](../../examples/rust-extension/README.md) for a complete
separate crate, runnable CLI, schemas and English / Classical Chinese profiles.

## Registration and wire contract

`extensions::Extension::typed::<Input, Output, _, _>(spec, handler)` generates
input and output JSON Schemas from the same Rust types used for calls. Input
implements `DeserializeOwned + JsonSchema`; output implements
`Serialize + JsonSchema`. Both root schemas must be objects. Serde validates input
before invoking the handler; use `#[serde(deny_unknown_fields)]` on strict inputs.
Schemas use deserialization semantics for input and serialization semantics for
output. The API does not interpret arbitrary JSON Schema annotations at runtime;
custom serializers and `JsonSchema` implementations are the extension author's
responsibility. Runtime serialization rejects a non-object result.

`ExtensionSpec` requires:

- `name`: at most 64 ASCII bytes, at least two dot-separated lowercase segments,
  each starting with a letter and otherwise using letters, digits, `_` or `-`.
  `llmbase` and `core` namespaces are reserved. Bare built-in names cannot collide.
- `contract_version`: `EXTENSION_API_VERSION` (currently 1). Other versions fail.
- `description`: the operation's purpose.
- `libraries`: distinct, explicit configured library IDs. No wildcard enablement.
- `required_capabilities`: distinct, nonempty native capabilities. Every one is
  required before invocation and becomes the maximum grant available in context.

Duplicate names fail when constructing the application. Language profiles of
enabled libraries are validated then. Registration lives in the application, not
a global registry. Cloning an application/extension deliberately shares its
handler closure; hosts should keep any captured mutable state appropriately scoped.

## Calling, discovery and services

`Application::execute_extension(principal, request, cancel).await` checks library
scope and required grants, decodes input, invokes the handler, and serializes its
output. The synchronous `Application::execute` rejects extension calls explicitly.
`extension_descriptors(principal, library_id)` returns authorized descriptors in
name order; ungranted or unenabled operations are omitted. These descriptors carry
the contract version, schemas and required capabilities.

Pass the configured application to `llmbase_cli::Gateway` for the shared async
dispatcher. Native HTTP accepts the same name through `POST /api/op/{name}` and
the existing structured operation endpoint; `GET /api/op` exposes authorized
descriptors for the selected library. MCP exposes the generated input/output
schemas and returns structured content. Its configured principal/default library
govern discovery and invocation. The example CLI's `--list` uses the same core
descriptor API. A host must compile its registrations in; stock CLI configuration
cannot name arbitrary Rust functions.

The handler receives only typed input and `ExtensionContext`. Context binds the
selected library, supplies its validated language profile and cancellation token,
and calls a deliberately small set of services with `execute(operation, args)`:

- Reads: `get`, `list`, `search`, `evidence`, `doctor`, `kb_stats`.
- Indexing: `index`.
- Journaled source mutations: `source_write`, `source_delete`, `source_rename`.

The existing application authorization, private-source checks, compare-and-swap,
revision history and recovery paths remain authoritative. An extra `library_id`
inside arguments never changes the selected library. Context exposes no raw
configuration, storage handle, credentials or filesystem paths. These are service
API constraints, not protection against arbitrary code in the trusted process.
Nested extensions, model calls, job management and other built-ins are not part
of this v1 context. Extension enqueueing is explicitly rejected until the worker
has a registered-handler outcome contract (#13).

## Cancellation and accepted writes

The caller token is a parent of the handler token. Cancelling the parent or
dropping the execution future cancels the child and drops the handler future.
Completing a call also cancels its child; it never cancels the caller's parent.
This is cooperative cancellation, without a promise of async cleanup: handlers
must not block an executor thread or detach uncontrolled work. Native MCP request
cancellation is wired to extension execution.

A blocking service already in progress can finish after cancellation. An accepted
write remains accepted if the extension later errors or is cancelled; extensions
are not multi-operation transactions. Inspect stored revisions and use the normal
expected hash before retrying. If indexing fails after a source write is accepted,
the durable intent remains pending and ordinary library reopen recovers it.

## Language and migration

English is the native default. A library may choose a single-section `lzh` profile
with an empty header, preserving a bare 文言 body. Original source bytes and citation
offsets remain unchanged; retrieval normalization is a separate representation.
Application behavior can move into typed Rust handlers, and prompt/language values
remain per-library settings. Arbitrary Python monkey-patch behavior does not become
an implicit process-wide Rust override. Lifecycle events (#12), scheduling (#14)
and final site acceptance remain separate work.
