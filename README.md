<div align="center">

# LLMBase

**Compile source material into a connected knowledge library, then ask questions with traceable evidence.**

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![PyPI](https://img.shields.io/pypi/v/llmwiki.svg)](https://pypi.org/project/llmwiki/)
[![GitHub](https://img.shields.io/github/stars/Hosuke/llmbase?style=social)](https://github.com/Hosuke/llmbase)

[English](#what-it-does) · [中文](#中文)

</div>

> **Rust migration candidate — not yet released.** This branch contains the native Rust core and the Python compatibility package. Public package badges reflect the published Python release, not this candidate. Follow the [acceptance status](docs/rust-migration/status.md) and [compatibility matrix](docs/rust-migration/parity.md) for implementation and verification coverage.

Development is paused for quota conservation. The [issue handoff](docs/rust-migration/issues/README.md) records the remaining work and the [clean-major design](docs/adr/0002-clean-rust-major-and-extensions.md): per-library profiles and explicit Rust extensions, with a documented optional Python transition layer. The extension API is planned, not yet a supported public interface.

## What it does

LLMBase keeps source documents, accepted knowledge articles, and their relationships in a library you control. Compilation produces reviewable drafts. Retrieval can search original sources, compiled articles, or both; answers carry references back to a particular source revision.

- **Native Rust core.** Storage, revision history, indexing, retrieval, model orchestration, and durable jobs run without a Python interpreter in the native runtime.
- **Markdown and SQLite.** Source files and accepted articles remain readable files. SQLite stores revision history, evidence, indexes, and operational records. Back up both; the database is not just a disposable cache.
- **Lexical and hybrid retrieval.** Full-text search works without model credentials. Optional HTTP embeddings enable vector search and reciprocal-rank fusion. Responses identify lexical-only fallback.
- **Traceable evidence.** Citations identify the library, source, revision, passage, original byte span, and exact quote. Updating or deleting a source does not silently redirect an old citation.
- **Explicit writes.** Native evidence-first answers are read-only by default. Saving an answer, compiling a source, and accepting or promoting a draft are separate operations.
- **Live Python customization.** The compatibility package preserves supported imports, runtime prompt and language overrides, callbacks, Flask factories, and static-site baking. See the matrix for boundaries still under migration.

The project is domain-agnostic. Existing downstream projects include [斯文](https://siwen.ink) and [華藏閣](https://huazangge-production.up.railway.app); their production deployments are separate from this migration.

## How the pieces fit

~~~text
source files → revisions → chunks → lexical / vector retrieval → cited answers
      │                         │
      └→ compilation drafts → reviewed, accepted articles
                                      │
                          aliases / backlinks / taxonomy

CLI · native HTTP · MCP · Python compatibility
                  ↓
         shared Rust application services
~~~

Native interfaces use English defaults. Unicode and CJK source text remain first-class data: normalization is confined to retrieval representations, while original text and citation offsets are preserved. Language profiles can select English, multiple languages, or a single Classical Chinese section with a bare body.

## Try the native candidate

Use a separate test library. The native executable requires Rust to build, but does not require Python to run. The repository pins its Rust toolchain and dependency lockfile.

~~~bash
git clone --branch rust-core-rebuild https://github.com/Hosuke/llmbase.git
cd llmbase
cargo build --locked --release -p llmbase-cli
~~~

Until the migration is published, the commands below apply to the migration worktree and its built binary. They are not an installation recipe for the current PyPI release.

~~~bash
mkdir -p /tmp/llmbase-example/raw
printf '# Example\n\nA library retains source revisions and exact quotations.\n' > /tmp/llmbase-example/raw/example.md

./target/release/llmbase --base-dir /tmp/llmbase-example --library-id example index
./target/release/llmbase --base-dir /tmp/llmbase-example --library-id example search raw "source revisions"
./target/release/llmbase --base-dir /tmp/llmbase-example --library-id example doctor
~~~

Run `--help` on the executable or a subcommand for supported options. Native operations also accept structured JSON through `op NAME --args '{...}'`. Results include explicit errors when a required model or extension is unavailable.

### Configure models

Generation and embedding adapters use configurable HTTP-compatible endpoints. Supply secrets through your process environment or deployment secret manager; do not put them in source files, frontend assets, or command examples.

~~~bash
export LLMBASE_BASE_URL=https://your-model-endpoint.example/v1
export LLMBASE_MODEL=your-model
# Supply LLMBASE_API_KEY through your secret manager.
~~~

See [model configuration and adapter behavior](docs/rust-migration/model-adapter.md) for retries, cancellation, usage, streaming, and embedding configuration.

~~~bash
./target/release/llmbase --base-dir /tmp/llmbase-example --library-id example ask "How are quotations preserved?" --stream
~~~

Streaming forwards provider deltas as they arrive. Progress events are distinguished from answer text. [The answer contract](docs/rust-migration/ask.md) covers evidence, context budgets, short answers, cancellation, and explicit saving.

### Compile and review

Use the source ID returned by indexing or source listing. Drafts retain their source revision; accepting an update checks the reviewed base hash.

~~~bash
llmbase --base-dir /path/to/test-library compile source SOURCE_ID
llmbase --base-dir /path/to/test-library compile draft DRAFT_ID
llmbase --base-dir /path/to/test-library compile apply DRAFT_ID
~~~

These commands assume the native binary is on your PATH. For an existing target, pass the draft's `base_hash` with `--expected-hash`. See [compilation and recovery](docs/rust-migration/compile.md).

## Python and downstream compatibility

The Python distribution keeps the name `llmwiki`, the import namespace `llmwiki.*`, and its legacy CLI entry point. Native and Python installations both provide a command named `llmbase`; use an explicit binary path when validating which implementation runs.

~~~python
from llmwiki.compile import rebuild_index
import llmwiki.compile as compile_module

# Assignment is the supported override; keep DEFAULT_SECTION_HEADERS intact.
compile_module.SECTION_HEADERS = [("文言", "")]
~~~

The legacy default remains English / 中文 / 日本語. A single section with an empty heading represents bare-body monolingual content. Supported overrides are read at call time; callable extensions execute at the documented Python boundary.

New native question answering is read-only by default. Legacy `deep`, `file_back`, and `promote` retain their documented compatibility behavior; select the interface deliberately.

- [Customization contract](docs/customization.md)
- [Python bridge and loaded-core verification](docs/rust-migration/python-native-bridge.md)
- [Query callbacks and write behavior](docs/rust-migration/python-query.md)
- [Compile overrides and source preservation](docs/rust-migration/python-compile.md)
- [Siwen candidate installation and bake checks](docs/rust-migration/downstream.md)

Python 3.11+ is required for the compatibility package. Build and install the candidate wheel using the [runbook](docs/rust-migration/runbook.md); `pip install llmwiki` installs the currently published release, not unpublished branch changes.

## HTTP, MCP, jobs, and extensions

The native executable offers `serve` for the Agent HTTP API, `web` for the Web API, and `mcp` for MCP stdio. MCP stdout contains protocol messages only. The retained Flask factories remain available for downstream route and middleware customization.

Remote access uses explicit library and capability grants. Public reads do not grant writes, private reads, or model calls. Configure authentication as described in the [runbook](docs/rust-migration/runbook.md) before running a remote service.

Long-running work belongs on a server or supervised runner. Durable jobs have status, leases, cancellation, retries, and event history. `jobs run-one` executes one ready job and exits, making it suitable for a scheduler. See [jobs](docs/rust-migration/jobs.md).

Profiles express data and language policy. Python callbacks and external process adapters are explicit extension boundaries. Process adapters return documents to the Rust write journal; they are trusted operator-configured programs, not a general-purpose sandbox. See [plugins](docs/rust-migration/plugins.md).

## Migrate and recover

Migration is copy-first. Inspect the original, create a separate target, verify it, then exercise backups and restoration before considering a deployment switch.

~~~bash
llmbase --base-dir /path/to/original migration dry-run
llmbase --base-dir /path/to/original migration copy /path/to/new-copy
llmbase migration verify /path/to/new-copy
llmbase --base-dir /path/to/new-copy migration backup /path/to/new-backup
llmbase migration restore /path/to/new-backup /path/to/restored-copy
~~~

Do not delete the original or treat a Markdown-only export as a full backup. The [migration runbook](docs/rust-migration/runbook.md) specifies authority, verification, restore, and rollback; the [architecture](docs/rust-migration/architecture.md) explains the file/database write journal and recovery boundary.

## Validation and release status

The migration uses fixed offline fixtures, real HTTP adapter tests, CLI/HTTP/MCP contract tests, isolated candidate-wheel tests, and a copied Siwen environment. Detailed receipts are linked from the [status report](docs/rust-migration/status.md).

[Retrieval evaluation](docs/rust-migration/retrieval-baseline.md) separates lexical quality from vector engineering tests. Synthetic embeddings do not establish semantic quality. External paid-provider smoke tests and real-embedding quality are **unverified** unless a report explicitly records otherwise. No Rust speedup is claimed without matching measurements.

Developer checks:

~~~bash
bash scripts/rust-migration/check-native.sh
bash scripts/rust-migration/check-candidate.sh /absolute/source /absolute/new-candidate-run
bash scripts/rust-migration/check-frontend.sh
~~~

Workspace checks need the isolated Python interpreter selected through `PYO3_PYTHON` for the optional PyO3 crate. Frontend checks use the existing React/TypeScript application. Run long builds and full suites on the isolated runner described in the runbook.

A release is complete only after its acceptance gates pass and the matching git tag, GitHub Release assets, PyPI package, and ClawHub skill have been verified. The release notes must state platform support, compatibility changes, migration steps, checksums, and remaining limitations.

## 中文

LLMBase 以原典為本，編為相聯之知識條目；檢索與問答皆可回查來源版本及原句。

本輪重建以 **Rust 核心、英文接口與文檔為先**，漢字、文言及多語 profile 並存。文言可作單節裸正文；Rust 關鍵字仍循官方語法。後續擴展以 profile、提示詞及明示 Rust 接口為方向；Rust 擴展註冊接口尚待實作，現有 Python callback 由相容層承接。

原生問答預設不落盤；保存、編纂及升條皆為明確操作。舊 Python 入口之寫回與動態覆寫契約，另由相容層承接。遷移先複本、後驗證，不改生產文庫。

此分支尚為候選版；安裝、驗收及發布實況，見[遷移手冊](docs/rust-migration/runbook.md)、[相容表](docs/rust-migration/parity.md)與[進度紀錄](docs/rust-migration/status.md)。

## License

[MIT](LICENSE)
