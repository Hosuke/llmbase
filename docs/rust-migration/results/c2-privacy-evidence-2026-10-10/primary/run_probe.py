"""Saved C2 runner: reuse only the hash-matching B16 runtime/C1 source copy."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

if not __debug__:
    raise RuntimeError("candidate verification requires assertions enabled; unset PYTHONOPTIMIZE")

root = Path(__file__).resolve().parents[1]
b16 = root.parent / "b16-native-config"
c1 = root.parent / "c1-dual-identity-20261010"
snapshot = c1 / "siwen-snapshot"
runtime = json.loads((c1 / "primary/engine-runtime.json").read_text())
downstream = json.loads((c1 / "primary/downstream-before.json").read_text())
provenance = json.loads((b16 / "candidate-provenance.json").read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify():
    import llmwiki
    from llmwiki import _core
    package = Path(llmwiki.__file__).parent
    assert package.is_relative_to(b16 / "candidate-v2")
    assert all(digest(b16 / "source" / name) == expected for name, expected in runtime.items())
    assert all(digest(package / name) == expected for name, expected in provenance["python_files"].items())
    assert digest(Path(_core.__file__)) == provenance["extension_sha256"]
    wheel, = (b16 / "wheels-v2").glob("*.whl")
    assert digest(wheel) == provenance["wheel_sha256"]
    assert all(digest(snapshot / name) == expected for name, expected in downstream["files"].items())


verify()
identity = {"matching_runtime_inputs": len(runtime), "matching_python_files": len(provenance["python_files"]),
            "wheel_sha256": provenance["wheel_sha256"], "extension_sha256": provenance["extension_sha256"],
            "snapshot": str(snapshot), "matching_snapshot_files": len(downstream["files"]),
            "downstream_head": downstream["head"], "downstream_dirty_snapshot": bool(downstream["git_status"]),
            "assets": "historical B5, reused from C1; current frontend not accepted",
            "scripts": {path.name: digest(path) for path in sorted((root / "primary").glob("*")) if path.is_file()}}
run_name = sys.argv[1] if len(sys.argv) == 2 else "proof"
assert run_name in {"proof", "proof-v2"}
code = subprocess.call([sys.executable, str(root / "primary/prove_site_privacy_evidence.py"),
                        "--snapshot", str(snapshot), "--run-root", str(root / run_name)])
verify()
identity["source_and_installation_unchanged_after"] = True
identity["diagnostic_exit"] = code
identity["proof_directory"] = run_name
(root / "candidate-identity.json").write_text(json.dumps(identity, ensure_ascii=False, indent=2) + "\n")
sys.exit(code)
