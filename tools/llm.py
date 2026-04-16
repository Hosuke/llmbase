"""LLM interaction layer with automatic fallback across models."""

import os
import time
import logging
from pathlib import Path

from dotenv import load_dotenv
import httpx
from openai import OpenAI

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


def get_client() -> OpenAI:
    global _client
    if _client is None:
        _client = OpenAI(
            api_key=os.getenv("LLMBASE_API_KEY") or os.getenv("OPENAI_API_KEY"),
            base_url=os.getenv("LLMBASE_BASE_URL") or os.getenv("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            timeout=httpx.Timeout(300.0, connect=30.0),
            max_retries=2,
        )
    return _client


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


def _call_llm(messages: list, model: str, max_tokens: int) -> str:
    """Single LLM call with response extraction.

    Handles models with thinking mode: if content is empty but
    reasoning_content exists, uses that as content.
    """
    client = get_client()
    response = client.chat.completions.create(
        model=model,
        messages=messages,
        max_tokens=max_tokens,
    )
    msg = response.choices[0].message
    content = msg.content or ""
    reasoning = getattr(msg, "reasoning_content", None) or ""

    # If content is empty, model might have put everything in reasoning
    if not content.strip() and reasoning:
        content = reasoning

    return content


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


def chat(
    prompt: str,
    system: str = "",
    model: str | None = None,
    max_tokens: int = 16384,
) -> str:
    """Send a prompt with automatic model fallback on failure."""
    if model is None:
        model = get_default_model()

    messages = []
    if system:
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})

    # Try primary model with retries
    models_to_try = [model] + get_fallback_models()

    for i, current_model in enumerate(models_to_try):
        retries = _get_retries(primary=(i == 0))
        for attempt in range(retries):
            try:
                result = _call_llm(messages, current_model, max_tokens)
                if result:
                    if i > 0:
                        logger.warning(f"Primary model failed, used fallback: {current_model}")
                    return result
                # Empty result — retry or try next model
                if attempt < retries - 1:
                    time.sleep(1)
                    continue
            except Exception as e:
                err_msg = str(e)
                if attempt < retries - 1:
                    wait = 2 ** attempt
                    logger.debug(f"{current_model} attempt {attempt+1} failed: {err_msg}, retrying in {wait}s")
                    time.sleep(wait)
                    continue
                if i < len(models_to_try) - 1:
                    logger.warning(f"{current_model} failed ({err_msg}), falling back to {models_to_try[i+1]}")
                    break  # Try next model
                raise  # Last model, re-raise

    return ""


def chat_with_context(
    question: str,
    context_files: list[dict],
    system: str = "",
    model: str | None = None,
    max_tokens: int = 16384,
) -> str:
    """Ask a question with file contents as context."""
    context_parts = []
    for f in context_files:
        context_parts.append(f"## {f['path']}\n\n{f['content']}")
    context_block = "\n\n---\n\n".join(context_parts)

    prompt = f"""Here are the relevant knowledge base articles:

{context_block}

---

Based on the above context, please answer the following question:

{question}"""

    return chat(prompt, system=system, model=model, max_tokens=max_tokens)
