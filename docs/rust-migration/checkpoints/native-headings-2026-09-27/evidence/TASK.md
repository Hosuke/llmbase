# B14 — native heading normalization, after accepted24f34f5

Work in /Users/lanchair/Developer/LLM-infra/Github/llmbase-rust. Primary has
diagnosed the problem and chosen the contracts below. Implement this bounded
task, verify and report. No subagents, git staging/commit/push, version changes,
downstream edits, provider requests or real corpus. Preserve existing dirty files.

Ownership:
- crates/llmbase-core/src/compat_util.rs: shared heading normalizer and native action
- crates/llmbase-python/src/util.rs: explicit heading matcher callback host
- llmwiki/_native.py: clear-trace bridge for that host
- llmwiki/normalize.py: normalize_heads default delegation/override boundary only
- new crates/llmbase-core/tests/compat_heads.rs
- new tests/rust_migration/test_native_heads.py
- new docs/rust-migration/normalization.md
- this task's run/report files, excluding primary/

No dependency, Cargo.lock, other algorithm or old test changes. In particular,
leave accepted paragraph normalization and its229-case oracle unchanged.
Primary owns tests/rust_migration/fixtures/heads.json. Its60 outputs came from
the actual original Python implementation, the real16-rule Taixu pack and all
four upstream Taixu fixtures. SHA256:
f82a1cf1ce130dbab392cc52ece30bd587d208fbde387009e97cf9ec409d8fe6.
Do not edit/regenerate the fixture. Read primary/diagnosis.md. Avoid dumping the
large fixture to the console; tests should consume it directly.

## Exact design

Rust owns line splitting, heading/fence/HTML recognition, rule sequencing for
native declarative profiles, heading rewriting and newline reconstruction.
Caller-supplied Python regex matching is an explicit extension boundary, so
lookaround/backreferences/compiled flags retain their actual Python semantics.
Do not port the whole Python normalizer into the facade or call it from Rust.

Add shared public core function along these lines:
normalize_heads_with(body:&str, enabled:bool,
  level_for:impl FnMut(&str)->Result<Option<u8>>) -> Result<String>.
`enabled=false` is byte-preserving no-op, including CR-only/Unicode line endings.
For enabled calls, use the existing paragraph_lines, whitespace, fence patterns
and ParagraphPatterns::html where appropriate; do not alter their semantics.
16MiB body bound applies to this new primitive. A matching callback level must
be1..6; invalid values fail clearly. No files, library registration or DB.

Match original normalize.py ordering exactly: active fence; new fence opener
(also resets HTML state); blank line (resets HTML state); active HTML; new HTML;
eligible ATX heading; first matching rule. Fences close only on matching character
and sufficient length with whitespace-only suffix. Match Python heading and
rewrite regex semantics, including whitespace U+001C..U+001F. Regexes:
- heading: ^ {0,3}(#{1,6})\s+(.+?)(?:\s+#+)?\s*$
- rewrite: ^( {0,3})(#{1,6})(\s+)(.+?)((?:\s+#+)?)\s*$
Classifier receives the Python-strip title from heading capture2. Preserve the
rewrite groups indent/separator/title/closer exactly; legacy behavior drops final
whitespace when rewriting. Nonmatching lines are unchanged before joining.
Retain original _line_ending/_preserve_trailing_newline behavior, including
mixed CRLF/LF and terminal blank lines. No Taixu-specific promotion/dedup logic.

Add pure compat_util::transform("normalize_heads", {body,rules}) for native
declarative profiles. rules is an array of {pattern:string,level:integer1..6};
use the existing Rust regex crate, no new dependency. Compile/validate every
rule before scanning even if body is empty; first match wins. Empty rules use
the no-op. Bound native rules to256, aggregate pattern bytes64KiB, and each
compiled regex to1MiB through RegexBuilder::size_limit. Unsupported syntax and
malformed DTOs return errors, never skip rules or fall back to Python. Document
this native Rust regex dialect separately from the Python compatibility ABI.

## Python ABI/facade

New local ABI, e.g. normalize_heads_run(body, callback=None), uses the shared
Rust scanner. callback(title) returns None or target heading level. With no
callback, return the byte-preserving no-op. Release GIL while scanning and
attach only for callbacks, following query.rs's actual PyErr retention pattern.
On callback/extraction failure preserve the original PyErr instance, no retry,
no partial success. Return {value,trace}; trace marks core=rust, operation=
utility_normalize_heads, storage=none, matcher=python_regex or none and actual
callback count. Never attribute Python matching to a Rust-only profile.

_native bridge clears traces before invocation and on any exception (including
callbacks that themselves issue a successful native call then raise). Only store
the final real success receipt. No configured root registration.

normalize_heads keeps its signature/default semantics. In the native default
path Python only validates/compiles caller rules and matches a supplied title:
copy the original ordered level validation and re.compile logic, including
compiled re.Pattern flags; do not split/scan/rewrite the document in Python.
Keep original exception ordering/identity and no matching calls inside skipped
blocks. Do not add new restrictions to trusted Python patterns. An accepted
float level still fails only when used as in the old code; booleans behave as
Python integers. Use real callback results, not cached regex decisions.

Keep the old body for unbuilt-development fallback and explicit parser overrides.
Capture these eight default identities after definitions: _FENCE_CLOSE_RE,
_FENCE_OPEN_RE,_HEADING_RE,_HTML_BLOCK_RE,_HEADING_REWRITE_RE,_line_ending,
_preserve_trailing_newline,_rewrite_heading_level. If any is replaced, use the
existing Python path against live helpers and record a Python boundary; on
failure preserve the exception and clear the trace. Default path must still work
when the old algorithm body is poisoned. Missing required native must fail closed.
Do not alter the paragraph function, constants or other helper bodies.

## Acceptance

Core tests: all52 shared-dialect frozen cases (names not starting python-pattern-)
match exactly through native rules. All60 cases must pass the real Python matcher
ABI. Add tests of callback title/order/count and error propagation; empty rules,
empty body with invalid rule, unsupported native regex, size/level/type errors.
Native Rust tests run without Python callbacks/interpreter and prove the actual
new transform executes; do not label native regex as Python-compatible generally.

Installed-candidate tests:60 frozen outputs, real downstream Taixu fixtures,
compiled patterns/flags/lookaround/backreferences; poison old Python normalizer,
line-splitting/rewrite helpers appropriately (without changing identity guards
to accidentally test the override branch). Assert real Rust trace and explicit
Python matcher trace, no inferred registry changes. Test per-call rule changes,
one actual TypeError callback instance/no retry/stale trace, matched/unused float
and bool levels, one real parser-helper override then restoration, unbuilt
fallback and required-native failure. Existing tests/oracles stay unchanged.

source/ is a coherent B13 snapshot plus primary's fixture (220 inputs). before/
holds the four existing owned files. Overlay only owned changes, freeze all
build inputs and prove candidate hashes. Fresh wheel/venv mandatory.

Long build/full tests only on isolated baypier-prod:
/root/llmbase-rust-migration-20260920/b14-heads
finite tmux llmbase-b14-heads; use timeout1500s. Follow B13 primary/verify.sh
with shared isolated CARGO_HOME/RUSTUP_HOME/target, nice10/jobs2, setuptools-rust
via B11 build-env-v2. Runner Python lacks ensurepip: venv --without-pip then
isolated pip --python. Copy B13 runtime constraints; clear model keys/endpoints.
Phases: source hashes, fmt, core/Python all-target Clippy, core compat_heads plus
compat_paragraphs, fresh wheel/install/provenance, outside-source focused
test_normalize + native_heads + native_paragraphs + downstream contracts, then
full Python once if green, final source hashes. Record logs/exits/active handles.

Timebox: prioritize implementation+tests within25 minutes. Save REPORT by30
minutes with exact live/terminal state; wrapper has35-minute hard deadline.
If remote tests are still running, report the actual session/log and finish
locally without killing them. Do not spend the whole timebox planning. Stop
promptly on authentication/network failure and preserve evidence. Primary owns
independent review/verification/checkpoint and the next task assignment.
