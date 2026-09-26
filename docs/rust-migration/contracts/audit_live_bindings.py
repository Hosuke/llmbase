"""Read-only installed-candidate binding audit; optional pure/temp-only probes."""
import hashlib
import importlib
import inspect
import json
from pathlib import Path
import sys
import tempfile

from llmwiki import _native

TARGETS = {
    "config": ["load_config", "ensure_dirs"],
    "resolve": ["build_aliases", "resolve_link"],
    "atomic": ["atomic_write_json", "atomic_write_text"],
    "sections": ["parse_sections", "make_anchor"],
    "anchor": ["locate_span", "normalize_text"],
    "split": ["split_by_heading"],
    "normalize": ["normalize_heads", "normalize_paragraphs"],
    "taxonomy": ["generate_taxonomy", "load_taxonomy"],
    "xici": ["generate_xici", "get_xici"],
    "entities": ["extract_entities", "get_entities", "_save_entities"],
    "lint": ["lint", "auto_fix", "lint_deep"],
    "worker": ["run_worker", "start_worker_thread"],
}
rows = []
for name, names in TARGETS.items():
    module = importlib.import_module(f"llmwiki.{name}")
    for key in names:
        function = getattr(module, key, None)
        if function is None:
            rows.append({"symbol": f"{name}.{key}", "missing": True})
            continue
        code = getattr(function, "__code__", None)
        path = inspect.getsourcefile(function)
        rows.append({
            "symbol": f"{name}.{key}", "module": function.__module__,
            "signature": str(inspect.signature(function)), "file": path,
            "line": code.co_firstlineno if code else None,
            "code_names": list(code.co_names) if code else [],
            "source_sha256": hashlib.sha256(Path(path).read_bytes()).hexdigest() if path else None,
        })
with tempfile.TemporaryDirectory(prefix="llmbase-binding-audit-") as root:
    probes = []
    for name, args in [
        ("config.load_config", (Path(root),)),
        ("sections.parse_sections", ("## Alpha\nOriginal 𠮷 text.\n",)),
        ("atomic.atomic_write_json", (Path(root) / "artifact.json", {"original": "𠮷"})),
    ]:
        module_name, function_name = name.split(".")
        _native.clear_trace()
        value = getattr(importlib.import_module(f"llmwiki.{module_name}"), function_name)(*args)
        probes.append({"symbol": name, "return_type": type(value).__name__, "native_trace": _native.last_trace()})
print(json.dumps({"python": sys.executable, "backend": _native.backend_info(), "bindings": rows, "probes": probes}, ensure_ascii=False, indent=2))
