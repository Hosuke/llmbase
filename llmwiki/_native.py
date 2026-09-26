"""Native transport and capability checks for the Python compatibility package.

Source checkouts may temporarily run legacy Python code before the extension is
built. A native candidate installation, or LLMBASE_REQUIRE_NATIVE=1, must never
fall back when the extension is missing or cannot load.
"""

from __future__ import annotations

from contextvars import ContextVar
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
from typing import Any


class NativeCapabilityError(RuntimeError):
    """The installed profile requires a native extension which is unavailable."""


_last_trace: ContextVar[dict | None] = ContextVar("llmwiki_native_trace", default=None)
_active_writer_config: ContextVar[dict | None] = ContextVar("llmwiki_active_writer_config", default=None)
_configured_roots: dict[tuple[str, str], dict] = {}


def _installed_candidate_requires_native() -> bool:
    try:
        files = importlib.metadata.distribution("llmwiki").files or ()
    except importlib.metadata.PackageNotFoundError:
        return False
    return any(str(path).startswith("llmwiki/_core.") for path in files)


def _extension(required: bool = False):
    required = required or os.environ.get("LLMBASE_REQUIRE_NATIVE", "").lower() in {"1", "true", "yes"}
    try:
        return importlib.import_module("llmwiki._core")
    except ModuleNotFoundError as exc:
        if exc.name != "llmwiki._core":
            raise NativeCapabilityError("native extension dependency is unavailable") from exc
        if required or _installed_candidate_requires_native():
            raise NativeCapabilityError(
                "llmwiki._core is required but unavailable; install a matching native candidate wheel"
            ) from exc
        return None
    except ImportError as exc:
        raise NativeCapabilityError("llmwiki._core could not load; legacy fallback is disabled") from exc


def available() -> bool:
    """False is allowed only for an unbuilt development checkout, never a candidate."""
    return _extension() is not None


def backend_info() -> dict:
    extension = _extension()
    if extension is None:
        return {"core": "python", "native": False, "reason": "unbuilt development checkout"}
    result = json.loads(extension.backend_info())
    result.update(native=True, extension_path=extension.__file__)
    return result


def dispatch(operation: str, args: dict | None = None, base_dir: Path | str | None = None,
             *, library_id: str | None = None, paths: dict | None = None) -> Any:
    """Dispatch locally through the native Application; errors never trigger fallback."""
    _last_trace.set(None)
    extension = _extension(required=True)
    base = Path(base_dir).resolve() if base_dir is not None else Path.cwd().resolve()
    # Explicit library selection is per call. The default is stable within one data root.
    library = library_id or os.environ.get("LLMBASE_LIBRARY_ID", "default")
    arguments = [str(base), library, operation, json.dumps(args or {}, ensure_ascii=False)]
    if paths is not None:
        arguments.append(json.dumps({key: str(value) for key, value in paths.items()}, ensure_ascii=False))
    result = json.loads(extension.dispatch(*arguments))
    _last_trace.set(result.get("trace") if isinstance(result, dict) else None)
    return result


def last_trace() -> dict | None:
    """Last real native operation receipt in this Python context; not a startup marker."""
    trace = _last_trace.get()
    return dict(trace) if trace is not None else None


def clear_trace() -> None:
    """Mark an explicit Python callback boundary without reusing an earlier native receipt."""
    _last_trace.set(None)


def utility_transform(action: str, args: dict):
    clear_trace()
    result = json.loads(_extension(required=True).utility_transform(
        action, json.dumps(args, ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["value"]


def resolve_transform(action: str, args: dict):
    clear_trace()
    result = json.loads(_extension(required=True).resolve_transform(
        action, json.dumps(args, ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["value"]


def config_read_run(base, decoder, defaults, path_coercer):
    """Native IO/sequence, with an opaque live Python format document."""
    clear_trace()
    try:
        value, receipt = _extension(required=True).config_read_run(
            base, decoder, defaults, path_coercer)
        trace = json.loads(receipt)
    except BaseException:
        clear_trace()
        raise
    _last_trace.set(trace)
    return value


def config_default_paths(base):
    clear_trace()
    try:
        value, receipt = _extension(required=True).config_default_paths(base)
        trace = json.loads(receipt)
    except BaseException:
        clear_trace()
        raise
    _last_trace.set(trace)
    return value


def config_ensure_dirs(cfg, path_coercer):
    clear_trace()
    try:
        receipt = _extension(required=True).config_ensure_dirs(cfg, path_coercer)
        trace = json.loads(receipt)
    except BaseException:
        clear_trace()
        raise
    _last_trace.set(trace)


def normalize_heads_run(body: str, callback=None):
    """Native document scanner with an explicit, live Python title matcher."""
    clear_trace()
    try:
        result = json.loads(_extension(required=True).normalize_heads_run(body, callback))
    except BaseException:
        # A callback can itself perform a native call before failing.
        clear_trace()
        raise
    _last_trace.set(result["trace"])
    return result["value"]


def taxonomy_transform(action: str, args: dict) -> Any:
    """Run a trusted local taxonomy read/value helper; no library is opened."""
    clear_trace()
    result = json.loads(_extension(required=True).taxonomy_transform(
        action, json.dumps(args, ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["value"]


def maintenance_transform(action: str, args: dict) -> Any:
    """Run a pure native maintenance helper; no library is opened or inferred."""
    clear_trace()
    result = json.loads(_extension(required=True).maintenance_transform(
        action, json.dumps(args, ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["value"]


def run_maintenance(action: str, args: dict, callback) -> Any:
    """Host explicit live Python callbacks in the Rust-owned maintenance sequence."""
    clear_trace()
    extension = _extension(required=True)
    def host(stage: str, payload_json: str) -> str:
        clear_trace()
        value = callback(stage, json.loads(payload_json))
        return json.dumps(value, ensure_ascii=False)
    try:
        result = json.loads(extension.maintenance_run(
            action, json.dumps(args, ensure_ascii=False), host))
    except BaseException:
        # A failed run keeps no receipt: neither a stale trace nor a success trace.
        clear_trace()
        raise
    _last_trace.set(result.get("trace"))
    return result["value"]


def register_config(cfg: dict, base_dir=None) -> None:
    """Remember only roots obtained from an actual explicit config load."""
    base = Path(base_dir if base_dir is not None else cfg["base_dir"]).resolve()
    library = os.environ.get("LLMBASE_LIBRARY_ID", "default")
    _configured_roots[(str(base), library)] = {"base": base, "library": library, "cfg": cfg}


def _atomic_config(path: Path):
    def matches(record):
        return any(path.is_relative_to(Path(value).resolve()) for key, value
                   in record["cfg"].get("paths", {}).items()
                   if key in {"raw", "concepts", "outputs", "meta"} and value)
    active = _active_writer_config.get()
    if active is not None and matches(active):
        return active
    matches_found = [record for record in list(_configured_roots.values()) if matches(record)]
    if len(matches_found) > 1:
        raise NativeCapabilityError("atomic path matches multiple configured libraries; use an explicit writer context")
    return matches_found[0] if matches_found else None


def atomic_write(path, content: bytes):
    clear_trace()
    path = Path(path)
    resolved = path.parent.resolve() / path.name
    record = _atomic_config(resolved)
    result = json.loads(_extension(required=True).atomic_write_at(str(resolved), content,
        None if record is None else str(record["base"]),
        "default" if record is None else record["library"],
        None if record is None else json.dumps(record["cfg"].get("paths", {}), ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["value"]


def python_boundary(operation: str, callback: str) -> None:
    """Record a real external callback, without attributing its work to Rust."""
    _last_trace.set({"core": "python", "operation": operation, "callback": callback})


def fetch_cancellation():
    """A thread-safe native cancellation handle for an in-flight web fetch."""
    return _extension(required=True).FetchCancellation()


def fetch_url(url: str, *, options: dict | None = None, cancellation=None) -> dict:
    clear_trace()
    result = json.loads(_extension(required=True).fetch_url(
        url, json.dumps(options or {}), cancellation))
    _last_trace.set(result["trace"])
    response = result["response"]
    response["content"] = bytes(response.pop("body_bytes"))
    return response


def validate_fetch_url(url: str) -> None:
    clear_trace()
    _extension(required=True).validate_fetch_url(url)


def model_complete(config: dict, request: dict, *, api_key: str | None = None) -> dict:
    """One actual native HTTP attempt; the compatibility caller owns retry policy."""
    clear_trace()
    extension = _extension(required=True)
    result = json.loads(extension.model_complete(
        json.dumps(config, ensure_ascii=False), json.dumps(request, ensure_ascii=False), api_key))
    _last_trace.set(result.get("trace"))
    return result["response"]


def compile_transform(action: str, args: dict) -> Any:
    """Run a pure native compile helper without opening or creating a library."""
    clear_trace()
    extension = _extension(required=True)
    result = json.loads(extension.compile_transform(action, json.dumps(args, ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["value"]


def pipeline_dispatch(operation: str, args: dict, base_dir: Path | str) -> Any:
    """Native pipeline storage at an explicit root, including a not-yet-created root."""
    clear_trace()
    extension = _extension(required=True)
    result = json.loads(extension.pipeline_dispatch(
        str(Path(base_dir).resolve()), os.environ.get("LLMBASE_LIBRARY_ID", "default"),
        operation, json.dumps(args, ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["result"]


def stage_lease_acquire(base_dir: Path | str, stage: str, key: str):
    """Acquire an actual per-stage kernel lease; None means a live holder is busy."""
    clear_trace()
    extension = _extension(required=True)
    lease = extension.stage_lease_acquire(str(Path(base_dir).resolve()), stage, key)
    if lease is not None:
        _last_trace.set(json.loads(lease.receipt()))
    return lease


def query_transform(action: str, args: dict) -> Any:
    """Run a pure query helper in Rust; it does not initialize storage."""
    clear_trace()
    extension = _extension(required=True)
    result = json.loads(extension.query_transform(action, json.dumps(args, ensure_ascii=False)))
    _last_trace.set(result.get("trace"))
    return result["value"]


def run_query(request: dict, callback, base_dir: Path | str | None, *, paths: dict | None = None) -> Any:
    """Host explicit live Python callbacks in the Rust-owned query sequence."""
    clear_trace()
    extension = _extension(required=True)
    base = Path(base_dir).resolve() if base_dir is not None else Path.cwd().resolve()
    library = os.environ.get("LLMBASE_LIBRARY_ID", "default")
    def host(stage: str, payload_json: str) -> str:
        clear_trace()
        value = callback(stage, json.loads(payload_json))
        return json.dumps(value, ensure_ascii=False)
    paths_json = None if paths is None else json.dumps(
        {key: str(value) for key, value in paths.items()}, ensure_ascii=False)
    result = json.loads(extension.query_run(
        str(base), library, json.dumps(request, ensure_ascii=False), host, paths_json))
    _last_trace.set(result.get("trace"))
    return result["value"]


def with_writer(base_dir: Path | str | None, callback, *, library_id: str | None = None,
                paths: dict | None = None):
    """Hold the shared native writer lease while invoking the actual Python handler."""
    clear_trace()
    extension = _extension()
    if extension is None:
        return callback()  # Explicit unbuilt-checkout compatibility only.
    base = Path(base_dir).resolve() if base_dir is not None else Path.cwd().resolve()
    library = library_id or os.environ.get("LLMBASE_LIBRARY_ID", "default")
    paths_json = None if paths is None else json.dumps(
        {key: str(value) for key, value in paths.items()}, ensure_ascii=False)
    from .config import load_config
    cfg = {"paths": paths} if paths is not None else load_config(base)
    def scoped():
        token = _active_writer_config.set({"base": base, "library": library, "cfg": cfg})
        try:
            return callback()
        finally:
            _active_writer_config.reset(token)
    return extension.with_writer(str(base), library, scoped, paths_json)
