"""Stage state — a **derived view** of the log (议 D, v0.7.7).

``StageState`` is the at-a-glance summary operators read; the log is
the source of truth. ``rebuild_state`` is its constructor — walk the
log events, fold them into the dataclass per the contract documented
on the function.

No ``state.json`` on disk. A cached snapshot would risk drifting from
the log on a mid-run crash; instead, callers that want O(1) reads
maintain their own cache, invalidated whenever the log's mtime
changes. The cost of always rebuilding is small — logs are
per-(stage, key), bounded by one run's event count.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from . import log as _log
from .. import _native


# Events written by the driver / lock. ``ctx.log()`` refuses these
# so downstream event streams never masquerade as driver signals.
RESERVED_EVENTS = frozenset({
    "start",
    "ok",
    "failed",
    "partial",
    "interrupted",
    "artifact",
    "meta_update",
})

_TERMINAL_EVENTS = frozenset({"ok", "failed", "partial", "interrupted"})


@dataclass
class StageState:
    """At-a-glance summary of a (stage, key) run history.

    All fields are derived from the log; constructing this object
    directly is only useful in tests.
    """

    stage: str
    key: str
    status: str = "pending"
    """One of ``pending`` (no events), ``running`` (start without
    terminal in the last round), ``ok``, ``failed``, ``partial``,
    ``interrupted``. The type hint is ``str`` deliberately — downstream
    may introduce their own intermediate statuses via custom events
    without upstream churn, though the set above is what ``rebuild``
    produces natively."""
    attempts: int = 0
    """Count of ``start`` events in the log (all rounds)."""
    started_at: str | None = None
    """ISO-8601 timestamp of the *last round's* start event."""
    finished_at: str | None = None
    """ISO-8601 timestamp of the *last round's* terminal event, or
    ``None`` if the last round has not terminated."""
    last_err: str | None = None
    """One-line summary of the last round's failure: ``err`` from a
    ``failed`` event, ``by`` from ``interrupted``, or ``reason`` from
    ``partial``. ``None`` for clean exits."""
    artifacts: list[str] = field(default_factory=list)
    """Union of every ``artifact`` event's ``path`` across **all**
    rounds, sorted, deduped. Intended as a GC hint — operator may
    ``rmtree`` these paths once they are sure the stage's downstream
    consumers no longer need them."""
    meta: dict[str, Any] = field(default_factory=dict)
    """Last round's cumulative meta: starts from that round's
    ``meta_init`` (from its ``start`` event) and overlays each
    ``meta_update`` event in the same round, last-write-wins. Earlier
    rounds' meta is discarded."""


def rebuild_state(base_dir: Path | str, stage: str, key: str) -> StageState:
    """Reconstruct :class:`StageState` from the log. Contract:

    1. **Last terminal wins within the last round.** A round is the
       slice of events from a ``start`` up to (but not including) the
       next ``start``. ``status`` is the name of the last terminal
       event in the last round; if the last round has none, status is
       ``running``. Earlier rounds' terminals are historical and do
       not set status.

    2. **``attempts`` = count of ``start`` events.** Every
       ``run_stage`` entry writes one ``start``, regardless of how it
       ended.

    3. **Multiple runs → last round.** A log of ``start → ok → start
       → failed`` yields ``status="failed"``, ``attempts=2``,
       ``last_err`` from the second round.

    4. **``started_at`` / ``finished_at``** come from the last round's
       ``start`` and terminal respectively.

    5. **``artifacts``** are unioned across all rounds, set-deduped,
       then sorted — a path that appears twice shows up once.

    6. **``meta``** uses only the last round's events.
       ``meta_init`` from that round's ``start`` seeds it, then every
       ``meta_update`` event overlays last-write-wins. Earlier rounds'
       meta is discarded entirely.

    7. **Torn / malformed lines are skipped** (delegated to
       :func:`tools.pipeline.log.iter_events`).

    This contract is part of llmwiki's stable API. Changes to rebuild
    semantics require a major version bump.
    """
    if _native.available():
        args = {"stage": stage, "key": key}
        if _log.iter_events is not _log._DEFAULT_ITER_EVENTS:
            _native.clear_trace()
            args["events"] = list(_log.iter_events(Path(base_dir), stage, key))
        values = _native.pipeline_dispatch("compat_pipeline_state", args, base_dir)
        return StageState(**values)
    events = list(_log.iter_events(Path(base_dir), stage, key))
    state = StageState(stage=stage, key=key)
    if not events:
        return state

    start_indices = [i for i, e in enumerate(events) if e.get("event") == "start"]
    state.attempts = len(start_indices)
    if not start_indices:
        # Log has events but no ``start`` — a malformed history.
        # Return pending with artifacts still collected (the only
        # salvageable signal).
        state.artifacts = _collect_artifacts(events)
        return state

    last_round = events[start_indices[-1]:]
    state.started_at = last_round[0].get("ts")

    # Last terminal in the last round.
    terminal: dict | None = None
    for e in reversed(last_round):
        if e.get("event") in _TERMINAL_EVENTS:
            terminal = e
            break

    if terminal is None:
        state.status = "running"
    else:
        state.status = terminal["event"]
        state.finished_at = terminal.get("ts")
        if terminal["event"] == "failed":
            state.last_err = terminal.get("err")
        elif terminal["event"] == "interrupted":
            by = terminal.get("by", "unknown")
            state.last_err = f"interrupted: {by}"
        elif terminal["event"] == "partial":
            reason = terminal.get("reason", "")
            state.last_err = f"partial: {reason}" if reason else "partial"

    state.artifacts = _collect_artifacts(events)
    state.meta = _collect_last_round_meta(last_round)
    return state


# ── internals ─────────────────────────────────────────────────────────

def _collect_artifacts(events: list[dict]) -> list[str]:
    seen: set[str] = set()
    for e in events:
        if e.get("event") == "artifact":
            p = e.get("path")
            if isinstance(p, str):
                seen.add(p)
    return sorted(seen)


_META_EVENT_RESERVED_KEYS = frozenset({"event", "ts"})


def _collect_last_round_meta(last_round: list[dict]) -> dict:
    # Start event seeds meta with its ``meta_init`` payload. Guard
    # against a malformed ``start`` whose ``meta_init`` is not a dict
    # — e.g. a hand-edited log line with ``"meta_init": "whoops"`` —
    # so rebuild degrades to empty-meta instead of raising.
    start_event = last_round[0]
    seed = start_event.get("meta_init")
    meta: dict[str, Any] = dict(seed) if isinstance(seed, dict) else {}
    for e in last_round[1:]:
        if e.get("event") != "meta_update":
            continue
        for k, v in e.items():
            if k in _META_EVENT_RESERVED_KEYS:
                continue
            meta[k] = v
    return meta
