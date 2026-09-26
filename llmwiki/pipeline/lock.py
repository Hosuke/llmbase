"""Per-stage kernel lease with compatible PID metadata and TTL semantics.

``StageLock`` provides mutual exclusion for a long-running stage such
as ``wenguan`` (siwen's flagship: 62 太虛 books × hours-long LLM pass),
where a double-run would burn an LLM quota, corrupt a chunk cache, and
serialize back into the sync stage as two conflicting partial writes.

Native candidate: a stable ``.lock.kernel`` inode holds an exclusive Rust flock
for the whole StageLock lifetime. A kernel-v1 pidfile with an available lease is
stale even if its former PID has been reused. Removing a live holder's pidfile
does not grant a second lease. Different stage/key pairs remain independent.
Unbuilt source checkouts and unmarked pre-migration pidfiles retain the legacy
protocol below; PID reuse cannot be resolved from an unmarked pidfile alone.

Legacy metadata/recovery protocol:

- **O_EXCL creation** — acquire is atomic. If two processes race on
  an empty lock slot, only one creates the pidfile; the other sees
  ``FileExistsError`` and falls through to the stale-check path.

- **Stale ↔ live discrimination** by three independent signals:

  1. pidfile missing / malformed / empty / missing required fields
     → stale (treat as recoverable crash).
  2. pidfile's ``host`` differs from current host → stale (we cannot
     verify the PID on another machine; assume its caller is dead).
     **Single-host scope.** This policy is only safe when the
     ``base_dir`` filesystem is dedicated to one machine. On shared
     storage (NFS / CIFS / clustered FS) with multiple writing hosts,
     a legitimately live holder on host X would be classified stale
     by host Y and the lock broken — mutual exclusion violated. If
     you need multi-host, build a separate advisory layer (e.g.
     ``flock(2)`` over NFSv4, or an external mutex like etcd /
     consul) and wrap ``tools.pipeline`` underneath.
  3. pidfile's PID is not alive (``os.kill(pid, 0)`` raises
     ``ProcessLookupError``) → stale.

- **PID-reuse window (known limitation).** If a stale holder's PID
  has been recycled by the kernel and is now held by an unrelated
  live process, ``os.kill(pid, 0)`` reports alive and this lock
  misclassifies the dead holder as live. The pidfile's
  ``started_at`` is recorded for operator inspection but not used
  as an identity token; doing so portably requires ``psutil`` or
  per-OS procfs code the stdlib does not expose cleanly. Siwen's
  usage (processes that die and restart on the same-minute
  timescale, inside a 32k–99k PID space) makes a collision
  vanishingly rare; operators who hit it fall back to
  ``force_break()`` after ``ps -fp <pid>``. Downstream projects
  running longer-dormant stale pidfiles in high-PID-churn
  environments should layer their own identity verification.

- **TTL is informational, not enforcing.** A live-PID on the same host
  is *never* broken by ``acquire()``, even past its recorded TTL —
  that's a hung process and may still hold real resources (an open
  LLM session, a locked file handle). Operators diagnose
  (``ps -fp <pid>``) before calling ``force_break()``.

- **Interrupted event** — when a stale lock is broken, ``acquire``
  first appends ``{"event": "interrupted", "by": "stale_lock_break",
  "prior_pid": ..., "prior_host": ..., "prior_started_at": ...}`` to
  the log. This guarantees ``rebuild_state`` resolves a SIGKILL'd
  prior run to ``status="interrupted"`` instead of stuck ``"running"``.

This module is package-internal. Downstream goes through
``run_stage()``, which acquires + releases as part of its
contextmanager contract.
"""

from __future__ import annotations

import errno
import fcntl
import json
import os
import socket
import time
from datetime import datetime, timezone
from pathlib import Path

from . import log as _log
from .. import _native


def _parse_holder(raw: str) -> dict | None:
    """Parse pidfile content into a holder dict, or ``None`` when
    malformed / missing required fields. Does not touch the
    filesystem — separate from reading so that ``acquire()`` can
    distinguish "file missing" (transient race) from "file present
    but unparseable" (truly stale)."""
    if not raw.strip():
        return None  # empty file — mid-write crash (pre-v0.7.7 layout)
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return None
    if not isinstance(data, dict):
        return None
    if not all(k in data for k in ("pid", "host", "started_at")):
        return None
    return data


_PIDFILE_MISSING = "missing"
_PIDFILE_OK = "ok"
_PIDFILE_MALFORMED = "malformed"


def _read_pidfile_text(path: Path) -> tuple[str, str | None]:
    """Read pidfile bytes with a tri-state outcome.

    Returns a ``(status, content)`` tuple where ``status`` is one of:

    - ``"missing"`` — file doesn't exist. Safe to treat as "no holder";
      caller can retry ``os.link`` without logging anything.
    - ``"ok"`` — file read and UTF-8-decoded successfully; ``content``
      is the decoded string.
    - ``"malformed"`` — file exists but its bytes are not valid UTF-8
      (torn multi-byte, binary trash). Safe to treat as stale.

    Other ``OSError`` subclasses (``PermissionError``, I/O errors)
    propagate to the caller. Codex HIGH v0.7.7 round 10: swallowing
    permission errors as ``None`` caused ``acquire()`` to misclassify
    a live-but-unreadable pidfile as stale and log a spurious
    ``interrupted`` event. Caller must decide: ``acquire()`` should
    treat the ambiguity as busy (don't break on unreadable);
    ``release()`` should treat it as "can't verify, don't unlink".
    """
    try:
        raw_bytes = path.read_bytes()
    except FileNotFoundError:
        return (_PIDFILE_MISSING, None)
    # All other OSError subclasses propagate — the caller decides.
    try:
        return (_PIDFILE_OK, raw_bytes.decode("utf-8"))
    except UnicodeDecodeError:
        return (_PIDFILE_MALFORMED, None)


class StageLock:
    """Per-(stage, key) exclusive file lock."""

    def __init__(self, base_dir: Path | str, stage: str, key: str):
        self._base_dir = Path(base_dir)
        self._stage = stage
        self._key = key
        self._path = _log.log_path(base_dir, stage, key).with_suffix(".lock")
        self._acquired = False
        self._native_lease = None

    @property
    def path(self) -> Path:
        return self._path

    def acquire(self, ttl: int = 3600) -> bool:
        """Acquire the per-stage lease and publish compatible holder metadata."""
        if _native.available():
            if self._native_lease is not None:
                return False
            lease = _native.stage_lease_acquire(self._base_dir, self._stage, self._key)
            if lease is None:
                return False
            self._native_lease = lease
            try:
                if self._acquire_pidfile(ttl):
                    return True
            except BaseException:
                lease.release()
                self._native_lease = None
                raise
            lease.release()
            self._native_lease = None
            return False
        return self._acquire_pidfile(ttl)

    def _acquire_pidfile(self, ttl: int = 3600) -> bool:
        """Attempt to take the lock.

        Returns ``True`` on success. Returns ``False`` only when a
        *live* holder (same host, alive PID) already owns it — the
        caller should surface a ``StageBusyError`` and bail. If the
        holder is stale (crashed, cross-host, malformed pidfile), the
        method breaks the stale lock, logs an ``interrupted`` event,
        and retries.

        **Atomic publication via tempfile + os.link.** Writing content
        into a fresh ``O_CREAT | O_EXCL`` fd would leave a
        zero-byte window during which another acquirer could read
        the empty pidfile, classify it as stale, and break *our own
        new lock* before we finished writing — Codex HIGH v0.7.7
        round 2. Instead we fully populate a tempfile first, then
        ``os.link`` it into place: linking fails atomically with
        ``FileExistsError`` if anyone already holds the lock slot,
        and the published pidfile is never observable as incomplete.
        """
        self._path.parent.mkdir(parents=True, exist_ok=True)
        payload = json.dumps({
            "pid": os.getpid(),
            "host": socket.gethostname(),
            "started_at": datetime.now(timezone.utc).isoformat(),
            "ttl": ttl,
            "key": self._key,
            **({"lock_protocol": "kernel-v1"} if self._native_lease is not None else {}),
        })
        candidate = self._path.with_name(
            f"{self._path.name}.cand-{os.getpid()}-{time.monotonic_ns()}"
        )
        # Fully populate the candidate BEFORE attempting publication.
        with open(candidate, "w", encoding="utf-8") as f:
            f.write(payload)
            f.flush()
            os.fsync(f.fileno())

        try:
            # Bounded retry. Each iteration either wins the link,
            # finds a live holder (return False), or breaks a stale
            # holder (continue). Converges because each break leaves
            # the slot empty (or held by someone else — no busy-loop).
            for _ in range(8):
                try:
                    os.link(str(candidate), str(self._path))
                except FileExistsError:
                    # File existed at link-time. A concurrent
                    # ``release()`` may have unlinked it since, which
                    # we must distinguish from "truly stale holder".
                    # Reading the pidfile and getting FileNotFoundError
                    # now means it's a transient race — retry the link
                    # without logging ``interrupted`` (Codex HIGH
                    # v0.7.7 round 5). Only malformed content (file
                    # exists but unparseable) warrants a stale-break.
                    try:
                        status, raw = _read_pidfile_text(self._path)
                    except OSError:
                        # PermissionError or other I/O failure — we
                        # cannot classify this pidfile's staleness
                        # safely. Return busy rather than risk
                        # breaking a live holder. Codex v0.7.7 r10.
                        return False
                    if status == _PIDFILE_MISSING:
                        continue  # release race — retry link
                    if status == _PIDFILE_MALFORMED:
                        # File exists but undecodable → truly stale;
                        # _break_stale will re-verify under its mutex.
                        self._break_stale(None)
                        continue
                    holder = _parse_holder(raw)  # type: ignore[arg-type]
                    if holder is None or self._is_stale(holder):
                        self._break_stale(holder)
                        continue
                    return False  # live holder — caller backs off
                self._acquired = True
                return True
            # Exhausted retries — treat as busy (caller may retry).
            return False
        finally:
            # After a successful link, the lockfile is an independent
            # directory entry pointing to the same inode; unlinking
            # the candidate does not affect it. Unconditional cleanup.
            try:
                candidate.unlink()
            except FileNotFoundError:
                pass

    def release(self) -> None:
        """Release holder metadata and the kernel lease; safe when never acquired."""
        lease = self._native_lease
        if lease is not None and not lease.owned_here():
            # A forked child must not unlink its parent's pidfile or explicitly
            # unlock the shared open file description.
            lease.release()
            self._native_lease = None
            self._acquired = False
            return
        try:
            self._release_pidfile()
        finally:
            if lease is not None:
                lease.release()
                self._native_lease = None

    def _release_pidfile(self) -> None:
        """Release the lock if we hold it. Idempotent: safe to call
        after a failed ``acquire`` (no-op when not held)."""
        if not self._acquired:
            return
        # Only unlink if the file is still ours (guard against a
        # concurrent operator ``force_break`` that replaced the
        # pidfile mid-run).
        holder = self._read_holder()
        if holder is not None and holder.get("pid") == os.getpid() \
                and holder.get("host") == socket.gethostname():
            try:
                self._path.unlink()
            except FileNotFoundError:
                pass
        self._acquired = False

    def force_break(self) -> None:
        """Operator escape hatch: remove the pidfile regardless of
        liveness. Use only after confirming (``ps -fp <pid>``) that
        the holder is truly dead or intentionally being preempted.

        For native holders this removes only the metadata; it cannot release
        another live process's kernel lease or permit overlapping stage work.

        Does **not** log an ``interrupted`` event — forced breaks are
        operator actions outside the normal recovery protocol. If you
        want that recorded, append to the log yourself after calling.
        """
        try:
            self._path.unlink()
        except FileNotFoundError:
            pass

    # ── internals ─────────────────────────────────────────────────────

    def _read_holder(self) -> dict | None:
        """Return parsed pidfile, or ``None`` when the file is
        missing, malformed, or unreadable. Only ``release()`` uses
        this — it's permissive: if we can't verify the pidfile is
        ours, the safer behavior is to not unlink at all (the
        guard on matching pid/host below achieves this when holder
        is ``None``)."""
        try:
            status, raw = _read_pidfile_text(self._path)
        except OSError:
            return None
        if status != _PIDFILE_OK:
            return None
        return _parse_holder(raw)  # type: ignore[arg-type]

    def _is_stale(self, holder: dict | None) -> bool:
        if holder is None:
            return True
        if holder.get("lock_protocol") == "kernel-v1" and self._native_lease is not None:
            # We already hold the exclusive kernel lease. A former native
            # holder is gone even if its PID was reused by an unrelated process.
            return True
        if holder.get("host") != socket.gethostname():
            return True  # can't verify PID across hosts
        pid = holder.get("pid")
        # ``type(pid) is int`` (not ``isinstance``) rejects ``bool``
        # — ``True`` / ``False`` are ``int`` subclasses and would
        # otherwise survive the check, letting a malformed pidfile
        # of ``{"pid": true, ...}`` probe PID 1 (always alive → init)
        # and lock the slot forever (Codex HIGH v0.7.7 round 3).
        if type(pid) is not int or pid <= 0:
            return True
        try:
            os.kill(pid, 0)  # existence probe; no signal delivered
        except ProcessLookupError:
            return True
        except PermissionError:
            # Process exists but owned by another user — still alive.
            return False
        except OverflowError:
            # ``pid`` larger than C ``pid_t`` (e.g. a malicious or
            # corrupt pidfile with ``2**100``). Treat as stale — the
            # PID cannot possibly be live.
            return True
        except OSError as e:
            if e.errno == errno.ESRCH:
                return True
            return False
        return False

    def _break_stale(self, holder: dict | None) -> None:
        """Serialize stale-breaks under a ``fcntl.flock`` mutex;
        re-read the pidfile inside the mutex and only unlink if still
        stale.

        **Why a separate breaker flock?** Codex HIGH v0.7.7 round 6:
        two racers (A, B) both read the same stale pidfile H0 outside
        any mutex. B wins the rename-to-claim race, a fresh acquirer
        C then links a new live pidfile H1 into place. A, still
        executing, renames ``self._path`` — but ``self._path`` now
        holds H1, not H0. A's rename kicks the LIVE lock out, and
        A's later O_EXCL succeeds too. Two processes think they hold
        the lock — mutual-exclusion gone.

        The flock breaker makes stale-breaks sequential: only one
        process is inside ``_break_stale`` at a time. Inside, we
        re-read the pidfile; if another breaker already cleared the
        stale and a fresh acquirer won, the re-read shows a live
        holder and we do nothing. The invariant is: **no pidfile is
        unlinked here unless it is re-verified stale inside the
        breaker mutex**. ``fcntl.flock`` releases automatically on
        process death, so a crashed breaker cannot hang the system.

        Known limitation: an operator calling ``force_break()``
        concurrently with a programmatic stale-break is outside this
        protocol's envelope. ``force_break`` is explicitly documented
        as a dangerous operator tool; operators should ensure the
        slot is quiesced before using it.
        """
        breaker_path = self._path.with_name(self._path.name + ".breaker")
        breaker_path.parent.mkdir(parents=True, exist_ok=True)
        # ``a`` so the file is created if missing; we never write to
        # it. Closing the fd drops the lock automatically.
        with open(breaker_path, "a") as bf:
            # Blocking LOCK_EX: wait for any concurrent breaker to
            # finish. Codex v0.7.7 round 7 flagged ``LOCK_NB`` as a
            # liveness hazard — bounded outer retries combined with
            # an always-busy breaker lock could surface a false
            # ``StageBusyError`` even though the only holder was dead.
            # Blocking is safe because ``_break_stale``'s critical
            # section is bounded (a read + an append + an unlink; no
            # network I/O, no LLM calls) and ``fcntl.flock`` releases
            # automatically on process death.
            fcntl.flock(bf.fileno(), fcntl.LOCK_EX)

            try:
                # Re-read pidfile *inside* the mutex. This is the
                # verification: only unlink if the current content is
                # still stale (i.e. no one freshly acquired since our
                # outer-loop read).
                try:
                    status, raw = _read_pidfile_text(self._path)
                except OSError:
                    # Permission / I/O error on the pidfile — cannot
                    # safely classify. Back off rather than risk
                    # breaking a live holder whose file we merely
                    # cannot read. Operator uses force_break.
                    return
                if status == _PIDFILE_MISSING:
                    return  # another breaker finished, or released
                if status == _PIDFILE_MALFORMED:
                    current: dict | None = None
                else:
                    current = _parse_holder(raw)  # type: ignore[arg-type]
                if current is not None and not self._is_stale(current):
                    # A fresh acquirer has already claimed the slot
                    # since our outer-loop classification. Do NOT
                    # unlink — that would violate mutual exclusion.
                    return

                # Still stale. Log ``interrupted`` BEFORE unlink so
                # the event lands in the dead run's round (v0.7.7
                # round 2 ordering invariant), then remove the file.
                prior = current if current is not None else (holder or {})
                try:
                    _log.append(self._base_dir, self._stage, self._key, {
                        "event": "interrupted",
                        "by": "stale_lock_break",
                        "prior_pid": prior.get("pid"),
                        "prior_host": prior.get("host"),
                        "prior_started_at": prior.get("started_at"),
                    })
                except OSError:
                    # Log append failure is non-fatal for recovery —
                    # we still want the stale lock gone.
                    # ``rebuild_state`` will then see no interrupted
                    # event and report status="running", which
                    # operators can resolve manually.
                    pass
                try:
                    self._path.unlink()
                except FileNotFoundError:
                    pass
            finally:
                fcntl.flock(bf.fileno(), fcntl.LOCK_UN)
