"""Primary's adversarial ABI checks; run only on the isolated remote candidate."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile

from llmwiki import _core, _native, taxonomy

assert sys.platform == "linux", "large fixtures belong on the isolated remote"
assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
before_roots = dict(_native._configured_roots)
checks = []


def call(action, args):
    return _native.taxonomy_transform(action, args)


def rejects(name, action, args, fragment):
    # Seed a real success trace. Failed operations must not leave it in place.
    assert call("localize_title", {"title": "English / 經", "lang": "zh"}) == "經"
    try:
        call(action, args)
    except ValueError as exc:
        assert fragment.lower() in str(exc).lower(), (name, str(exc))
        assert not _native.last_trace(), (name, "stale success trace")
        checks.append(name)
    else:
        raise AssertionError((name, "expected ValueError"))


def inventory(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.rglob("*") if p.is_file()}


with tempfile.TemporaryDirectory(prefix="b13-primary-bounds-") as tmp:
    root = Path(tmp)
    concepts = root / "knowledge" / "entries"
    concepts.mkdir(parents=True)
    meta = root / "metadata"
    args = {"concepts_dir": str(concepts), "meta_dir": str(meta), "lang": "en"}
    article = concepts / "a.md"
    for tags in (None, [42], {"bad": True}, 7):
        article.write_text("---\n" + json.dumps({"tags": tags}) + "\n---\n原文\n")
        original = inventory(root)
        loaded = call("load_articles", {"concepts_dir": str(concepts)})
        assert loaded[0]["tags"] == tags, "read DTO must preserve explicit metadata"
        rejects("uncached malformed tags " + repr(tags), "build", args, "tags")
        assert inventory(root) == original
        assert not meta.exists()

    # Each file is below the existing per-file cap; their total exceeds the
    # required 64 MiB action budget. The body is synthetic, never real corpus.
    chunk = b"x" * (1024 * 1024)
    for name in ("a.md", "b.md"):
        with (concepts / name).open("wb") as stream:
            stream.write(b'---\n{"tags": ["topic"]}\n---\n')
            for _ in range(33):
                stream.write(chunk)
    original = inventory(root)
    for action in ("load_articles", "title_map", "build"):
        rejects("aggregate read budget " + action, action, args, "64 MiB")
    assert inventory(root) == original
    assert not meta.exists()

rejects("ABI input budget", "localize_title",
        {"title": "a" * (16 * 1024 * 1024), "lang": "en"}, "16 MiB")
original_config = taxonomy.load_config
failure = ValueError("primary configuration failure")


def failing_config(_base=None):
    raise failure


try:
    taxonomy.load_config = failing_config
    for entry in (taxonomy.load_taxonomy, taxonomy.build_taxonomy):
        call("localize_title", {"title": "English / 經", "lang": "zh"})
        try:
            entry()
        except ValueError as exc:
            assert exc is failure
            assert not _native.last_trace(), (entry.__name__, "stale trace after config failure")
            checks.append(entry.__name__ + " config failure")
        else:
            raise AssertionError("configuration error was swallowed")
finally:
    taxonomy.load_config = original_config
assert _native._configured_roots == before_roots
receipt = {"passed": True, "checks": checks, "no_library_registration": True,
           "extension": _core.__file__,
           "extension_sha256": hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest()}
out = Path(__file__).with_name("bounds-result.json")
out.write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt))
