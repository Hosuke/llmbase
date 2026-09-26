# Native configuration IO and format boundaries

The Python `config._read_config` facade enters
`llmbase_core::compat_config::read_with`. Rust reads `config.yaml`, chooses the
missing/default or present/decoder branch, resolves configured paths one at a
time, and assigns the final base directory. It never registers a library, creates
directories or opens a database during a read. `load_config` explicitly publishes
the completed config through the existing Python registry afterward.

The native CLI's `LibraryConfig::load` shares the bounded file reader, then uses
the existing Rust metadata parser, mapping validation and native defaults. It
does not invoke Python. The Python facade uses an explicit PyYAML format plugin:
UTF-8 and universal-newline decoding, followed by the current `yaml.safe_load`.
Rich Python values, typed keys, shared aliases and recursive documents remain
opaque Python objects; they are never flattened into JSON. A successful facade
receipt reports `parser=python_yaml` or `parser=defaults`. This is not a claim
that the Python format plugin has become native.

`config._defaults` retains the live profile policy and language deepcopy in
Python; its default path construction/resolution is native. A replaced defaults
callable runs only for a missing file and receives the effective public base
argument. Arbitrary paths returned by that override are not silently normalized
or merged with defaults. Existing partial configuration remains partial.

`ensure_dirs` delegates sequential lookup and directory creation to the core.
The order is raw, wiki, outputs, meta, concepts. A failure preserves earlier
directory creation and prevents later lookups/creation. The FFI retains Python
callback exception identity, native OS error classes, errno and filename.
Filesystem paths use OS strings, including POSIX surrogateescape round trips.

Configuration resolution follows symlinks before parent components, permits
missing suffixes and dangling targets, and fails explicitly for loops/resource
limits. This config-specific resolver is not a scope/authorization primitive;
the existing atomic-write and recovery guards are unchanged. Resolving a path
does not authorize access to it. The native registry remains a separate local
extension boundary.

Reads are limited to 16 MiB of actual bytes, not just reported metadata size.
Path resolution is bounded to 4096 components and 128 active symlinks; directory
creation to 8192 attempts. Exceeding a bound produces a capability/validation
failure, not partial success. A missing first existence observation chooses
defaults; failures opening/reading an observed file propagate. Installed native
candidates fail closed. Only an unbuilt source checkout may use the explicitly
named Python read/mkdir fallback.

For acceptance status and evidence, consult the
[B16 checkpoint](checkpoints/native-config-2026-09-27/README.md), covering the
fresh remote candidate, actual-old comparison and required checks.
