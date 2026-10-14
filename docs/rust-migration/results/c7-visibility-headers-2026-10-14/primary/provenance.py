import hashlib
import importlib.metadata
import json
from pathlib import Path
import sys

import llmwiki
from llmwiki import _core

root = Path(__file__).resolve().parents[1]
prefix = Path(sys.prefix).resolve()
for module in (llmwiki, _core):
    assert Path(module.__file__).resolve().is_relative_to(prefix), module.__file__
package = Path(llmwiki.__file__).parent
files = list((root / "engine/llmwiki").rglob("*.py"))
for source in files:
    installed = package / source.relative_to(root / "engine/llmwiki")
    assert installed.read_bytes() == source.read_bytes(), source
wheel = next((root / "wheels").glob("*.whl"))
receipt = {
    "version": importlib.metadata.version("llmwiki"), "python": sys.version,
    "package": llmwiki.__file__, "extension": _core.__file__,
    "wheel": str(wheel), "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
    "extension_sha256": hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
    "matching_python_files": len(files), "backend": _core.backend_info(),
}
(root / "provenance.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt, indent=2))
