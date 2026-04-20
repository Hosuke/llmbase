"""Tests for tools.split — 议 E flat-section primitive (v0.7.6).

Strict primitive: parse only. No corpus heuristics. These tests
exercise the core contract siwen's ``split_taixu_bian`` and any
other downstream will build on top of.
"""

from pathlib import Path

import pytest

from tools.split import Section, split_by_heading


FIXTURES_DIR = Path(__file__).parent / "fixtures" / "taixu"


# ─── empty / no-match → [] ─────────────────────────────────────────────


def test_empty_body_returns_empty():
    assert split_by_heading("", level=2) == []


def test_no_headings_returns_empty():
    assert split_by_heading("plain prose\nmore prose\n", level=2) == []


def test_no_target_level_returns_empty():
    """Body has # and ### but no ##; asking for level=2 → []."""
    body = "# top\ncontent\n### deep\ndeep content\n"
    assert split_by_heading(body, level=2) == []


# ─── level validation ─────────────────────────────────────────────────


@pytest.mark.parametrize("level", [0, -1, 7, 100])
def test_invalid_level_raises(level):
    with pytest.raises(ValueError):
        split_by_heading("## x\n", level=level)


# ─── single heading ───────────────────────────────────────────────────


def test_single_heading_yields_one_section():
    body = "## 緒論\n第一段。\n"
    out = split_by_heading(body, level=2)
    assert len(out) == 1
    s = out[0]
    assert s.level == 2
    assert s.title == "緒論"
    assert s.header_line == "## 緒論"
    assert s.start == 0
    assert s.end == len(body)
    assert s.content == "第一段。"


def test_single_heading_last_line_no_newline():
    body = "## 終"
    out = split_by_heading(body, level=2)
    assert len(out) == 1
    assert out[0].header_line == "## 終"
    assert out[0].content == ""


# ─── multiple headings, same level ────────────────────────────────────


def test_two_same_level_sections():
    body = "## 一\n第一段。\n## 二\n第二段。\n"
    out = split_by_heading(body, level=2)
    assert [s.title for s in out] == ["一", "二"]
    assert out[0].content == "第一段。"
    assert out[1].content == "第二段。"
    # Offsets are consistent with body slicing.
    assert body[out[0].start:out[0].end].startswith("## 一")
    assert body[out[1].start:out[1].end].startswith("## 二")


def test_section_offsets_cover_body_contiguously():
    """Offsets should partition ``body[sections[0].start:]`` with no
    gaps and no overlaps — useful contract for downstream stitching."""
    body = "## A\naaa\n## B\nbbb\n## C\nccc\n"
    out = split_by_heading(body, level=2)
    assert out[0].end == out[1].start
    assert out[1].end == out[2].start
    assert out[2].end == len(body)


# ─── mixed levels: deeper vs higher headings ──────────────────────────


def test_deeper_headings_stay_in_content():
    """### inside a ## section is not a split point — stays in content."""
    body = "## 章\n前言\n### 節\n節內\n## 次章\nx\n"
    out = split_by_heading(body, level=2)
    assert len(out) == 2
    assert out[0].title == "章"
    # The ### line and its content belong to section 0, not a new section.
    assert "### 節" in out[0].content
    assert "節內" in out[0].content


def test_higher_level_ends_section():
    """# (higher in hierarchy) ends a ## section."""
    body = "## 一\naaa\n# 頂\nbbb\n## 二\nccc\n"
    out = split_by_heading(body, level=2)
    assert len(out) == 2
    # Section ["一"] must end at "# 頂" (not include it).
    assert out[0].content == "aaa"
    assert "# 頂" not in out[0].content
    # "# 頂" is not level=2 so it's NOT in return; "二" is.
    assert out[1].title == "二"


# ─── preface handling (NOT returned — downstream slices body) ─────────


def test_preface_before_first_heading_not_in_result():
    body = "這是前言\n還有一句\n## 章\n內文\n"
    out = split_by_heading(body, level=2)
    assert len(out) == 1
    # Downstream recovery pattern:
    preface = body[:out[0].start]
    assert preface == "這是前言\n還有一句\n"


# ─── fenced code blocks — headings inside must NOT split ──────────────


def test_fence_protects_pseudo_heading():
    body = "## real\n```\n## fake in fence\n```\n內文\n"
    out = split_by_heading(body, level=2)
    assert len(out) == 1  # fence-inner ## is not a split point
    assert out[0].title == "real"
    # Fence block stays inside content:
    assert "## fake in fence" in out[0].content


def test_tilde_fence_also_respected():
    body = "## a\n~~~\n## fake\n~~~\ntext\n## b\nx\n"
    out = split_by_heading(body, level=2)
    assert [s.title for s in out] == ["a", "b"]


# ─── CommonMark §4.2: 0-3 space indent tolerated ──────────────────────


def test_three_space_indent_heading_recognized():
    body = "   ## indented\ncontent\n"
    out = split_by_heading(body, level=2)
    assert len(out) == 1
    assert out[0].title == "indented"


def test_four_space_indent_not_heading():
    """4+ leading spaces = indented code block per §4.4, not a heading."""
    body = "    ## not-a-heading\ncontent\n## real\nx\n"
    out = split_by_heading(body, level=2)
    # Only "real" is a true heading.
    assert len(out) == 1
    assert out[0].title == "real"


# ─── line ending variants ──────────────────────────────────────────────


def test_crlf_body():
    body = "## 一\r\nfoo\r\n## 二\r\nbar\r\n"
    out = split_by_heading(body, level=2)
    assert len(out) == 2
    assert out[0].title == "一"
    assert out[0].header_line == "## 一"  # no \r
    assert out[0].content == "foo"
    assert out[1].title == "二"


def test_cr_only_body():
    """Classic-Mac ``\\r``-only line endings (rare but valid) must not
    collapse content into header_line. Codex v0.7.6 MEDIUM regression."""
    body = "## A\rfoo\r## B\rbar\r"
    out = split_by_heading(body, level=2)
    assert len(out) == 2
    assert out[0].header_line == "## A"
    assert out[0].content == "foo"
    assert out[1].header_line == "## B"
    assert out[1].content == "bar"


# ─── content stripping ─────────────────────────────────────────────────


def test_content_leading_trailing_blank_lines_stripped():
    body = "## a\n\n\n正文。\n\n\n## b\nx\n"
    out = split_by_heading(body, level=2)
    assert out[0].content == "正文。"


def test_content_preserves_internal_blank_lines():
    body = "## a\n第一段\n\n第二段\n## b\nx\n"
    out = split_by_heading(body, level=2)
    assert "第一段\n\n第二段" in out[0].content


# ─── various levels ───────────────────────────────────────────────────


@pytest.mark.parametrize("level,hashes", [(1, "#"), (3, "###"), (6, "######")])
def test_splits_at_requested_level(level, hashes):
    body = f"{hashes} a\nfoo\n{hashes} b\nbar\n"
    out = split_by_heading(body, level=level)
    assert [s.title for s in out] == ["a", "b"]
    assert all(s.level == level for s in out)


# ─── realistic 太虛-style structure ────────────────────────────────────


def test_taixu_style_bian_with_chapters():
    """Representative shape of siwen 太虛 input: one 編 header at level=1
    with multiple 章 sections at level=2, each with 甲乙 sub-items at
    level=3. split_by_heading(body, level=2) yields one section per 章."""
    body = (
        "# 第一編 判教論\n"
        "前言\n"
        "## 第一章 總論\n"
        "甲乙丙丁...\n"
        "### 甲、釋義\n"
        "釋文...\n"
        "### 乙、判屬\n"
        "判文...\n"
        "## 第二章 別論\n"
        "別論內文...\n"
        "### 甲、分科\n"
        "分科文...\n"
    )
    out = split_by_heading(body, level=2)
    assert [s.title for s in out] == ["第一章 總論", "第二章 別論"]
    # Chapter 1's ### subsections stay in its content:
    assert "### 甲、釋義" in out[0].content
    assert "### 乙、判屬" in out[0].content
    # Chapter 2 doesn't accidentally absorb chapter 1's tail:
    assert "甲乙丙丁" not in out[1].content


# ─── Section dataclass ────────────────────────────────────────────────


def test_section_is_dataclass():
    """Downstream code assumes field-based (not dict-based) access —
    verify the dataclass contract holds."""
    body = "## a\ncontent\n"
    s = split_by_heading(body, level=2)[0]
    assert isinstance(s, Section)
    # All documented fields present:
    for f in ("level", "title", "header_line", "start", "end", "content"):
        assert hasattr(s, f)


# ─── Taixu corpus fixtures (tests/fixtures/taixu/) ────────────────────
# These exercise split_by_heading against real siwen·太虛 book bodies
# (post-wenguan / post-normalize). The expected counts match the per-
# file shape documented in tests/fixtures/taixu/README.md; regressions
# here mean either the splitter changed semantics or a fixture drifted.


@pytest.mark.parametrize("filename, level2_count, level3_count", [
    # (file, expected h2 sections, expected h3 sections)
    ("sanming_lun.md", 3, 0),
    ("xinjing_shiyi.md", 1, 7),
    ("focheng_zongyao_lun_head50kb.md", 6, 24),
])
def test_taixu_fixture_split_counts(filename, level2_count, level3_count):
    body = (FIXTURES_DIR / filename).read_text(encoding="utf-8")
    assert len(split_by_heading(body, level=2)) == level2_count
    assert len(split_by_heading(body, level=3)) == level3_count


def test_taixu_fixture_sanming_lun_h2_titles():
    """Concrete titles the splitter should recover — guards against
    off-by-one or whitespace-handling regressions on CJK titles."""
    body = (FIXTURES_DIR / "sanming_lun.md").read_text(encoding="utf-8")
    out = split_by_heading(body, level=2)
    assert [s.title for s in out] == ["緣起分第一", "名義分第二", "界別分第三"]


def test_taixu_fixture_duplicate_chapter_numbers_preserved():
    """``focheng_zongyao_lun`` restarts chapter numbering at each 编
    boundary — ``第一章`` appears twice at h2. The splitter must yield
    every occurrence (no title-based dedup)."""
    body = (FIXTURES_DIR / "focheng_zongyao_lun_head50kb.md").read_text(encoding="utf-8")
    out = split_by_heading(body, level=2)
    first_chapters = [s for s in out if s.title.startswith("第一章")]
    assert len(first_chapters) == 2, \
        f"expected two '第一章' sections (multi-编 cross-boundary), got {len(first_chapters)}"


def test_taixu_fixture_xinjing_no_h1_in_body():
    """Parse-only contract: body has no h1 even though frontmatter's
    ``book`` field matches the h2 title. Upstream must not fabricate
    an h1 — that's siwen's post-processor concern."""
    body = (FIXTURES_DIR / "xinjing_shiyi.md").read_text(encoding="utf-8")
    assert split_by_heading(body, level=1) == []
    h2 = split_by_heading(body, level=2)
    assert len(h2) == 1
    assert h2[0].title == "般若波羅密多心經釋義"
