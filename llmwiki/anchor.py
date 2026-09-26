"""
Span anchoring — locate an annotated passage within source content.

Generic "annotation → annotated span" alignment. One pattern, many uses:
  - kepan (科判) tied to a sutra passage
  - paper citation tied to a source paragraph
  - book review's targeted comment tied to a page span
  - journal mark tied to a specific sentence

Pure string algorithm, no LLM dependency. Lives alongside split / sections /
normalize as a structural primitive over markdown text.

Caller contract:
  - ``head`` / ``tail`` are short match keys (caller self-truncates; this
    module does not slice them). Empty ``head`` raises ``ValueError``.
  - Returned ``Anchor`` offsets are indices into the **original** ``content``
    (not the normalized form), so callers can slice, highlight, or scroll
    against the unchanged source without reversing normalization.
  - On no head match, returns ``None``. ``None`` vs ``Anchor`` is the only
    meaningful threshold — no confidence float.

Strategy levels:
  - ``"exact"``      — head + tail both matched; span from head start to tail end.
  - ``"head_only"``  — tail missing or unmatched; span from head start to
    ``len(content)``. Applies when ``tail == ""`` as well.

Frontend mirror:
  ``normalize_text`` exposes the same regex so TS/JS can compute matching
  offsets client-side. The charset in ``normalize_text``'s docstring is
  the ground truth — keep any JS port in lockstep.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Literal

NormalizeLevel = Literal["none", "punct", "punct_spaces"]

# Punctuation charset — CJK + ASCII common marks.
# JS mirror must use this exact set:
#   CJK: ！。，、；：？「」『』（）〔〕【】《》〈〉·・—–―…
#   ASCII: !.,;:?()[]{}"'
# Whitespace for ``punct_spaces`` uses ``\s`` which matches U+3000
# (ideographic space) in both Python (re default is UNICODE for str
# patterns) and JavaScript (ECMAScript \s includes U+3000).
_PUNCT_CHARS = (
    "！。，、；：？"          # CJK terminators / comma / semicolon / colon
    "「」『』"                # CJK quotes
    "（）〔〕【】《》〈〉"    # CJK brackets (full-width)
    "·・—–―…"                # middle dots, em/en/horizontal dashes, ellipsis
    "!.,;:?"                  # ASCII terminators
    "()"                      # ASCII parens
    r"\[\]"                   # ASCII square brackets (escaped for re class)
    "{}"                      # ASCII curly
    "\"'"                     # ASCII quotes
)
_PUNCT_RE = re.compile(f"[{_PUNCT_CHARS}]+")
_PUNCT_SPACES_RE = re.compile(f"[{_PUNCT_CHARS}\\s]+")


@dataclass(frozen=True)
class Anchor:
    """Located span in original content.

    Invariants:
      - ``0 <= start < end <= len(content)``
      - offsets are indices into the ORIGINAL ``content`` (not normalized)
      - ``strategy`` is ``"exact"`` only when both head and a non-empty tail matched
    """

    start: int
    end: int
    strategy: Literal["exact", "head_only"]


def normalize_text(s: str, level: NormalizeLevel = "punct_spaces") -> str:
    """Normalize ``s`` for span matching.

    Levels:
      - ``"none"``          — return as-is.
      - ``"punct"``         — strip CJK + ASCII punctuation (see charset below).
      - ``"punct_spaces"``  — ``punct`` plus all whitespace (``\\s``, which
        includes U+3000 ideographic space in both Python and JavaScript).

    Charset (JS mirror must match exactly):
      CJK punct: ``！。，、；：？「」『』（）〔〕【】《》〈〉·・—–―…``
      ASCII punct: ``!.,;:?()[]{}"'``

    JS equivalents:
      punct        — ``/[！。，、；：？「」『』（）〔〕【】《》〈〉·・—–―…!.,;:?()\\[\\]{}"']+/g``
      punct_spaces — same class plus ``\\s``

    Raises:
      ValueError: on unknown level.
    """
    if level == "none":
        return s
    if level == "punct":
        return _PUNCT_RE.sub("", s)
    if level == "punct_spaces":
        return _PUNCT_SPACES_RE.sub("", s)
    raise ValueError(f"unknown normalize level: {level!r}")


def _normalize_with_map(content: str, level: NormalizeLevel) -> tuple[str, list[int]]:
    """Return ``(normalized, idx_map)`` where ``idx_map[i]`` is the original
    index of ``normalized[i]``. Drops exactly the chars the matching regex
    strips — no heuristics. ``level="none"`` returns an identity map.
    """
    if level == "none":
        return content, list(range(len(content)))
    if level == "punct":
        drop_re = _PUNCT_RE
    elif level == "punct_spaces":
        drop_re = _PUNCT_SPACES_RE
    else:
        raise ValueError(f"unknown normalize level: {level!r}")

    dropped = bytearray(len(content))
    for m in drop_re.finditer(content):
        for k in range(m.start(), m.end()):
            dropped[k] = 1

    out_chars: list[str] = []
    idx_map: list[int] = []
    for i, ch in enumerate(content):
        if not dropped[i]:
            out_chars.append(ch)
            idx_map.append(i)
    return "".join(out_chars), idx_map


def locate_span(
    content: str,
    head: str,
    tail: str = "",
    *,
    normalize: NormalizeLevel = "punct_spaces",
) -> Anchor | None:
    """Locate the passage ``[head .. tail]`` within ``content``.

    Args:
      content: Source text to search. Empty returns ``None``.
      head: Short match key for the passage start (caller self-truncates).
        Normalizing-to-empty (all punct/space) also returns ``None``.
      tail: Short match key for the passage end. Empty or unmatched →
        ``strategy="head_only"``, span extends to ``len(content)``.
      normalize: Which chars to ignore during matching. Offsets in the
        returned Anchor are always into the ORIGINAL ``content``.

    Returns:
      ``Anchor(start, end, strategy)`` on head match, else ``None``.

    Raises:
      ValueError: if ``head`` is the empty string (invalid anchor).

    Notes:
      - ``norm_content.find(norm_head)`` returns the FIRST occurrence.
        Callers whose head is ambiguous should pass more context.
      - Tail search begins at the position just after the matched head,
        so tails that appear before or inside head do not spuriously match.
    """
    if head == "":
        raise ValueError("head must not be empty")
    if content == "":
        return None

    norm_content, idx_map = _normalize_with_map(content, normalize)
    norm_head = normalize_text(head, normalize)
    if norm_head == "":
        return None

    h_start = norm_content.find(norm_head)
    if h_start < 0:
        return None
    h_end = h_start + len(norm_head)

    start_orig = idx_map[h_start]

    if tail:
        norm_tail = normalize_text(tail, normalize)
        if norm_tail:
            t_start = norm_content.find(norm_tail, h_end)
            if t_start >= 0:
                t_end = t_start + len(norm_tail)
                tail_end_orig = idx_map[t_end - 1] + 1
                return Anchor(start_orig, tail_end_orig, "exact")

    return Anchor(start_orig, len(content), "head_only")


_legacy_normalize_text = normalize_text
_legacy_normalize_with_map = _normalize_with_map
_legacy_locate_span = locate_span
_DEFAULT_REGEXES = (_PUNCT_RE, _PUNCT_SPACES_RE)


def normalize_text(s: str, level: NormalizeLevel = "punct_spaces") -> str:
    from . import _native
    if _native.available() and (_PUNCT_RE, _PUNCT_SPACES_RE) == _DEFAULT_REGEXES:
        return _native.utility_transform("normalize_span", {"text": s, "level": level})
    return _legacy_normalize_text(s, level)


def _normalize_with_map(content: str, level: NormalizeLevel) -> tuple[str, list[int]]:
    from . import _native
    if _native.available() and (_PUNCT_RE, _PUNCT_SPACES_RE) == _DEFAULT_REGEXES:
        text, mapping = _native.utility_transform("normalize_span_map", {"text": content, "level": level})
        return text, mapping
    return _legacy_normalize_with_map(content, level)


def locate_span(content: str, head: str, tail: str = "", *,
                normalize: NormalizeLevel = "punct_spaces") -> Anchor | None:
    from . import _native
    if (_native.available() and normalize_text is _DEFAULT_NORMALIZE_TEXT
            and _normalize_with_map is _DEFAULT_NORMALIZE_WITH_MAP
            and (_PUNCT_RE, _PUNCT_SPACES_RE) == _DEFAULT_REGEXES):
        value = _native.utility_transform("locate_span", {
            "content": content, "head": head, "tail": tail, "normalize": normalize})
        return None if value is None else Anchor(**value)
    result = _legacy_locate_span(content, head, tail, normalize=normalize)
    if _native.available():
        _native.python_boundary("locate_span", "live normalization override")
    return result


_DEFAULT_NORMALIZE_TEXT = normalize_text
_DEFAULT_NORMALIZE_WITH_MAP = _normalize_with_map
