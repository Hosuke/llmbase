# Effective configuration roots

Effective default `raw`, `wiki`, `concepts`, `outputs` and `meta` paths are
absolute. `_defaults(base)` resolves the base before appending those suffixes.
This also applies when the base does not exist yet, names a symlink, is `.` or
contains a missing component followed by `..`. Loading configuration creates no
directories; `ensure_dirs` remains the explicit creation step.

Previously a missing config with a relative base such as `library` produced
`library/wiki/concepts`. The native host then joined that to the resolved base
again, yielding `/cwd/library/library/wiki/concepts`; its scope check correctly
rejected a write to the intended `/cwd/library/wiki/concepts`. Canonical default
roots fix this mismatch without changing or weakening the native scope guard.
The returned root strings remain stable after the caller changes directory.

For an existing YAML document, `_read_config` continues to resolve each supplied
truthy root relative to the base. It does not merge absent defaults, discard
unknown fields or modify the YAML file. Explicit custom roots outside the base
remain permitted configured roots. Native scoped writes outside those roots
and writes to reserved database/coordination files remain prohibited.

`_read_config` is non-registering. `load_config` explicitly registers the loaded
configuration; trusted standalone helpers must not infer/register a root merely
to read config. Live default helper replacements remain callable and the live
language profile defaults remain deep-copied on every call. A custom default
helper is responsible for returning its chosen effective roots.

This is a correction to effective default path representation, including direct
`_defaults` calls. Configuration parsing and `ensure_dirs` still execute in Python;
their required native migration is separate and is not claimed complete here.
