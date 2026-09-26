"""LLM interaction layer with automatic fallback across models."""

import os
import time
import logging
import math
from contextvars import ContextVar
from dataclasses import dataclass, field
from pathlib import Path

from dotenv import load_dotenv
import httpx
from openai import OpenAI

from . import _native

logger = logging.getLogger("llmbase.llm")


def _load_env() -> Path | None:
    """Resolve .env in a way that works for both editable and PyPI installs.

    Search order (first hit wins; shell exports always beat all of them
    because ``override=False``):

    1. ``$LLMBASE_ENV_FILE`` — explicit path override.
    2. ``$PWD/.env`` — but only when the CWD also contains a ``config.yaml``
       (i.e. this is an actual KB project root). Gating on ``config.yaml``
       stops a hostile ``.env`` dropped into an unrelated working directory
       from redirecting ``LLMBASE_BASE_URL`` while shell-exported keys leak
       to attacker infra.
    3. ``~/.config/llmbase/.env`` — user-level default, install-agnostic.
    4. ``<package_parent>/.env`` — legacy path that only worked for
       ``pip install -e .`` source checkouts.

    The old single-location lookup silently failed under pipx/PyPI installs
    because ``__file__`` resolves inside ``site-packages/`` (issue #4).
    """
    def _safe_load(path: Path) -> bool:
        # python-dotenv's return value is unreliable as a success signal —
        # it returns False for valid empty/comment-only files as well as
        # unreadable ones. Since we already gate on ``is_file()`` above,
        # only a raised exception should count as a load failure here.
        try:
            load_dotenv(path, override=False)
        except Exception as exc:
            logger.warning("Failed to read .env at %s: %s", path, exc)
            return False
        return True

    def _safe_resolve(path: Path) -> Path | None:
        try:
            return path.resolve()
        except (OSError, RuntimeError):
            return None

    # Explicit override is authoritative: if LLMBASE_ENV_FILE is set, we load
    # that exact file or nothing — never fall through to CWD/XDG/package. A
    # typo in the override must not silently pick up a different .env that
    # could redirect requests to attacker infra.
    override = os.environ.get("LLMBASE_ENV_FILE")
    if override:
        explicit = Path(override).expanduser()
        try:
            is_file = explicit.is_file()
        except OSError:
            is_file = False
        resolved = _safe_resolve(explicit) if is_file else None
        if resolved is not None and _safe_load(resolved):
            logger.info("Loaded .env from %s (via LLMBASE_ENV_FILE)", resolved)
            return resolved
        logger.error(
            "LLMBASE_ENV_FILE=%r is not a readable file; refusing to fall back "
            "to other .env locations.", override,
        )
        return None

    candidates: list[Path] = []
    try:
        cwd = Path.cwd()
    except (OSError, RuntimeError):
        cwd = None
    # Trust CWD/.env only when CWD looks like a real llmbase KB project —
    # config.yaml alone is too weak a marker (many unrelated projects ship
    # one). Require llmbase-canonical keys under ``paths:`` in the config.
    if cwd is not None and _is_llmbase_project(cwd):
        candidates.append(cwd / ".env")
    try:
        home = Path.home()
    except (OSError, RuntimeError):
        home = None
    if home is not None:
        candidates.append(home / ".config" / "llmbase" / ".env")
    candidates.append(Path(__file__).resolve().parent.parent / ".env")

    seen: set[Path] = set()
    for p in candidates:
        resolved = _safe_resolve(p)
        if resolved is None or resolved in seen:
            continue
        seen.add(resolved)
        try:
            is_file = resolved.is_file()
        except OSError:
            continue
        if is_file and _safe_load(resolved):
            logger.info("Loaded .env from %s", resolved)
            return resolved
    logger.info(
        "No .env file found (checked LLMBASE_ENV_FILE, CWD, ~/.config/llmbase, package dir); "
        "using shell environment only"
    )
    return None


def _is_llmbase_project(root: Path) -> bool:
    """Heuristic: does *root*/config.yaml declare llmbase-canonical paths?

    Requires at least one of ``paths.concepts`` / ``paths.wiki`` /
    ``paths.raw`` — the keys llmbase actually consumes. This keeps stray
    ``config.yaml`` files in unrelated repos (gatsby, hugo, k8s manifests,
    …) from being treated as KB roots and pulling in a hostile ``.env``.
    """
    cfg_path = root / "config.yaml"
    try:
        if not cfg_path.is_file():
            return False
        import yaml
        with cfg_path.open("r", encoding="utf-8") as f:
            data = yaml.safe_load(f)
    except Exception:
        return False
    if not isinstance(data, dict):
        return False
    paths = data.get("paths")
    if not isinstance(paths, dict):
        return False
    return any(k in paths for k in ("concepts", "wiki", "raw"))


_ENV_SOURCE = _load_env()

_client = None
_default_client = None
_DEFAULT_OPENAI = OpenAI
_call_trace: ContextVar[dict | None] = ContextVar("llmwiki_model_call_trace", default=None)


def _get_float_env(name: str, default: float) -> float:
    """Read *name* as a positive float, logging and falling back on bad input."""
    raw = os.getenv(name, "")
    if not raw:
        return default
    try:
        val = float(raw)
        if not math.isfinite(val) or val <= 0:
            raise ValueError("must be > 0")
        return val
    except ValueError:
        logger.warning(f"Invalid {name}={raw!r}, using default {default}")
        return default


def get_client(api_key: str | None = None) -> OpenAI:
    """Return an OpenAI client.

    When *api_key* is ``None`` (the common path), returns the
    module-level singleton built from env (``LLMBASE_API_KEY`` /
    ``OPENAI_API_KEY``). Cached across calls.

    When *api_key* is a string, returns a **fresh, un-cached** client
    built with that key. This is the per-request override path used by
    ``/api/ask`` (``X-LLM-Key`` header). Never cached — mixing a
    caller-supplied key into the singleton would leak it across
    subsequent requests with different identity.
    """
    # HTTP timeouts — overridable via env for local Ollama / slow GPUs
    # where the 300 s default isn't enough (issue #6). CONNECT covers
    # the initial TCP/TLS handshake; READ covers per-call wall time.
    read_timeout = _get_float_env("LLMBASE_HTTP_TIMEOUT", 300.0)
    connect_timeout = _get_float_env("LLMBASE_HTTP_CONNECT_TIMEOUT", 30.0)
    base_url = os.getenv("LLMBASE_BASE_URL") or os.getenv("OPENAI_BASE_URL", "https://api.openai.com/v1")

    if api_key is not None:
        return OpenAI(
            api_key=api_key,
            base_url=base_url,
            timeout=httpx.Timeout(read_timeout, connect=connect_timeout),
            max_retries=2,
        )

    global _client, _default_client
    if _client is None:
        _client = OpenAI(
            api_key=os.getenv("LLMBASE_API_KEY") or os.getenv("OPENAI_API_KEY"),
            base_url=base_url,
            timeout=httpx.Timeout(read_timeout, connect=connect_timeout),
            max_retries=2,
        )
        _default_client = _client
    return _client


_DEFAULT_GET_CLIENT = get_client


def get_default_model() -> str:
    return os.getenv("LLMBASE_MODEL", "gpt-4o")


def get_fallback_models() -> list[str]:
    """Get fallback model list from env. Comma-separated; empty = no fallback.

    Since 0.5.0: an empty/unset ``LLMBASE_FALLBACK_MODELS`` means *no*
    fallback — only the primary model is retried. Earlier versions
    auto-generated a fallback chain (e.g. gpt-4o → gpt-4o-mini), which
    silently broke aggregator deployments where the API token only had
    rights to the primary model. Downstream that wants fallback must now
    set the env var explicitly, e.g.::

        LLMBASE_FALLBACK_MODELS=gpt-4o-mini,gpt-3.5-turbo
    """
    fallbacks = os.getenv("LLMBASE_FALLBACK_MODELS", "")
    if not fallbacks:
        return []
    return [m.strip() for m in fallbacks.split(",") if m.strip()]


def _get_retries(primary: bool) -> int:
    """Per-model retry budget. Configurable via env, with sane defaults."""
    if primary:
        env_key, default = "LLMBASE_PRIMARY_RETRIES", 3
    else:
        env_key, default = "LLMBASE_FALLBACK_RETRIES", 1
    raw = os.getenv(env_key, "")
    if not raw:
        return default
    try:
        return max(1, int(raw))
    except ValueError:
        logger.warning(f"Invalid {env_key}={raw!r}, using default {default}")
        return default


def _redact_key(text: str, api_key: str | None) -> str:
    """Strip any literal occurrence of *api_key* from *text* (for log lines).

    Defence-in-depth: OpenAI/httpx error messages can echo the key back
    (``"Incorrect API key provided: sk-****"`` or similar). Since we
    never write raw error strings without going through a logger, this
    replacement catches both per-request and environment keys before
    they land in stdout / syslog / structured log collectors."""
    if not isinstance(text, str):
        return text
    for key in (api_key, os.getenv("LLMBASE_API_KEY"), os.getenv("OPENAI_API_KEY")):
        if key:
            text = text.replace(key, "[redacted]")
    return text


def last_call_trace() -> dict | None:
    """Execution boundary of this context's latest LLM call, without credentials."""
    trace = _call_trace.get()
    return dict(trace) if trace is not None else None


def _call_llm(
    messages: list,
    model: str,
    max_tokens: int,
    api_key: str | None = None,
) -> tuple[str, str | None, dict]:
    """Single LLM call with response extraction.

    Returns ``(content, finish_reason, usage)`` where:

    - ``content`` — text, with reasoning-content fallback for models
      whose message ``content`` is empty but ``reasoning_content`` is
      populated (see below).
    - ``finish_reason`` — from ``choices[0].finish_reason``; may be
      ``None`` when the provider omits it.
    - ``usage`` — flat dict ``{prompt_tokens, completion_tokens,
      reasoning_tokens, total_tokens}``. ``reasoning_tokens`` is pulled
      from OpenAI's nested ``usage.completion_tokens_details`` when
      present; absent/zero otherwise. Keys may be missing if the
      provider response omits them — callers must treat ``usage.get(k)``
      as ``None``-safe.

    Reasoning-mode fallback: some providers (DeepSeek, o-series via
    certain aggregators) return empty ``message.content`` with the
    full body in ``message.reasoning_content``. We copy reasoning into
    content so callers receive the actual answer. This predates v0.7.8
    and is preserved for compatibility.

    *api_key*: when provided, uses a fresh un-cached client for this
    call (see ``get_client``)."""
    custom_client = (
        get_client is not _DEFAULT_GET_CLIENT
        or OpenAI is not _DEFAULT_OPENAI
        or (_client is not None and _client is not _default_client)
    )
    if not custom_client and _native.available():
        # Configuration is read for every invocation. The secret travels as a
        # separate in-memory argument, never profile/operation JSON or storage.
        _call_trace.set({"core": "rust", "operation": "model_complete", "status": "pending"})
        try:
            response = _native.model_complete(
                {
                    "base_url": os.getenv("LLMBASE_BASE_URL") or os.getenv("OPENAI_BASE_URL", "https://api.openai.com/v1"),
                    "model": model,
                    "request_timeout_seconds": _get_float_env("LLMBASE_HTTP_TIMEOUT", 300.0),
                    "connect_timeout_seconds": _get_float_env("LLMBASE_HTTP_CONNECT_TIMEOUT", 30.0),
                },
                {"messages": messages, "model": None, "max_tokens": max_tokens, "temperature": None},
                api_key=api_key if api_key is not None else (os.getenv("LLMBASE_API_KEY") or os.getenv("OPENAI_API_KEY")),
            )
        except Exception:
            _call_trace.set({"core": "rust", "operation": "model_complete", "status": "failed"})
            raise
        _call_trace.set(_native.last_trace())
        return response["content"], response["finish_reason"], response["usage"]
    _native.clear_trace()
    _call_trace.set({"core": "python", "operation": "model_complete", "boundary": "client_override" if custom_client else "unbuilt_development_checkout"})
    client = get_client(api_key=api_key)
    response = client.chat.completions.create(
        model=model,
        messages=messages,
        max_tokens=max_tokens,
    )
    choice = response.choices[0]
    msg = choice.message
    content = msg.content or ""
    reasoning = getattr(msg, "reasoning_content", None) or ""

    # If content is empty, model might have put everything in reasoning
    if not content.strip() and reasoning:
        content = reasoning

    finish_reason = getattr(choice, "finish_reason", None)
    usage = _extract_usage(getattr(response, "usage", None))
    return content, finish_reason, usage


_DEFAULT_CALL_LLM = _call_llm


def _extract_usage(raw_usage) -> dict:
    """Flatten a provider usage object into a plain dict.

    OpenAI SDK returns a ``CompletionUsage`` with a nested
    ``completion_tokens_details.reasoning_tokens``. We flatten that
    one nested field (the one siwen's 11-hour post-mortem needed) and
    leave other provider-specific nesting alone. Absent fields become
    ``None``; callers should treat the dict as best-effort.

    Accepts both attribute-style objects (OpenAI SDK pydantic models)
    and dict-style payloads (some aggregators / older client versions
    hand back plain dicts) — the Codex v0.7.8 review flagged that a
    strict `getattr` path silently coerced the whole dict to ``None``
    on a dict-shaped response.
    """
    if raw_usage is None:
        return {
            "prompt_tokens": None,
            "completion_tokens": None,
            "reasoning_tokens": None,
            "total_tokens": None,
        }
    out = {
        "prompt_tokens": _usage_get(raw_usage, "prompt_tokens"),
        "completion_tokens": _usage_get(raw_usage, "completion_tokens"),
        "total_tokens": _usage_get(raw_usage, "total_tokens"),
        "reasoning_tokens": None,
    }
    details = _usage_get(raw_usage, "completion_tokens_details")
    if details is not None:
        rt = _usage_get(details, "reasoning_tokens")
        if rt is not None:
            out["reasoning_tokens"] = rt
    return out


def _usage_get(obj, key: str):
    """Attribute-or-key lookup on a usage-shaped object.

    OpenAI pydantic models expose fields as attributes; some aggregator
    responses and older clients surface them as dict entries. Tolerate
    both without forcing callers to know which they got."""
    if isinstance(obj, dict):
        return obj.get(key)
    return getattr(obj, key, None)


def extract_json(text: str) -> str:
    """Extract valid JSON from mixed thinking+content LLM output.

    Call this explicitly when you expect JSON — not applied globally.
    Searches from the END of the text to find the last valid JSON
    block (thinking comes first, JSON output last).
    Returns the original text if no valid JSON found.
    """
    import json as _json

    stripped = text.strip()

    # Quick validation if it already looks like JSON
    if stripped.startswith(("[", "{")):
        try:
            _json.loads(stripped)
            return stripped
        except _json.JSONDecodeError:
            pass  # Might be incomplete, try extraction below

    # Search from the end — try whichever closing bracket is rightmost first
    pairs = [("[", "]"), ("{", "}")]
    pairs.sort(key=lambda p: text.rfind(p[1]), reverse=True)

    for start_char, end_char in pairs:
        end_pos = text.rfind(end_char)
        if end_pos == -1:
            continue
        # Find the matching opening bracket before it
        start_pos = text.rfind(start_char, 0, end_pos)
        if start_pos == -1:
            continue
        candidate = text[start_pos:end_pos + 1]
        try:
            _json.loads(candidate)
            return candidate
        except _json.JSONDecodeError:
            # Try progressively earlier opening brackets
            while True:
                start_pos = text.rfind(start_char, 0, start_pos)
                if start_pos == -1:
                    break
                candidate = text[start_pos:end_pos + 1]
                try:
                    _json.loads(candidate)
                    return candidate
                except _json.JSONDecodeError:
                    continue

    return text  # No valid JSON found


@dataclass
class LLMMeta:
    """Metadata from a :func:`chat_with_meta` call (议 5-甲, v0.7.8).

    Collected across the full retry + fallback attempt tree; fields
    describe the *final* accepted call (not an intermediate failure).
    ``attempts`` is the only cross-attempt accumulator.

    The field set is OpenAI-compatible by design (the native HTTP
    adapter and overridden SDK clients use that wire format). If
    llmbase ever grows a non-OpenAI-compat provider, the dataclass
    can gain aliases additively; no field is load-bearing for the
    driver in 议 D — downstream picks what it needs.
    """

    model: str
    """The model that actually produced ``content`` — may differ from
    the caller's request when a fallback model succeeded."""

    finish_reason: str | None
    """Provider's ``choices[0].finish_reason`` for the accepted call.
    Canonical OpenAI values: ``stop`` / ``length`` / ``content_filter``
    / ``tool_calls`` / ``function_call``. ``None`` when the provider
    omits it (some aggregators do)."""

    truncated: bool
    """Convenience: ``finish_reason == "length"``. True means the model
    hit ``max_tokens`` before completing — the returned text is a
    prefix of the intended output. **This is the field the 11-hour
    wenguan post-mortem needed** — the ratio check that replaced it
    did not distinguish length-cut from stop."""

    usage: dict = field(default_factory=dict)
    """Flat token-usage dict: ``{prompt_tokens, completion_tokens,
    reasoning_tokens, total_tokens}``. Any key may be ``None`` when
    the provider omits it. ``reasoning_tokens`` is flattened from
    OpenAI's ``completion_tokens_details`` so reasoning-model budget
    tuning doesn't require nested dict walks."""

    attempts: int = 0
    """Total number of ``_call_llm`` invocations across all models
    before (and including) the successful one. ``1`` on a clean first
    try; ``> 1`` when retries or fallback kicked in."""


def chat_with_meta(
    prompt: str,
    system: str = "",
    model: str | None = None,
    max_tokens: int = 16384,
    api_key: str | None = None,
) -> tuple[str, LLMMeta]:
    """Like :func:`chat` but also returns response metadata (v0.7.8).

    Identical retry + fallback semantics as :func:`chat`: primary
    model gets ``LLMBASE_PRIMARY_RETRIES`` attempts (default 3), each
    fallback gets ``LLMBASE_FALLBACK_RETRIES`` (default 1); empty
    results count as soft failures and retry.

    Returns ``(text, meta)``. On the exhausted-all-retries-with-empty
    path, returns ``("", LLMMeta(...))`` with ``meta.finish_reason``
    reflecting the last observed response (matching :func:`chat`'s
    silent-empty contract).

    **Truncation is NOT raised as an exception.** When the provider
    says ``finish_reason="length"``, the prefix text is returned and
    ``meta.truncated`` is ``True``; the caller decides whether to
    ``mark_partial``, shrink the chunk, or accept the prefix. This
    keeps the primitive honest — detection is surfaced, policy stays
    downstream (议 5-甲 design, 2026-04-21).

    *api_key*: per-call override (v0.7.4). None → module singleton.
    """
    _native.clear_trace()
    _call_trace.set(None)
    if model is None:
        model = get_default_model()

    messages = []
    if system:
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})

    models_to_try = [model] + get_fallback_models()

    # Track the last observed response so the empty-exhaustion path
    # still carries meaningful meta.finish_reason / usage / model.
    attempts_total = 0
    last_meta_model = model
    last_finish_reason: str | None = None
    last_usage: dict = {}

    for i, current_model in enumerate(models_to_try):
        retries = _get_retries(primary=(i == 0))
        for attempt in range(retries):
            attempts_total += 1
            try:
                if _call_llm is not _DEFAULT_CALL_LLM:
                    _native.clear_trace()
                    _call_trace.set({"core": "python", "operation": "model_complete", "boundary": "callable_override"})
                content, finish_reason, usage = _call_llm(
                    messages, current_model, max_tokens, api_key=api_key
                )
                # Record regardless of whether we accept this response —
                # the empty-exhaustion path surfaces the last seen state.
                last_meta_model = current_model
                last_finish_reason = finish_reason
                last_usage = usage
                if content:
                    if i > 0:
                        logger.warning(
                            f"Primary model failed, used fallback: {current_model}"
                        )
                    meta = LLMMeta(
                        model=current_model,
                        finish_reason=finish_reason,
                        truncated=(finish_reason == "length"),
                        usage=usage,
                        attempts=attempts_total,
                    )
                    return content, meta
                # Empty result — retry or try next model
                if attempt < retries - 1:
                    time.sleep(1)
                    continue
            except Exception as e:
                err_msg = _redact_key(str(e), api_key)
                if attempt < retries - 1:
                    wait = 2 ** attempt
                    logger.debug(
                        f"{current_model} attempt {attempt+1} failed: "
                        f"{err_msg}, retrying in {wait}s"
                    )
                    time.sleep(wait)
                    continue
                if i < len(models_to_try) - 1:
                    logger.warning(
                        f"{current_model} failed ({err_msg}), "
                        f"falling back to {models_to_try[i+1]}"
                    )
                    break  # Try next model
                # Last model — re-raise via a redacted wrapper so the
                # key doesn't land in the caller's traceback / HTTP 500.
                if api_key or err_msg != str(e):
                    raise RuntimeError(err_msg) from None
                raise  # Last model, re-raise

    # Exhausted all models with empty content. Preserve chat()'s silent
    # contract: return "" plus meta reflecting the last observed state.
    return "", LLMMeta(
        model=last_meta_model,
        finish_reason=last_finish_reason,
        truncated=(last_finish_reason == "length"),
        usage=last_usage,
        attempts=attempts_total,
    )


def chat(
    prompt: str,
    system: str = "",
    model: str | None = None,
    max_tokens: int = 16384,
    api_key: str | None = None,
) -> str:
    """Send a prompt with automatic model fallback on failure.

    Thin wrapper over :func:`chat_with_meta` that drops the metadata.
    v0.7.x callers who don't need ``finish_reason`` / ``usage``
    continue working unchanged; new callers that need length-cut
    detection use :func:`chat_with_meta` directly.

    *api_key*: per-call override (v0.7.4). None → module singleton.
    """
    text, _ = chat_with_meta(
        prompt,
        system=system,
        model=model,
        max_tokens=max_tokens,
        api_key=api_key,
    )
    return text


def reasoning_budget(
    max_tokens: int,
    tokens_per_char: float,
    safety: float = 0.8,
) -> int:
    """Safe input chunk size (chars) for a given output token budget (议 5-乙, v0.7.8).

    Pure arithmetic — no model table, no implicit defaults. Inverts the
    question "how many input characters can I feed the LLM before its
    output budget overflows?":

        safe_chars = int(max_tokens * safety / tokens_per_char)

    Parameters
    ----------
    max_tokens : int
        Output token budget for the LLM call (the ``max_tokens`` you'll
        pass to :func:`chat` / :func:`chat_with_meta`).
    tokens_per_char : float
        Expected **output tokens produced per input character**. A
        combined metric: model token density × prompt output-expansion
        factor. Measure empirically from real runs::

            tokens_per_char = mean(
                meta.usage["completion_tokens"] / len(chunk_text)
                for each (chunk_text, meta) in recent_sample
            )

        Rough baselines (verify against your own prompts):
        - CJK reasoning-model output with mild normalization: ~15-20
        - CJK non-reasoning LLM, translation/restatement: ~2-4
        - ASCII summarization: ~1-3

        siwen's 2026-04-21 post-mortem measured 17 for wenguan/taixu.
    safety : float, default 0.8
        Fraction of ``max_tokens`` to target. Leaves headroom for
        output variance — some chunks expand more than the mean.
        Values in (0, 1]; 0.7-0.9 typical.

    Returns
    -------
    int
        Safe input chunk size in characters. Always ``>= 1`` when the
        inputs are valid (callers slicing exactly at this size may
        still overflow on unlucky variance; that's what ``safety``
        buys you — tune it against real logs).

    Raises
    ------
    ValueError
        If any input is non-positive, or ``safety`` is outside
        ``(0, 1]``.

    Examples
    --------
    siwen's post-mortem numbers::

        >>> reasoning_budget(max_tokens=32000, tokens_per_char=17)
        1505

    Tighter safety for high-variance outputs::

        >>> reasoning_budget(32000, 17, safety=0.6)
        1129
    """
    if max_tokens <= 0:
        raise ValueError(f"max_tokens must be > 0, got {max_tokens!r}")
    if tokens_per_char <= 0:
        raise ValueError(
            f"tokens_per_char must be > 0, got {tokens_per_char!r}"
        )
    if not (0 < safety <= 1):
        raise ValueError(f"safety must be in (0, 1], got {safety!r}")
    # Guard the whole float pipeline: any of these can fail on
    # pathological inputs (Codex v0.7.8 review, two rounds):
    #   - ``int * float`` raises OverflowError for an int that won't
    #     fit in float64 (e.g. ``10**309``) — before any result value
    #     is produced to inspect.
    #   - When the multiplication survives, the final quotient can
    #     still be ``+/-inf`` or ``NaN`` for in-range-but-extreme
    #     floats; ``int(inf)`` / ``int(nan)`` then raise
    #     OverflowError / ValueError.
    #   - NaN inputs slip past the ``<= 0`` comparisons.
    # Consolidate all of these into a single ValueError so the
    # docstring's "ValueError on bad input" promise holds for every
    # axis.
    import math
    try:
        scaled = max_tokens * safety / tokens_per_char
    except (OverflowError, ValueError) as exc:
        raise ValueError(
            f"reasoning_budget: arithmetic failed for "
            f"max_tokens={max_tokens!r}, safety={safety!r}, "
            f"tokens_per_char={tokens_per_char!r} ({exc})"
        ) from None
    if not math.isfinite(scaled):
        raise ValueError(
            f"reasoning_budget: non-finite result from "
            f"max_tokens={max_tokens!r}, safety={safety!r}, "
            f"tokens_per_char={tokens_per_char!r}"
        )
    chars = int(scaled)
    return max(chars, 1)


def strip_surrogates(s: str) -> str:
    """Replace lone UTF-16 surrogates in *s* with ``?``.

    The OpenAI client serialises payloads with strict UTF-8, which rejects
    isolated surrogate code points (U+D800–U+DFFF). They sneak in via
    upstream HTML/PDF ingest where mojibake decode left half a surrogate
    pair, then sit in concept files until a deep query stitches them into
    a prompt — at which point the request crashes with::

        'utf-8' codec can't encode character '\\udce7': surrogates not allowed

    Implementation: ``encode("utf-8", "replace")`` emits ``b"?"`` (0x3F)
    for each lone surrogate — not U+FFFD — and the subsequent decode is a
    no-op on those ASCII bytes. Valid text round-trips unchanged.
    """
    if not isinstance(s, str):
        return s
    return s.encode("utf-8", "replace").decode("utf-8", "replace")


def chat_with_context(
    question: str,
    context_files: list[dict],
    system: str = "",
    model: str | None = None,
    max_tokens: int = 16384,
    api_key: str | None = None,
) -> str:
    """Ask a question with file contents as context.

    *api_key*: per-call override (v0.7.4). None → module singleton.
    """
    context_parts = []
    for f in context_files:
        path = strip_surrogates(str(f.get("path", "")))
        content = strip_surrogates(str(f.get("content", "")))
        context_parts.append(f"## {path}\n\n{content}")
    context_block = "\n\n---\n\n".join(context_parts)
    safe_question = strip_surrogates(question)

    prompt = f"""Here are the relevant knowledge base articles:

{context_block}

---

Based on the above context, please answer the following question:

{safe_question}"""

    return chat(prompt, system=system, model=model, max_tokens=max_tokens, api_key=api_key)
