"""Event hooks — extension point for downstream projects.

Core modules emit events (ingested, compiled, etc.) at key lifecycle points.
Downstream projects register callbacks to react — e.g. pushing state to a
remote sync table, triggering a CI pipeline, sending a notification.

Usage from a downstream project (e.g. 华藏阁 startup)::

    from tools.hooks import register
    register("ingested", lambda source, work_id, **kw: sync.push(source, work_id))
    register("compiled", lambda source, work_id, **kw: sync.mark(source, work_id))

Hooks are best-effort: exceptions are logged but never propagate to the
caller, so a broken hook cannot disrupt core operations.
"""

import logging
from typing import Any, Callable

logger = logging.getLogger("llmbase.hooks")

_registry: dict[str, list[Callable[..., Any]]] = {}


def register(event: str, callback: Callable[..., Any]) -> None:
    """Register a callback for an event. May be called multiple times."""
    _registry.setdefault(event, []).append(callback)


def emit(event: str, **kwargs: Any) -> None:
    """Fire all callbacks registered for *event*.

    Each callback receives the kwargs as keyword arguments.
    Failures are logged at warning level and swallowed.
    """
    for cb in _registry.get(event, []):
        try:
            cb(**kwargs)
        except Exception as e:
            logger.warning("[hooks] %s handler %s failed: %s", event, cb, e)


def clear(event: str | None = None) -> None:
    """Remove all callbacks (or for a specific event). Useful for testing."""
    if event is None:
        _registry.clear()
    else:
        _registry.pop(event, None)
