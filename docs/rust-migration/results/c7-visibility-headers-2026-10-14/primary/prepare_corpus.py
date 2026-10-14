"""Copy only source Markdown/JSON on the remote host; never its site config."""
import hashlib
import json
from pathlib import Path
import subprocess

from llmwiki.config import BUILTIN_LANGUAGES

task = Path(__file__).resolve().parents[1]
source = Path("/srv/siwen/wenku-fo")
target = task / "corpus"
if target.exists():
    raise SystemExit("isolated corpus already exists")


def manifest(root):
    result = {}
    for name in ("raw", "wiki"):
        for path in sorted((root / name).rglob("*")):
            relative = path.relative_to(root)
            if any(part in {".original", ".git"} for part in relative.parts):
                continue
            if path.is_symlink():
                raise SystemExit("source symlink requires explicit inspection")
            if path.is_file() and path.suffix in {".md", ".json"}:
                result[relative.as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


before = manifest(source)
target.mkdir()
for name in ("raw", "wiki"):
    (target / name).mkdir()
    subprocess.run(["rsync", "-a", "--exclude=.original", "--exclude=.git",
                    "--include=*/", "--include=*.md", "--include=*.json", "--exclude=*",
                    str(source / name) + "/", str(target / name) + "/"], check=True)
assert manifest(source) == before, "source changed during copy"
assert manifest(target) == before, "copy differs from source"
config = {
    "paths": {key: str(target / value) for key, value in {
        "raw": "raw", "wiki": "wiki", "concepts": "wiki/concepts",
        "outputs": "wiki/outputs", "meta": "wiki/_meta"}.items()},
    "entities": {"enabled": False}, "llm": {"max_tokens": 8192},
    "languages": BUILTIN_LANGUAGES,
}
(target / "config.yaml").write_text(json.dumps(config, indent=2) + "\n")
receipt = {"source": str(source), "target": str(target), "copied_files": len(before),
           "source_unchanged": True, "copy_matches": True,
           "aggregate_sha256": hashlib.sha256(json.dumps(before, sort_keys=True).encode()).hexdigest()}
(task / "corpus-copy.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt, indent=2))
