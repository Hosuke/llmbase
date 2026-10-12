import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / "corpus"
config = {"paths": {key: str(root / value) for key, value in {
    "raw": "raw", "wiki": "wiki", "concepts": "wiki/concepts",
    "outputs": "wiki/outputs", "meta": "wiki/_meta"}.items()},
    "entities": {"enabled": False}, "llm": {"max_tokens": 8192}}
target = root / "config.yaml"
if target.exists():
    raise SystemExit("copied corpus config already exists")
# Deliberately copy no site credential/config file; JSON is also valid YAML.
target.write_text(json.dumps(config, indent=2) + "\n")
print("Created isolated corpus path config without credentials")
