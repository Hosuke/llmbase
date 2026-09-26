# Integration pilot: adopting the Rust candidate wheel in isolated downstream venvs

Current acceptance status (2026-09-26): the B5 dual-site run remains diagnostic,
not accepted proof. B5.1's single-site taxonomy/bake and installed-wheel tests
are accepted in `checkpoints/helper-registration-2026-09-26/`; that wheel
predates the current native core repairs. Checkpoint `d937550` now records a
fresh passing Rust workspace/release CLI, but no matching final candidate wheel
or accepted Siwen+Taixu run yet. The instructions below are a pilot procedure;
historical B5 receipts and descriptions must not be cited as current dual-site
or production acceptance. Build and verify a fresh candidate before promotion.

This document tells a downstream developer (e.g. the Siwen/Taixu sites) how to
evaluate the llmbase Rust integration candidate in a throwaway venv against
**copies** of each library, verify exactly what is loaded, and return to the
previous environment. It proves an integration candidate only — it is **not**
production acceptance, not full Rust parity, and not a data migration. No
production service, `/srv` path, real corpus, or provider account is touched.

Scope limits (asserted by the proofs, not just claimed):

- The Siwen custom compiler, custom taxonomy and Flask extension code remain
  Python; native receipts cover only the named migrated operations.
- No real-provider semantic quality is claimed; all model behaviour in the
  proofs is explicit offline Python callbacks, and external network is blocked
  inside proof processes.
- Global Python monkey-patches (e.g. prompt overrides) are process-wide; the
  dual-site proof serializes them per operation and does not claim per-library
  isolation for them.

## 1. Build or obtain the candidate wheel

The project builds with `setuptools.build_meta` + `setuptools-rust` (see
`pyproject.toml`), not maturin. From a source tree:

```sh
python3 -m venv build-venv
build-venv/bin/pip install build
build-venv/bin/python -m build --wheel --outdir wheels /path/to/llmbase-source
```

Record the wheel path and SHA-256 (`shasum -a 256 wheels/*.whl`). The B5 run's
wheel, hashes, platform and receipts are in
`.rust-migration-runs/b5-integration/logs/b5-receipt.json` (remote host) and
`.rust-migration-runs/b5-integration/REPORT.md`.

## 2. Install into a NEW isolated venv

```sh
python3 -m venv candidate
candidate/bin/pip install /path/to/wheels/llmwiki-*.whl pytest pytest-timeout
# downstream package without touching the wheel:
candidate/bin/pip install --no-deps /path/to/siwen-copy
candidate/bin/pip install click rich python-frontmatter pyyaml opencc-python-reimplemented
```

Never `pip install -e` llmwiki, never set `PYTHONPATH` to a source checkout —
the point is to exercise the installed wheel. Before running anything:

```sh
unset OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
export LLMBASE_REQUIRE_NATIVE=1
```

## 3. Verify loaded extension identity

```sh
candidate/bin/python - <<'PY'
import hashlib, importlib.metadata, pathlib, sys
import llmwiki
from llmwiki import _core, _native
prefix = pathlib.Path(sys.prefix).resolve()
assert pathlib.Path(llmwiki.__file__).resolve().is_relative_to(prefix)
assert pathlib.Path(_core.__file__).resolve().is_relative_to(prefix)
assert _native.backend_info()["core"] == "rust"
print(importlib.metadata.version("llmwiki"), _core.__file__)
print(hashlib.sha256(pathlib.Path(_core.__file__).read_bytes()).hexdigest())
PY
```

Compare the extension hash against the run receipt (`candidate-provenance.json`
/ REPORT.md) to confirm you load the exact candidate build.

## 4. Select a COPY of each library

Each site keeps its own library root (e.g. wenku, wenku-fo). Copy, never share:

```sh
cp -a /path/to/wenku /tmp/pilot-wenku-siwen
cp -a /path/to/wenku-fo /tmp/pilot-wenku-taixu
```

Point each app/KB at its copy (`create_siwen_web_app(base)` /
`SiwenKB(base)` with the copy path). The proofs demonstrate this end to end
with synthetic copies: distinct library IDs, distinct branding
(斯文 / 太虛間), distinct canonical origins, same-slug articles with
site-specific sentinels, private fixtures excluded from public routes, and
per-library atomic journal isolation. Data in the originals is never read or
written; switching back needs no migration because nothing migrated.

## 5. Run the bounded offline proofs

From any cwd, with the candidate venv python:

```sh
# single-site adoption proof (provenance via copied-source manifest):
candidate/bin/python scripts/rust-migration/prove_siwen_candidate.py \
  --snapshot /path/to/siwen-copy --run-root /tmp/proof-siwen \
  --source-manifest /path/to/siwen-sha256.txt --wheel wheels/llmwiki-*.whl

# dual-site isolation proof:
candidate/bin/python scripts/rust-migration/prove_dual_site_candidate.py \
  --snapshot /path/to/siwen-copy --run-root /tmp/proof-dual --wheel wheels/llmwiki-*.whl
```

On a host that can see the original Siwen checkout, the legacy invocation
(`--baseline baseline-siwen-original.json` instead of `--source-manifest`)
still works and additionally re-hashes the original tree and its git status.
The remote receipt never claims to hash an original it cannot reach; original
preservation is attested by a separate local receipt.

## 6. Return to the previous environment

```sh
deactivate            # or just close the shell
source /path/to/previous-venv/bin/activate
```

The candidate venv, proof run roots and library copies are disposable
(`rm -rf` when done). Original libraries, the previous venv and any running
services are untouched, so there is nothing to roll back.

## Evidence and boundaries for this run

Exact commands, exit codes, wheel/extension hashes, test and bake counts, and
the remaining boundaries for the B5 pilot are recorded in
`.rust-migration-runs/b5-integration/REPORT.md` and the remote
`b5-receipt.json`. Historical output counts from earlier candidates are
context only; accept only counts produced by the current run.
