"""Flat section splitter — 议 E (v0.7.6).

Upstream primitive that cuts a Markdown body into a flat list of
sections at a chosen ATX heading level. **Parse only** — no domain
heuristics (no "single-book 弁", no "merge tiny items", no "strip
trailing next title"). Downstream corpora (siwen 太虛, CBETA, ...)
compose their own ``split_<corpus>.py`` on top of this parse output.

Contrast with ``tools/sections.py``: that module produces a *nested*
tree with stable anchors for TOC / section-API use. This one produces
a *flat* list at a single level, sized for pipeline consumption (one
section = one chunk = one LLM call).

Reuses ``sections._iter_headings`` for fence-aware heading iteration
— no duplicate regex / fence state machine.

Example::

    from llmwiki.split import split_by_heading

    body = "## 緒論\\n第一段。\\n## 第一章\\n內文。\\n### 子\\n子內\\n"
    sections = split_by_heading(body, level=2)
    # [Section(level=2, title='緒論', ..., content='第一段。'),
    #  Section(level=2, title='第一章', ..., content='內文。\\n### 子\\n子內')]

    # Preface (anything before sections[0].start) is NOT in the return —
    # downstream takes it explicitly when needed:
    preface = body[:sections[0].start] if sections else body
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from .sections import _iter_headings

# Match any of the three Markdown-relevant line terminators. ``\r``-only
# (classic Mac) inputs are rare but still valid and must not silently
# collapse the section body into ``header_line`` — Codex v0.7.6 flagged
# the ``body.find("\n", ...)``-only variant as MEDIUM data loss.
_LINE_END_RE = re.compile(r"\r\n|\r|\n")


@dataclass
class Section:
    """One flat section produced by :func:`split_by_heading`.

    ``body[start:end]`` yields the heading line + content + any deeper
    sub-headings that sit inside this section (same subtree semantics
    as ``tools/sections.py``). ``content`` is the convenience slice —
    ``body[first_newline_after_start + 1 : end]`` with outer whitespace
    stripped.
    """

    level: int           #: ATX depth of the heading (matches the ``level`` arg)
    title: str           #: Heading text, trimmed — no ``##`` prefix, no trailing ``#+``
    header_line: str     #: Raw heading line (``## 第一章``), no trailing newline
    start: int           #: Char offset into ``body`` where the heading line begins
    end: int             #: Char offset where the next same-or-higher heading starts (or ``len(body)``)
    content: str         #: ``body`` between the heading line and ``end``, stripped


def split_by_heading(body: str, level: int) -> list[Section]:
    """Split *body* at every ATX heading of depth *level*, return flat list.

    Rules (议 E spec):

    - Only headings of exactly *level* depth are split points.
    - Next same-or-higher-level heading (level ≤ *level*, i.e. smaller
      or equal number of ``#``) ends the section. Deeper headings
      (level > *level*) stay inside the section's ``content``.
    - Fenced code blocks are skipped (inherited from
      ``sections._iter_headings``).
    - CommonMark §4.2 indent tolerance (0-3 leading spaces).
    - Text before the first matched heading — the "preface" — is NOT
      in the returned list. Downstream: ``body[:sections[0].start]``.
    - No heading of *level* found ⇒ returns ``[]``.

    :raises ValueError: *level* not in 1..6.
    """
    if not (1 <= level <= 6):
        raise ValueError(f"level must be 1..6, got {level!r}")

    heads = list(_iter_headings(body))  # [(lvl, char_offset, title), ...]
    if not heads:
        return []

    sections: list[Section] = []
    for i, (lvl, off, title) in enumerate(heads):
        if lvl != level:
            continue
        # End = offset of next heading at level <= target (i.e. same or
        # higher in hierarchy); else end of body.
        end = len(body)
        for lvl2, off2, _ in heads[i + 1:]:
            if lvl2 <= level:
                end = off2
                break
        # Split the header line off from content via the first line
        # terminator after the heading offset. Handles ``\r\n``, ``\r``,
        # ``\n``, and sections whose heading is the last line of the
        # body (no trailing terminator).
        m = _LINE_END_RE.search(body, off, end)
        if m is None:
            header_line = body[off:end]
            content_start = end
        else:
            header_line = body[off:m.start()]
            content_start = m.end()
        content = body[content_start:end].strip()
        sections.append(Section(
            level=lvl,
            title=title,
            header_line=header_line,
            start=off,
            end=end,
            content=content,
        ))
    return sections


_legacy_split_by_heading = split_by_heading
_DEFAULT_ITER_HEADINGS = _iter_headings


def split_by_heading(body: str, level: int) -> list[Section]:
    from . import _native
    if _native.available() and _iter_headings is _DEFAULT_ITER_HEADINGS:
        return [Section(**row) for row in _native.utility_transform("split", {"body": body, "level": level})]
    result = _legacy_split_by_heading(body, level)
    if _native.available():
        _native.python_boundary("split_by_heading", "live heading iterator override")
    return result
