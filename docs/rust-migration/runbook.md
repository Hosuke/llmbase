# Isolated build and migration runbook

This is a development candidate. Use a separate worktree and fixture library. The user has authorized the README update and release once the Rust migration passes acceptance. The release workflow follows repository review/publication rules and must record which channels actually published; development checks below do not constitute a production switch.

## Build and checks

Rust1.98 is pinned by rust-toolchain.toml and Cargo.lock. The native CLI/core/model do not require Python to run. Python3.11+ is needed only to build/test the optional PyO3 facade; Node22 is used for the unchanged React frontend.

```bash
bash scripts/rust-migration/check-native.sh
bash scripts/rust-migration/check-candidate.sh /absolute/source /absolute/new-candidate-run
bash scripts/rust-migration/check-frontend.sh
```

Set PYO3_PYTHON to the isolated Python interpreter before workspace checks. The native check performs fmt, clippy with warnings denied, workspace tests and release CLI build. The candidate script requires a new run directory, installs the real wheel in isolation, verifies import paths/native provenance and runs the original tests using a tests-only multiprocessing harness. It must not import the source checkout's llmwiki package. Provider credentials are unset for these checks.

The authorized remote runner is baypier-prod, root `/root/llmbase-rust-migration-20260920`. Toolchains and venv live below that root; no system or production files are changed. Run long checks via tmux, saving a log and exit file. Do not rsync over a source snapshot while it is building. The current commands and results are recorded in status.md.

The Linux no-Python proof uses the release binary:

```bash
python scripts/rust-migration/verify-standalone.py --binary target/release/llmbase --run-root /absolute/new-proof-run --chroot
```

The external finite test controller hosts a deterministic HTTP fixture. Its child chroot contains only the native ELF, shared libraries and fixture library, with no Python executable/package. The receipt records the binary hash, runtime files, actual commands and provider-call counts. This is engineering validation; real provider and semantic quality remain separately marked.

After the first CLI lexical search/evidence resolution, the same proof starts
the actual release executable inside that interpreter-free chroot: `serve
--host 127.0.0.1 --port 0 --public-read` and `mcp` over stdio. The HTTP port is
read from its startup banner; a synthetic owner bearer stays in the child
environment. HTTP checks complete CLI/evidence identity, a temporary source's
write/search/resolve/delete lifecycle, and 403 rejections for visitor-cookie
writes and owner access to another configured library.

For SSE, the fixture pauses after its first delta. The controller must receive
that delta while the gate remains held, then release it and collect evidence,
usage and done with matching request ID/version and increasing sequence. Gate
expiry fails the proof; an absolute stream deadline also bounds keepalives.
MCP checks initialization, tool discovery/schema, index/search/evidence calls,
unknown-tool errors and protocol-only stdout. HTTP must exit cleanly after
SIGINT; MCP must exit cleanly after stdin EOF. Terminate/kill are failure cleanup
only. Receipts preserve actual JSON requests/responses, exits and cleanup
actions without authorization headers. Both servers end before CLI work resumes.

## Copy-first migration

Read-only inspection comes before Library/database creation:

```bash
llmbase --base-dir /path/original --library-id example --json migration dry-run
llmbase --base-dir /path/original --library-id example --json migration copy /path/new-copy
llmbase --json migration verify /path/new-copy
```

The target must be separate from every configured source/meta root. The migration preserves original files, unknown metadata and hashes, records receipts, and indexes the copy. Resume verifies existing copied bytes instead of blindly overwriting them. Use the new copy explicitly for subsequent operations.

```bash
llmbase --base-dir /path/new-copy --library-id example --json index
llmbase --base-dir /path/new-copy --library-id example search query "example"
llmbase --base-dir /path/new-copy --library-id example search raw "原句"
llmbase --base-dir /path/new-copy --library-id example export article example
llmbase --base-dir /path/new-copy --library-id example lint check
```

Native query is read-only by default. Source compilation produces a reviewable draft; inspect its bytes and base hash, then explicitly apply it. Model access uses operator configuration and runtime environment secrets; no key belongs in config.yaml or a .env file. Legacy deep/file_back/promote defaults are translated by their compatibility entry points.

```bash
llmbase --base-dir /path/new-copy --library-id example compile source SOURCE_ID
llmbase --base-dir /path/new-copy --library-id example compile draft DRAFT_ID
llmbase --base-dir /path/new-copy --library-id example compile apply DRAFT_ID
```

Updates require `--expected-hash` matching the reviewed draft's existing target; new articles omit it. Draft acceptance also verifies all input source revisions and live hashes under the same writer lock.

## Backup, restore and rollback

```bash
llmbase --base-dir /path/new-copy --library-id example migration backup /path/new-snapshot
llmbase migration verify /path/new-snapshot
llmbase migration restore /path/new-snapshot /path/new-restored
llmbase migration verify /path/new-restored
llmbase migration rollback /path/new-snapshot
```

Backup includes authoritative original revisions, accepted content, taxonomy/relations, jobs and audit state, not just rebuildable FTS data. Restore uses a new target and checks the receipt. Rollback verifies the preserved original; it does not switch a runtime or falsely claim destructive in-place format reversal. The original remains available to the old Python reader.

The end-to-end copy-first proof is `scripts/rust-migration/verify-migration.py` (run on the isolated remote runner with the pinned release binary): it builds a synthetic custom-path legacy library, proves the actual baseline Python reader (config/export/raw search from the pristine source archive, module origins and hashes asserted) against the untouched original before and after the cycle, then exercises dry-run read-only guarantees, copy/verify byte-exact mapping, restore-into-working writes and a finite index job, backup/restore survival of accepted/manual content, assets, metadata, completed jobs and deleted-source evidence, rollback against the immutable first copy, a simulated incomplete-receipt resume with conflict recovery (not a crash experiment), and a negative verify on tampered bytes. The legacy reader is a separate Python subprocess and is not interpreter-free runtime evidence.

The controller requires PyYAML; the external legacy interpreter also needs the
baseline package's dependencies. Supply the pinned archive, its pristine
extracted source and per-file hash manifest. Provenance is checked after actual
reader calls, including lazy imports. The native source manifest is recorded
alongside the binary hash; the accepted checkpoint separately verifies those
source bytes. Use a new run directory on every attempt:

```bash
python scripts/rust-migration/verify-migration.py \
  --binary /isolated/llmbase-cli-bin --run-root /isolated/new-proof \
  --python /isolated/dependency-venv/bin/python \
  --legacy-source /isolated/pristine-legacy-source \
  --legacy-archive /isolated/legacy-a7c4d9d.tar.gz \
  --legacy-hashes /isolated/legacy-source-sha256.json \
  --source-manifest /isolated/accepted-source.sha256 \
  --fixtures tests/rust_migration/fixtures/downstream/library
```

The proof checks full historical Evidence, deleted source and completed job
identity, non-path configuration/profile preservation, and saves both original
inventories. A child timeout remains a failure and retains its attempted command
and partial output. `summary.json` is the result, not an exit-code substitute.

## Serving and queued work

Run `serve`, `web`, workers and long tasks on the remote runner/supervisor. Both HTTP surfaces require bearer authorization by default. Optional public reads expose only the default library. LLMBASE_API_SECRET grants the configured default-library owner; LLMBASE_AUTH_GRANTS maps token environment-variable names to explicit library/capability lists. Tokens themselves stay in the process environment, never configuration files or logs. Model and write capabilities are independent of visitor cookies.

MCP uses `llmbase ... mcp` over stdio, with stdout reserved for the official protocol. Tool discovery advertises implemented native capabilities; remaining parity gaps are explicit errors until implemented.

```bash
llmbase --base-dir /path/new-copy --library-id example jobs enqueue index
llmbase --base-dir /path/new-copy --library-id example jobs run-one --worker-id runner-1
llmbase --base-dir /path/new-copy --library-id example jobs list
```

Each run-one invocation is finite. See jobs.md for leases, retry/backoff, cancellation and committed-write limits, plugins.md for external adapters, and architecture.md for authority/recovery rules.
