# Integration pilot: adopting the Rust candidate wheel in isolated downstream venvs

## C1: identity verification slice (2026-10-10)

Issue #10 is being accepted in small slices. [C1 is accepted](results/c1-dual-identity-2026-10-10/README.md):
six regressions, independent installed-candidate identity checks and the actual
two-site diagnostic pass (24 required routes, 159 JSON / 135 HTML per site).
C1 repairs the diagnostic's library
identity assertions; it is not completion of the whole issue or permission to
switch either site. The older B5 narrative below remains historical.

Current downstream inventory: Siwen HEAD
`cb7ccbb21bfca9a0be78ab290b12b55dea62503d`, with existing uncommitted backend/bake
changes. Its `AGENTS.md` and `CLAUDE.md` explicitly define Taixu as the same
application code with a separate library and instance. Both use
`SiwenKB`, `create_siwen_web_app` and `siwen.bake`; a separate Taixu repository is
not a missing prerequisite. The diagnostic uses a manifest-verified copy of
60 current source/config files, including the dirty working versions, never
the original checkout or production corpus.

The source checkout changed concurrently after this snapshot (`web.py` and
`prerender.py` hashes differ); the isolated 60-file copy did not. Acceptance
applies only to the frozen copy, not those subsequent changes.

The engine is archived commit `951dfb1`. The runner may reuse B16 candidate-v2
only after matching all 166 current runtime/build inputs to the B16 source,
all 48 installed Python files, and the recorded wheel/native-extension hashes.
Reusing that exact runtime saves a build; it does not refresh broader B16 gates.
Historical B5 static assets are copied within the remote runner for the bake
diagnostic; this is not acceptance of the current frontend build or browser UI.

The old verifier declared `siwen`/`taixu` in its receipt while accepting native
`default` journal rows. C1 must instead select and observe the actual IDs in
native doctor results and journal rows (the current rebuild/ask traces omit
the ID). Environment selection is a sequential
test technique and must be restored after exceptions; deploy independent
processes for the existing environment/global-Python profile boundary. Do not
claim concurrent global-patch isolation.

### Minimum site contract and adaptation queue

| Surface | Required contract | Classification / next acceptance |
|---|---|---|
| Library identity | Distinct IDs and roots; same slug resolves only within its library; wrong-ID access refused | C1 tests observed identity; authenticated cross-library transport denial remains issue #18/#19 |
| Read/search/evidence | Current site entry reads correct content; source revisions and byte spans remain resolvable | Existing native services; complete site-level evidence checks remain #10 |
| Language | English native default; explicit bare-body wenyan site profile; original bytes unchanged | Profile data; live Python globals are an optional transition boundary, #11/#20 |
| Compile/review | Site compiler behavior maps to explicit native draft/apply operations and reviewed hashes | Downstream adaptation: the current Python compiler is a fork, not proof of a native site compiler; #11/#16 |
| Hooks/jobs | Registered behavior is scoped and outcomes survive failure/restart | Engine extension/workflow work in #12/#13/#14; not covered by a read/bake proof |
| Static export | Real Flask client and BakeRun; no other-library/private data; source files unchanged | Current backend copy with historical assets is diagnostic only; current frontend acceptance #19 |
| Privacy | Explicit private data must be denied across all public API/export surfaces | The historical internal-type raw fixture tests display filtering, not full private-capability enforcement; remains open |

Do not replace the current diagnostic's explicit failures with permissive
fallbacks. Record the actual failing surface and assign it to the queue above;
the identity-verifier slice must not grow into an engine or downstream rewrite.

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
