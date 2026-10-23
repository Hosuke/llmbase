# Visibility snapshot limits

Since 0.10.0, operators can configure the native compatibility visibility gate
in the library's `config.yaml`. It scans frontmatter, not complete Markdown
bodies. The former 64 MiB *corpus-body* failure was fixed in `c17f3eb`; a large
scripture body does not consume that full amount of snapshot budget.

| Setting | Default | Meaning |
| --- | ---: | --- |
| `max_files` | 20000 | Markdown files across raw, concepts and outputs |
| `max_read_bytes` | 67108864 | Total bytes actually read, including failed parses and buffered read-ahead |
| `max_header_bytes` | 1048576 | Maximum framed frontmatter bytes per file |

For a growing dictionary library, an operator might choose:

```yaml
visibility_snapshot:
  max_files: 100000
  max_read_bytes: 268435456
  max_header_bytes: 1048576
```

Values must be positive integers smaller than `18446744073709551615`;
`max_header_bytes` must not exceed `max_read_bytes`. Unknown keys, booleans,
fractions, strings and null are invalid. Omitted keys retain their defaults.
Changing only `max_files` is sufficient when file count is the only bottleneck.
The native dispatch reloads these settings on each call. Request arguments and
environment variables cannot override them.

Read accounting includes up to 1 KiB of read-ahead per file, even if its header
is shorter. At the boundary, the reader may probe one extra byte to establish
that a limit has been exceeded. Headerless files need only a short opening probe.
This budget does not change individual content-reader limits or the work needed
by search and other full-content operations.

When a limit is exceeded, the Web and Agent HTTP visibility gates return 503:

```json
{
  "status": "error",
  "code": "visibility_snapshot_limit_exceeded",
  "message": "Public visibility snapshot exceeds configured max_files limit",
  "limit": {"resource": "max_files", "configured": 20000, "observed": 20001},
  "config_key": "visibility_snapshot.max_files"
}
```

The response identifies the setting without exposing filenames, paths, metadata
or body text. Invalid configuration returns code
`visibility_snapshot_config_invalid` and `config_key: "visibility_snapshot"`.
Other snapshot failures retain the generic unavailable response.

Direct Python native dispatch raises `llmwiki._core.VisibilitySnapshotLimitError`
(a `RuntimeError`) with `code`, `resource`, `limit` and `observed` attributes, or
`VisibilitySnapshotConfigError` (a `ValueError`) with `code`.

All failures remain closed to anonymous readers. Malformed or unterminated
headers within budget remain non-public; a header over budget fails the entire
snapshot with the explicit limit error. Raising a budget does not change the
privacy predicate, grant private-read scope, bypass symlink checks, or make an
opaque mixed aggregate safe to publish. See [HTTP authentication](python-http-auth.md)
and the [stable public projection API](public-read-projections.md).
