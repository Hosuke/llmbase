"""
Markdown pre-process normalizers for classical-text corpora.

Two independent, CommonMark-safe passes. Both skip fenced code blocks,
ATX headings, list items, blockquotes, table rows, thematic breaks, and
HTML block starters — only body-paragraph lines are touched.

- ``normalize_paragraphs(body)`` — merge a line into its predecessor when
  the predecessor does not end with a sentence terminator. 古籍 OCR /
  web-scrape often splits sentences on visual column breaks; this
  undoes that without flattening real paragraph boundaries (blank lines
  are preserved verbatim). Terminator set and the closing-wrapper set
  that may follow a terminator (``」』）]...``) are module-level
  constants so downstream corpora can extend them.

- ``normalize_heads(body, rules)`` — rewrite ATX heading levels when the
  title matches a regex in ``rules`` (first match wins). No default
  rule pack: upstream does not know the corpus's heading conventions.
  Downstream (e.g. siwen's 太虛 pack) supplies a list such as
  ``[{"pattern": r"^第[一二三四五六七八九十百千]+[章編篇卷]", "level": 2}]``.

Empirical baseline (siwen 太虛 62 books, 2026-04): ~1,500 head re-levels,
~14,000 paragraph merges — these two passes replace two ~200-line
siwen-local post-process scripts.

Known limitations (intentional — target corpora are classical-Chinese
OCR where these constructs are vanishingly rare; callers with mixed
content should insert blank-line separators as a workaround):

- HTML block types 1-5 (``<script>``, ``<!-- comment -->``, ``<?pi?>``,
  ``<!DOCTYPE>``, ``<![CDATA[``) close only on blank lines in this
  implementation, not on their CommonMark-specified close markers
  (``</script>``, ``-->``, etc.). Following prose may be treated as
  container continuation until the next blank line.
- Multi-line link reference definitions (title on the following line)
  are not detected; only single-line definitions are protected.
- Regex patterns in ``normalize_heads`` rules are compiled without any
  complexity or ReDoS check — rules come from trusted downstream
  config (not user input); callers must not plumb untrusted input
  through them.
"""

from __future__ import annotations

import re
from typing import TypedDict

from .sections import _FENCE_CLOSE_RE, _FENCE_OPEN_RE, _HEADING_RE

# Sentence terminators — a line whose last non-wrapper character is NOT in
# this set is considered unfinished and merged with the next line.
# Default covers CJK full-width (。！？；) and ASCII (.!?;). Override to
# add (or remove — e.g. some corpora prefer merging on ASCII ``.`` too
# aggressively) as needed.
SENTENCE_TERMINATORS = "。！？；.!?;"

# Closing wrappers: trailing brackets / quotes that may follow a
# terminator (``說：『道』。`` ends at the ``。`` even with a ``』`` after).
# Stripped from the right before checking the terminator.
CLOSING_WRAPPERS = "）」』】》〉)]}\"'"


class HeadRule(TypedDict):
    """One pattern→level rule for ``normalize_heads``.

    ``pattern`` is a Python regex applied (via ``re.search``) to the
    heading's title text (the portion after ``#+`` and the required
    space, with any trailing ``#+`` closer stripped). ``level`` is the
    target ATX depth, 1-6.
    """

    pattern: str
    level: int


# Rewrite regex: splits an ATX heading line into indent / opener / sep /
# title / optional-closer so we can swap the opener without disturbing
# the rest. Mirrors CommonMark §4.2 (0-3 space indent, 1-6 ``#``, required
# space, optional trailing ``#+`` preceded by whitespace).
_HEADING_REWRITE_RE = re.compile(r"^( {0,3})(#{1,6})(\s+)(.+?)((?:\s+#+)?)\s*$")

# List markers: ``-``, ``*``, ``+`` bullets and ``N.`` / ``N)`` ordered
# (per CommonMark §5.2, max 9-digit marker). Leading indent up to 3 spaces.
_LIST_RE = re.compile(r"^ {0,3}(?:[-*+]|\d{1,9}[.)])\s")

# Thematic break (``---`` / ``***`` / ``___``), possibly spaced.
_THEMATIC_BREAK_RE = re.compile(r"^ {0,3}(?:-[ -]{2,}|\*[ *]{2,}|_[ _]{2,})\s*$")

# HTML block starter. Restricted to CommonMark §4.6 type 1-6 cases:
#   - type 1: ``<script``, ``<pre``, ``<style``, ``<textarea``
#   - type 2: ``<!--`` (comment)
#   - type 3: ``<?`` (processing instruction)
#   - type 4: ``<!`` + uppercase (DOCTYPE etc.)
#   - type 5: ``<![CDATA[``
#   - type 6: opening/closing block-level tag from the explicit list in
#     §4.6 (address, article, ... ul, video) — inline tags such as
#     ``<span>`` or ``<a>`` must NOT open a block, otherwise following
#     prose lines are wrongly suppressed from merging. Type 7 (any
#     complete start/end tag alone on a line followed by a blank) is
#     omitted intentionally: the lookahead requirement is expensive
#     and type-7 blocks in OCR corpora are negligible.
_HTML_BLOCK_TYPE_6_TAGS = (
    "address|article|aside|base|basefont|blockquote|body|caption|center|"
    "col|colgroup|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|"
    "figure|footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|"
    "legend|li|link|main|menu|menuitem|nav|noframes|ol|optgroup|option|p|"
    "param|section|source|summary|table|tbody|td|tfoot|th|thead|title|"
    "tr|track|ul"
)
_HTML_BLOCK_RE = re.compile(
    r"^ {0,3}(?:"
    r"<(?:script|pre|style|textarea)(?:\s|>|$)"  # type 1
    r"|<!--"  # type 2
    r"|<\?"  # type 3
    r"|<![A-Z]"  # type 4
    r"|<!\[CDATA\["  # type 5
    rf"|</?(?:{_HTML_BLOCK_TYPE_6_TAGS})(?:\s|/?>|$)"  # type 6
    r")",
    re.IGNORECASE,
)

# Link reference definition (CommonMark §4.7): ``[label]: destination``
# with optional title. Conservative one-line form — the multi-line form
# (title on the next line) is rare in the target corpora and the
# structural check only needs to prevent the *first* line from being
# merged into prose above. Matches 0-3 space indent, label, ``:``, and
# at least one non-space destination char.
_LINK_REF_DEF_RE = re.compile(r"^ {0,3}\[[^\]\n]+\]:\s*\S+")

# Indented code block starter (CommonMark §4.4): 4+ leading spaces OR a
# leading tab. Only a *block start* when preceded by a blank line (or
# doc start); otherwise it's lazy paragraph continuation. State tracked
# in the main loop, not here.
_INDENTED_CODE_RE = re.compile(r"^(?:    |\t)")

# ATX heading detector for *structural* purposes — wider than
# ``sections._HEADING_RE``, which requires a non-empty title (and so
# misses CommonMark-valid empty headings like ``#`` or ``## ``). We
# only need "is this line a heading, so do not merge it"; empty
# headings qualify and must not be swallowed into adjacent prose.
_HEADING_STRUCTURAL_RE = re.compile(r"^ {0,3}#{1,6}(?:[ \t]|$)")


def _is_structural(line: str) -> bool:
    """Lines that must never merge or be merged into.

    A blank line is NOT structural here — blank lines are handled as
    paragraph boundaries by the main loop, which stops any merge chain
    without extra checks.
    """
    if _HEADING_STRUCTURAL_RE.match(line):
        return True
    if _LIST_RE.match(line):
        return True
    stripped = line.lstrip()
    if stripped.startswith(">"):  # blockquote
        return True
    if stripped.startswith("|"):  # table row
        return True
    if _THEMATIC_BREAK_RE.match(line):
        return True
    if _HTML_BLOCK_RE.match(line):
        return True
    if _LINK_REF_DEF_RE.match(line):
        return True
    # Fence delimiters (opener + closer share shape). An in-fence content
    # line never reaches here — the main loop short-circuits in fence
    # mode. This catches the *closer* line, which would otherwise look
    # like a 3-char non-terminator body line and swallow the line below.
    if _FENCE_OPEN_RE.match(line):
        return True
    return False


def _ends_with_terminator(line: str) -> bool:
    """True iff ``line``, after stripping trailing whitespace + closing
    wrappers, ends in a ``SENTENCE_TERMINATORS`` character.

    An empty line returns True (treated as "terminated" so the caller
    never tries to merge across it — though blank lines are filtered
    upstream anyway; this is defence-in-depth)."""
    s = line.rstrip()
    if not s:
        return True
    while s and s[-1] in CLOSING_WRAPPERS:
        s = s[:-1]
    if not s:
        return True
    return s[-1] in SENTENCE_TERMINATORS


def _line_ending(body: str) -> str:
    """Return ``\\r\\n`` if ``body`` contains any CRLF, else ``\\n``.

    ``splitlines()`` discards both styles; we rejoin with this on the way
    out to preserve CRLF inputs instead of silently downgrading to LF.
    Mixed-ending inputs (pathological — usually a malformed file) are
    emitted as all-CRLF rather than all-LF, which favours data
    preservation on the common case of CRLF files that accidentally
    contain a stray LF."""
    return "\r\n" if "\r\n" in body else "\n"


def _preserve_trailing_newline(original: str, rebuilt: str, nl: str) -> str:
    """Re-emit as many trailing line-endings as ``original`` had, using
    the chosen ``nl`` style on output.

    ``splitlines()`` discards trailing empty lines, so simply appending a
    single ``nl`` when the original ended with one would still lose
    inputs like ``"a\\n\\n"`` (two trailing newlines, one trailing blank
    line). Count both CRLF and LF so mixed-ending inputs don't silently
    drop a trailing newline when the dominant style is CRLF."""
    trail = 0
    s = original
    while s.endswith("\r\n") or s.endswith("\n"):
        s = s[:-2] if s.endswith("\r\n") else s[:-1]
        trail += 1
    while rebuilt.endswith(nl):
        rebuilt = rebuilt[: -len(nl)]
    return rebuilt + nl * trail


def normalize_paragraphs(body: str) -> str:
    """Merge consecutive non-blank paragraph lines whose predecessor does
    not end with a sentence terminator. Returns a new string.

    Preserves fenced code blocks, ATX headings, list items, blockquotes,
    table rows, thematic breaks, HTML block starters, and blank lines.
    Merged lines are joined with no separator — correct for CJK prose
    and harmless for ASCII (source newlines between ASCII words usually
    came with a trailing space, which ``rstrip`` on the predecessor
    already consumed; callers needing word-boundary space can override
    ``SENTENCE_TERMINATORS`` to include more punctuation).
    """
    nl = _line_ending(body)
    lines = body.splitlines()
    out: list[str] = []
    fence_opener: str | None = None
    # Inside a list item / blockquote / HTML block, subsequent non-blank
    # lines are lazy-continuations of the container (CommonMark §5.2,
    # §5.1, §4.6) — they belong to the container, not to the paragraph
    # above or below. Reset on blank line (container close) or on a
    # fence opener (fence terminates the container's paragraph).
    # Note: HTML block type 6 (``<div>`` etc.) closes ONLY on blank line
    # per CommonMark — even an explicit ``</div>`` on its own line keeps
    # the block open. So ``<div>\\n</div>\\nx\\ny`` does not merge
    # ``x``/``y``; that is spec-correct, not a bug. Callers who want
    # those lines merged must insert a blank line after ``</div>``.
    in_container = False
    # Indented code block (CommonMark §4.4): 4+ indent after blank line
    # or doc start. Continues across 4+ indented lines until a blank
    # line terminates it. Tracked separately from ``in_container``
    # because an indented code line that *interrupts* a paragraph is
    # lazy-continuation, not a new block — so we can only enter this
    # state when the preceding output is blank or absent.
    in_indented_code = False
    # A blank line inside a list region may either close the list or
    # separate multi-paragraph list items. Parked while we wait for the
    # next non-blank line to disambiguate.
    list_continuation_pending = False

    for line in lines:
        if fence_opener is not None:
            out.append(line)
            m = _FENCE_CLOSE_RE.match(line)
            if (
                m
                and m.group(1)[0] == fence_opener[0]
                and len(m.group(1)) >= len(fence_opener)
            ):
                fence_opener = None
            continue

        m_fence = _FENCE_OPEN_RE.match(line)
        if m_fence:
            out.append(line)
            fence_opener = m_fence.group(1)
            # Keep ``in_container`` as-is: a fenced code block inside a
            # list item is still list content, and the list container
            # must remain open for lines after the fence closes.
            in_indented_code = False
            continue

        # Blank line: paragraph boundary. Indented-code state is always
        # closed. List-container state is only *provisionally* closed —
        # a multi-paragraph list item keeps the container open across
        # interior blanks (CommonMark §5.2). We park that in
        # ``list_continuation_pending``; the next non-blank line commits
        # the close (unindented → really out of the list) or re-opens
        # (indented → still inside the list item).
        if not line.strip():
            out.append(line)
            # Any open container (list / blockquote / HTML) may extend
            # across this blank line — defer the close until we see
            # whether the next non-blank line is indented (continuation)
            # or flush-left (genuinely out of the container).
            if in_container:
                list_continuation_pending = True
            in_container = False
            in_indented_code = False
            continue

        # Resolve any pending list continuation set by a prior blank line.
        if list_continuation_pending:
            list_continuation_pending = False
            if line[:1] in (" ", "\t"):
                in_container = True

        # Indented code block: enter when 4+ indent (or a leading tab)
        # follows a blank line, doc start, or any non-paragraph block
        # (heading, list item, blockquote, HTML block, table row,
        # thematic break, fence closer — CommonMark §4.4 lets indented
        # code follow any block except a paragraph, where it'd be lazy
        # continuation instead). Stay while consecutive indented lines
        # continue. Lines are appended verbatim — preserving the indent
        # is the whole point. We do not flip ``in_container`` off here:
        # indented code inside a list item is still list content, and
        # exiting the list container still requires a blank line.
        if _INDENTED_CODE_RE.match(line):
            if (
                in_indented_code
                or not out
                or not out[-1].strip()
                or _is_structural(out[-1])
            ):
                out.append(line)
                in_indented_code = True
                continue
        in_indented_code = False

        if _is_structural(line):
            out.append(line)
            # Only list / blockquote / HTML blocks continue to the next
            # blank line. Headings, thematic breaks, table rows, and
            # fence closers are single-line structural — they don't open
            # a container for the following lines.
            stripped = line.lstrip()
            in_container = bool(
                _LIST_RE.match(line)
                or stripped.startswith(">")
                or _HTML_BLOCK_RE.match(line)
            )
            continue

        if not out:
            out.append(line)
            continue

        # Container continuation: lazy lines belong to the container
        # above — never merge them into sibling paragraphs.
        if in_container:
            out.append(line)
            continue

        prev = out[-1]

        # Prev-blank guard is defence-in-depth: blank lines above already
        # appended and reset in_container, but keep the explicit check so
        # the merge branch can't silently concatenate into "".
        if not prev.strip():
            out.append(line)
            continue

        if _is_structural(prev):
            out.append(line)
            continue

        # Prev was an indented-code line (appended via the indented-code
        # branch): it must not be extended by the following non-indented
        # line. ``_is_structural`` doesn't catch this because 4+ space
        # indent is only structural in the indented-code context —
        # lazy-continuation of a paragraph (which produces a merged,
        # non-indented prev) must still be mergeable.
        if _INDENTED_CODE_RE.match(prev):
            out.append(line)
            continue

        if _ends_with_terminator(prev):
            out.append(line)
            continue

        # Merge: concatenate without separator; lstrip the continuation
        # to drop lazy-continuation indent (CommonMark allows leading
        # whitespace on paragraph-continuation lines without changing
        # the block).
        out[-1] = prev.rstrip() + line.lstrip()

    return _preserve_trailing_newline(body, nl.join(out), nl)


def normalize_heads(body: str, rules: list[HeadRule]) -> str:
    """Rewrite ATX heading levels based on title-pattern rules.

    For each ATX heading, rules are checked in order; the first whose
    ``pattern`` matches (``re.search``) the title sets the new level.
    Non-matching headings and non-heading lines are preserved verbatim.
    Fenced code blocks and HTML blocks are skipped (``## inside fence``
    or ``# inside <div>`` is not an ATX heading under CommonMark and
    must not be rewritten).

    ``rules == []`` is a no-op — the upstream default.
    """
    if not rules:
        return body

    compiled: list[tuple[re.Pattern[str], int]] = []
    for r in rules:
        level = r["level"]
        if not (1 <= level <= 6):
            raise ValueError(f"Invalid heading level {level!r}: must be 1-6")
        compiled.append((re.compile(r["pattern"]), level))

    nl = _line_ending(body)
    lines = body.splitlines()
    out: list[str] = []
    fence_opener: str | None = None
    # HTML block: enters on an HTML block starter, exits on blank line
    # (CommonMark type 6 semantics — same conservative rule used by
    # ``normalize_paragraphs``). Lines inside are never treated as
    # headings.
    in_html_block = False

    for line in lines:
        if fence_opener is not None:
            out.append(line)
            m = _FENCE_CLOSE_RE.match(line)
            if (
                m
                and m.group(1)[0] == fence_opener[0]
                and len(m.group(1)) >= len(fence_opener)
            ):
                fence_opener = None
            continue

        m_fence = _FENCE_OPEN_RE.match(line)
        if m_fence:
            out.append(line)
            fence_opener = m_fence.group(1)
            in_html_block = False
            continue

        if not line.strip():
            out.append(line)
            in_html_block = False
            continue

        if in_html_block:
            out.append(line)
            continue

        if _HTML_BLOCK_RE.match(line):
            out.append(line)
            in_html_block = True
            continue

        m_head = _HEADING_RE.match(line)
        if not m_head:
            out.append(line)
            continue

        title = m_head.group(2).strip()
        new_line = line
        for pat, level in compiled:
            if pat.search(title):
                new_line = _rewrite_heading_level(line, level)
                break
        out.append(new_line)

    return _preserve_trailing_newline(body, nl.join(out), nl)


def _rewrite_heading_level(line: str, level: int) -> str:
    """Swap the ``#+`` opener in an ATX heading to ``level`` ``#``s,
    preserving leading indent, separator, title, and optional closer.
    Returns ``line`` unchanged if it does not parse (should not happen
    when called after ``_HEADING_RE`` match, but defensive)."""
    m = _HEADING_REWRITE_RE.match(line)
    if not m:
        return line
    indent, _opener, sep, title, closer = m.groups()
    return f"{indent}{'#' * level}{sep}{title}{closer}"
