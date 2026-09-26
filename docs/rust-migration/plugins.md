# Extension boundaries

Language profiles are serializable data: native English defaults, configured multilingual sections and bare-body wenyan all use the same Rust core. Python facades resolve live constants and callable overrides on each invocation. A Python callback remains a real Python boundary and is recorded as such; it is not silently converted into a Rust constant or skipped. Native profiles requiring Python-only behavior return an explicit capability error unless a host supplies that behavior.

Rust extensions use ordinary code/traits and application services. The first version does not promise a stable dynamic Rust ABI, arbitrary hot loading or a WASM sandbox.

External process adapters are configured by the operator under `plugins.process_adapters` in the trusted library configuration:

```yaml
plugins:
  process_adapters:
    external_parser:
      command: ["/absolute/path/to/parser", "--stdio-json"]
      timeout_seconds: 60
      max_output_bytes: 16777216
```

Invoke `op plugin_ingest --args '{"adapter":"external_parser","input":{"document":"external-format input"}}'`. Only the configured executable and fixed argument vector can run; request data goes to stdin, never a shell command. The caller needs Read, Write, Model and Private capabilities. Credentials and arbitrary environment variables are not inherited. The child receives only a minimal PATH and runs in the configured library directory. Configured executables are trusted operator code, **not sandboxed code**; they must implement their own network and external-file access policy. Remote callers cannot supply executable paths or modify the adapter registry.

Input is one JSON document:

```json
{"protocol_version":1,"library_id":"example","input":{"document":"external-format input"}}
```

The child must close stdout with one JSON reply and exit successfully:

```json
{"protocol_version":1,"documents":[{"path":"collection/source.md","content":"---\ntitle: Example\n---\nOriginal text.\n","expected_hash":null}]}
```

Protocol v1 accepts1–64 UTF-8 md/markdown/txt documents relative to this library's raw root. Traversal, duplicate paths, invalid frontmatter and unrecognized reply fields are rejected before any accepted write. Existing content requires the expected hash, while an exact replay of already accepted bytes is idempotent. The original accepted bytes are journaled unchanged, indexed by Rust, and acquire normal revisions/evidence. Partial concurrent conflicts return a partial job with accepted and failed documents; they never report an empty success.

Input is bounded to16MiB, output to the operator's1KiB–64MiB bound, stderr to64KiB, and runtime to1–600seconds. Timeout or cancellation kills and reaps the direct child. An adapter spawning its own descendants must manage them; the adapter interface does not claim process-tree sandboxing. Stderr and input are omitted from durable job errors; secrets are never echoed as diagnostics. A single durable running job is created atomically and renewed while the process runs. The native core retains storage, indexing, revisions, jobs and scope checks; the process only supplies parsed documents.

Tests: `cargo test -p llmbase-core --test plugins` runs actual subprocesses and proves stdin data is not executed, scope denial precedes spawn/storage, traversal rejects the full reply, bytes/revisions survive idempotent replay, stderr is withheld, and cancellation/output bounds stop the child. This does not claim that every historical network connector has already been migrated; that remains tracked in parity.md.
