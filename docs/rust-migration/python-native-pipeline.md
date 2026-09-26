# Python pipeline persistence boundary

The candidate routes default JSONL append/read/tail, `rebuild_state`, and
`ChunkCache.get/put/clear` into `llmbase_core::compat_pipeline`. Python retains
`run_stage`, `StageContext`, job callbacks, the `StageState` dataclass, and the
live timestamp hook. This is a compatibility file protocol, separate from the
native durable jobs ledger.

`compat_pipeline_{append,events,tail,state,cache_get,cache_put,cache_clear}` return
`{result,trace}`. The trace records the executed Rust operation, explicit library
ID, storage kind, and a process-local sequence. Application routing uses a
configured library root. The local `pipeline_dispatch` binding accepts an
explicit root because existing downstream cache roots may be artifact
directories and may not exist yet; it does not guess a knowledge-library
ancestor or open SQLite. Read misses create no files or directories.

Logs remain `.pipeline/<stage>/<sha256(key)>.jsonl`. Append obtains an exclusive
flock on the actual JSONL inode, interoperating with the original Python
appender, and fsyncs each complete record. An incomplete final line is retained
and separated by a newline before the next record. Reading decodes and parses
each line independently, skipping bad UTF-8, invalid JSON, blanks, and non-object
values. Other file errors propagate. Records supplied by a replaced
`iter_events` callback are reduced in Rust; `tail` calls the replaced iterator
directly and does not claim a native storage receipt.

State preserves last-round terminal selection, all-round attempt counting and
artifact union, JSON-valued timestamps/errors, and last-round metadata overlays.
The live Python `_now_iso` supplies append's authoritative timestamp on each call;
caller-supplied event timestamps remain overwritten.

Cache layout remains `<root>/<sha256(cid)>/<sha256(content_hash)>.md`, with both
hashes complete. Rust owns the atomic temporary-file/fsync/rename write and
scope-limited clear. A stable sibling lock serializes cache put/clear operations
without placing extra files inside the legacy cache tree. Reads retain the old
OSError-as-miss behavior, universal-newline normalization, empty-string hits,
and `UnicodeDecodeError` for invalid UTF-8. Replaced `atomic_write_text` and
`shutil.rmtree` callables remain actual Python boundaries; receipts are cleared
before invocation. No model is called by any of these operations.

Validation: 10 native pipeline integration tests and 2 codec unit tests pass, including concurrent
append, kernel exclusion, source layout, big integers, and insertion order.
The actually installed candidate v9 passes all 552 Python tests (7.71 seconds),
including the original pipeline/cache tests and new native-only tests. Its
[receipt](results/python-native-candidate-v9.json) records the wheel/extension
hashes, installed module provenance, matching source hashes, and real native
append/state/cache/stage-lease and Rust-orchestrated query calls. Tests ran outside the source package in an
isolated environment; multiprocessing workers used a tests-only harness.
Native-required mode rejects a missing extension or execution error without
falling back to Python.

Candidate v6 first exposed a shared JSON-to-YAML numeric serialization bug:
542 tests passed and two compile metadata tests failed because a numeric scalar
became an internal serde map. The shared serializer was corrected and v7 passed
both regressions. This failure and correction remain recorded in the receipt.

Candidate v9 includes native extended-JSON support for Python's `NaN`,
`Infinity`, and `-Infinity`: append, event iteration, tail, state, and callback
events retain the original Python values. JSONL keeps the existing plain format.
The local bridge uses `execute_json_at` and Python's ordinary JSON decoder;
Rust performs parsing, persistence and reduction. A per-call codec uses temporary
large-integer placeholders, reserves every input number, and escapes any later
literal equal to an earlier placeholder. Restoration is one pass; literal
strings and marker-shaped user data remain data. No placeholder is persisted,
and no private serde API is used. The standard Application `Value` transport
returns a capability error for a non-finite result, since its protocol is
standard JSON; it does not silently drop the record. V7's finite-value boundary
is historical; v9 validates exact non-finite round trips, Python error summaries,
literal marker collisions, callback events, plain on-disk JSONL, and cached text.
The candidate StageLock additionally holds a Rust kernel lease on a persistent
`.lock.kernel` inode for the whole stage lifetime. Its `kernel-v1` metadata can
be recovered after death even when the former PID has been reused; an operator
unlinking the PID file cannot grant overlapping execution. Different stage/key
pairs retain parallelism. Unmarked legacy PID files retain conservative legacy
liveness checks because their ownership cannot be proved from a kernel lease
that the old process never acquired. Dedicated candidate tests passed for
process kill, live force-break fencing, reused PID, parallel keys, and exception
release. The forked-child release path preserves the parent's PID metadata and
does not explicitly unlock its shared open file description.
