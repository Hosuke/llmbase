# Rust extension example

This separate crate uses only the public `llmbase-core` extension API. Its
`example.inspect` operation accepts `{"source_id":"…"}` and returns the selected
library ID, effective default language, whether its language profile uses a bare
body, the original UTF-8 byte count, and the complete native `get` result.

For an existing indexed library, run from the repository root:

```sh
cargo run -p llmbase-rust-extension-example -- /path/to/library research source-id-or-slug
cargo run -p llmbase-rust-extension-example -- /path/to/library research --list
cargo test -p llmbase-rust-extension-example
```

Index source files first with the native CLI's `op index`, or with the public
`Library::open(config)?.sync()?` API. The example requests public read authority;
private sources remain subject to native access checks.

Hosts register `inspect_extension(library_id)?` through
`Application::with_extensions`, then call `Application::execute_extension` with
a principal, a request for `example.inspect`, and a cancellation token. Typed
input/output structs supply JSON Schemas; input rejects unknown fields. The
handler calls `ExtensionContext::execute("get", …)` under the current library
and the extension's declared read authority. It does not split, normalize, or rewrite source text.
`--list` prints the same authorized descriptors used by native HTTP/MCP discovery.

Without language configuration the operation reports `default_lang: "en"` and
`bare_body: true`. For a single-section 文言 profile, put this in that library's
`config.yaml`:

```yaml
languages:
  default: wenyan
  profiles:
    wenyan:
      default_lang: lzh
      api_default_lang: lzh
      sections:
        - code: lzh
          key: 文言
          header: ''
```

This reports `default_lang: "lzh"` and `bare_body: true`; original source bytes
remain intact. The tests cover both profiles, mixed newlines and Unicode, and
separate application registries and profiles even when library IDs match.

Extensions are trusted in-process Rust code, not a sandbox. Only register code
you trust; application authorization constrains the context's native operations,
not arbitrary filesystem or network calls made by extension code itself.
