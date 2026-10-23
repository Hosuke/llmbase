# llmwiki 0.10.0 — Rust-backed compatibility engine

0.10.0 gives the Rust-backed `llmwiki` package a distinct release identity. The
existing `v0.9.0` tag and PyPI package belong to the earlier Python release and
are preserved. The distribution name, imports, Python CLI, Flask factories,
callbacks and scheduler remain supported. Full native replacement of every
Python lifecycle and worker extension is tracked separately in the open migration
issues; this release does not close those goals.

## Install and identify

```sh
python -m pip install llmwiki==0.10.0
python -c 'from llmwiki import _native; print(_native.backend_info())'
```

The [GitHub release](https://github.com/Hosuke/llmbase/releases/tag/v0.10.0)
contains the source distribution, wheels, SHA256 checksums and build receipts.
The [PyPI project](https://pypi.org/project/llmwiki/0.10.0/) and
[ClawHub skill](https://clawhub.ai/hosuke/llmwiki) must both show 0.10.0 before
publication is considered complete.

Supported binary targets are CPython 3.11+ ABI3 on macOS 11+ arm64 and Linux
x86_64 with glibc 2.28+. CI builds each wheel **from the same sdist**, runs the
installed Python suite on 3.11, and smoke-tests the ABI3 import on 3.13. Windows,
Linux arm64, musl and macOS Intel wheels are outside this release. Unsupported
platforms require a source build and their own acceptance checks.

`backend_info()` includes:

- `core: "rust"` and the existing native version/ABI fields;
- `build_commit`: the full Git object ID, or `"unknown"` for an unmarked local
  source copy;
- `build_dirty`: tracked Git changes (`true`/`false`), or `null` when no owning
  Git checkout is available, as in the official sdist build.

Official wheels must match the release tag's commit. The sdist contains
`SOURCE_COMMIT`, allowing that identity to survive an archive build. An explicit
`LLMBASE_BUILD_COMMIT` must be a full hexadecimal object ID and, in an owning Git
checkout, match HEAD. This is build provenance, not a cryptographic attestation
of arbitrary third-party source copies; verify the release checksums too.

Source builds require the pinned Rust 1.98.0 toolchain, a C build toolchain and
Python 3.11+. Use `python -m pip install .` in an isolated environment. For a
reproducible official export, follow `.github/workflows/native-wheels.yml` rather
than repackaging an uncommitted worktree.

## Downstream requests addressed

| Request | 0.10.0 behavior |
| --- | --- |
| Recoverable native package | Separate version/tag, sdist and platform wheels with checksums |
| Server/CI installation | Genuine auditwheel-repaired manylinux 2.28 x86_64 wheel |
| Distinguish old Python and native builds | Package version 0.10.0 plus commit in `backend_info()` |
| Growth beyond 20,000 files | [Configurable file/header/read limits](visibility-limits.md) and actionable 503 errors |
| Stable custom public routes | [Public projection API v1](public-read-projections.md), supported through 0.10.x |

These requests came from Siwen/Taixu's `docs/llmbase-v0.9.0-request.md`. Downstream
reports production use of `c17f3eb` since 2026-10-15, repeated two-site bakes of
about 94,000 files matching Python 0.8.0 except for added `api/languages.json`,
and zero private-fixture leaks. This is attributed downstream evidence of
`c17f3eb`, not a claim that upstream reran the latest production corpora on 0.10.0.

Before updating either site, retain its currently accepted wheel, install 0.10.0
in a fresh environment, verify `build_commit`, retain its public-projection
registrations, and run its existing bake/privacy comparison harness. There is no
corpus-format migration required by this compatibility-package update. Raising
visibility budgets never bypasses privacy checks. Site-specific privacy predicates
and custom-route filtering remain site responsibilities.

## Verification and limits

The release workflow records exact source identity, installed-wheel provenance,
Python suite results and per-artifact SHA256. The Rust migration workflow checks
fmt, clippy, workspace tests, native release compilation and frontend integration.
Publication must use those verified artifacts from the tagged commit.

Prior fixes and measurements remain linked from [status](status.md), including
the 83,391,106-byte large-body regression and bounded public detail projections.
Paid model-provider acceptance and real-embedding quality have not been established
by offline tests. No general Rust speedup or complete all-native migration is
claimed by this package release.
