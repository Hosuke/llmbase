"""Grow only a test copy's body to reproduce the reporter's cumulative size."""
import json
from pathlib import Path
import subprocess

from probe import inventory

task = Path(__file__).resolve().parents[1]
source, target = task / "corpus", task / "corpus-large"
if target.exists():
    raise SystemExit("grown test corpus already exists")
before = inventory(source)
target.mkdir()
subprocess.run(["rsync", "-a", str(source) + "/", str(target) + "/"], check=True)
config = json.loads((target / "config.yaml").read_text())
config["paths"] = {key: str(target / Path(value).relative_to(source))
                   for key, value in config["paths"].items()}
(target / "config.yaml").write_text(json.dumps(config, indent=2) + "\n")
desired_bytes = 83_391_106
growth = desired_bytes - before["markdown_bytes"]
assert growth > 0
raw = min((target / "raw").rglob("*.md"), key=lambda path: path.stat().st_size)
old_size = raw.stat().st_size
# Keep every individual file under the old full-document limit, isolating the
# cumulative-limit regression. Sparse zero-filled body growth is valid UTF-8.
assert old_size + growth < 64 * 1024 * 1024
with raw.open("r+b") as stream:
    stream.truncate(old_size + growth)
after = inventory(target)
assert after["markdown_bytes"] == desired_bytes
assert inventory(source) == before
receipt = {"kind": "synthetic body growth on a real copied corpus, not the latest site corpus",
           "original": before, "grown": after, "source_unchanged": True,
           "grown_path": raw.relative_to(target).as_posix(),
           "old_file_bytes": old_size, "new_file_bytes": old_size + growth}
(task / "growth.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt, indent=2))
