import hashlib
import json
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
baseline = root.parent / "c5-detail-visibility-20261012/candidate"
code = """
import hashlib,json,sys
from pathlib import Path
import llmwiki
from llmwiki import _native, _core
package=Path(llmwiki.__file__).resolve().parent
assert package.is_relative_to(Path(sys.prefix))
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix))
backend=_native.backend_info()
assert backend['native'] and backend['core']=='rust'
print(json.dumps({'package':str(package),'backend':backend,
'extension_sha256':hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest(),
'python_files':{str(p.relative_to(package)):hashlib.sha256(p.read_bytes()).hexdigest() for p in package.rglob('*.py')}}))
"""
report = {name: json.loads(subprocess.check_output([str(env / "bin/python"), "-c", code], text=True))
          for name, env in (("baseline", baseline), ("candidate", root / "candidate"))}
candidate = report["candidate"]
for name, sha in candidate["python_files"].items():
    assert hashlib.sha256((root / "engine/llmwiki" / name).read_bytes()).hexdigest() == sha, name
assert set(candidate["python_files"]) == {str(p.relative_to(root / "engine/llmwiki"))
                                        for p in (root / "engine/llmwiki").rglob("*.py")}
assert report["baseline"]["extension_sha256"] == candidate["extension_sha256"]
wheel = next((root / "wheels").glob("*.whl"))
candidate["wheel_sha256"] = hashlib.sha256(wheel.read_bytes()).hexdigest()
baseline_wheel = next((baseline.parent / "candidate-wheels").glob("*.whl"))
report["baseline"]["wheel_sha256"] = hashlib.sha256(baseline_wheel.read_bytes()).hexdigest()
(root / "provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(f"Verified {len(candidate['python_files'])} installed Python files; identical Rust extension")
