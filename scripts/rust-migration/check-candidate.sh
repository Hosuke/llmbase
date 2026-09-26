#!/usr/bin/env bash
set -euo pipefail
# Finite runner task. The supplied directory must be disposable and outside the repo.
source_root=${1:?usage: check-candidate.sh ABSOLUTE_SOURCE_ROOT ABSOLUTE_EMPTY_RUN_ROOT}
check_root=${2:?an isolated run root is required}
if [ -e "$check_root" ]; then
  printf '%s\n' 'Candidate run root must not already exist.' >&2
  exit 2
fi
mkdir -p "$check_root/wheels"
python3 -m venv "$check_root/build"
"$check_root/build/bin/python" -m pip install --disable-pip-version-check build
"$check_root/build/bin/python" -m build --wheel --outdir "$check_root/wheels" "$source_root"
python3 -m venv "$check_root/candidate"
"$check_root/candidate/bin/python" -m pip install --disable-pip-version-check "$check_root"/wheels/*.whl pytest pytest-timeout 'mcp>=1.0.0' 'pymupdf>=1.24.0'
unset PYTHONPATH OPENAI_API_KEY LLM_API_KEY LLMBASE_API_KEY LLMBASE_EMBEDDING_API_KEY
unset LLMBASE_BASE_URL LLMBASE_MODEL LLMBASE_EMBEDDING_BASE_URL LLMBASE_EMBEDDING_MODEL
export LLMBASE_REQUIRE_NATIVE=1
cd "$check_root"
"$check_root/candidate/bin/python" - <<'PY'
import hashlib
import importlib.metadata
import json
import pathlib
import sys
import llmwiki
from llmwiki import _core
prefix = pathlib.Path(sys.prefix).resolve()
for module in (llmwiki, _core):
    assert pathlib.Path(module.__file__).resolve().is_relative_to(prefix), module.__file__
wheel = next(pathlib.Path("wheels").glob("*.whl"))
receipt = {"version": importlib.metadata.version("llmwiki"), "python": sys.version,
           "package": llmwiki.__file__, "extension": _core.__file__,
           "wheel": str(wheel), "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
           "backend": _core.backend_info()}
pathlib.Path("candidate-provenance.json").write_text(json.dumps(receipt, indent=2))
print(json.dumps(receipt, indent=2))
PY
mkdir -p "$check_root/test-harness"
rsync -a --exclude=__pycache__ "$source_root/tests/" "$check_root/test-harness/tests/"
# Spawned multiprocessing workers must import test helpers without exposing a
# source-tree llmwiki package. Parent tests retain their original fixture paths.
cd "$check_root/test-harness"
"$check_root/candidate/bin/python" -m pytest --import-mode=importlib "$source_root/tests" -q
