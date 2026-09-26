# Markdown normalization

Paragraph normalization and heading scanning/rewriting run in the Rust
`compat_util` module. These are pure operations: they neither open a library nor
register roots or write files. The original paragraph algorithm and its frozen
229-case oracle are unchanged by the heading migration.

`compat_util::transform("normalize_heads", {"body": ..., "rules": [...]})`
accepts ordered `{pattern: string, level: integer}` rules, with levels 1–6.
The first matching rule wins. This native profile uses the Rust `regex` dialect;
lookaround and backreferences are unsupported and return errors. All rules are
validated eagerly, including for an empty body. Limits are 16 MiB of UTF-8 body,
256 rules, 64 KiB of aggregate pattern text and 1 MiB per compiled regex.
Empty rules preserve the body's bytes within the body-size limit.

`normalize_heads_with` owns Python-compatible line splitting, ATX recognition,
fence/HTML block state, title extraction, heading rewrite and newline assembly.
It accepts a classifier returning an optional level for each eligible title.
No classifier is called for skipped blocks; errors stop the operation immediately.
The conservative legacy HTML behavior closes blocks on blank lines. Mixed LF
and CRLF documents use CRLF when rebuilt; trailing LF/CRLF counts are retained.
These historical normalization semantics are not a general Markdown parser.

The Python `normalize_heads(body, rules)` facade eagerly validates levels and
compiles the original trusted Python patterns. Its callback only matches the
title supplied by Rust. Compiled flags, lookaround, backreferences and live
pattern objects retain Python semantics. This is an explicit Python extension
boundary: the success trace says `core=rust`, `matcher=python_regex` and records
the actual callback count. With no rules, `matcher=none` and the count is zero.
Python regex complexity remains the responsibility of the trusted caller;
the native declarative rule limits do not restrict this compatibility matcher.

The FFI releases the GIL during scanning and attaches for each callback. Callback
errors keep their original Python exception instance, with no retry or partial
result. Failed facade/bridge calls clear traces, including a nested native success
inside a failing callback. Float levels preserve the old behavior: validation
may accept them, but a matching rule raises TypeError when an integer is needed.

Replacing any of the eight captured parser/rewriter helpers activates the live
Python implementation and a `core=python` receipt. Restoring those identities
restores native execution. An unbuilt checkout may use that Python implementation;
an installed native candidate or `LLMBASE_REQUIRE_NATIVE=1` fails closed when the
extension cannot load. Taixu-specific title promotion and deduplication remain
downstream policy, outside this generic primitive.
