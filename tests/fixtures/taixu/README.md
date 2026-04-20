# Taixu corpus fixtures

Real-world markdown samples from the siwen 太虛 (Taixu Dashi) knowledge base,
used as regression fixtures for `tools.split` and `tools.normalize`.

## Source

The Taixu corpus is a 62-book collection of 民國 釋太虛 (Master Taixu,
1890-1947) from CBETA's 太虛大師全書, converted to markdown and integrated
via siwen's wenguan pipeline. These three samples were copied from the
siwen production instance (`/root/wenku-fo/raw/`) after `wenguan` and
`normalize_heads` / `normalize_paragraphs` post-processing.

Runtime metadata (`wenguan_at`, `normalized_*_at`, `ingested_at`, etc.)
has been stripped. Structural metadata (`book`, `bian`, `author`, etc.)
is preserved so fixture consumers can route by编 (division) or 品 (chapter).

## Files

| File | Size | Shape | Use |
|------|------|-------|-----|
| `sanming_lun.md` | 16 KB | 3 h2 (`## 緣起分第一` etc.) + 13 h3 + deep nesting | Small full book, structural edge cases |
| `xinjing_shiyi.md` | 45 KB | 1 h2 + 5 h3 + `## 般若波羅密多心經釋義` (book-name-as-h2) | Duplicate-title edge: book name appears as both h1 (top) and h2 (body) |
| `focheng_zongyao_lun_head50kb.md` | 46 KB | Truncated first ~45KB, multi-编 structure | Typical mid-size book, tests `split_by_heading` against a deep nested outline |

## Intended use

- `tools.split.split_by_heading(body, level=2)` → should yield 3 sections in
  `sanming_lun` (緣起分第一 / 名義分第二 / 界別分第三).
- `tools.normalize.normalize_heads(body, TAIXU_PACK)` where `TAIXU_PACK` is
  the siwen-side head-pattern list — should re-level `### 一、` to h4 etc.
  per 科分 / 總論 / 第N節 / [一二三…] / [甲乙丙…] / [子丑寅…] pattern ladder.
- Edge: `xinjing_shiyi.md` has `## 般若波羅密多心經釋義` that matches the
  book frontmatter's `book` field — siwen's post-processor promotes this
  to h1, upstream `normalize_heads` (parse-only) does not. This is a
  useful fixture for verifying the parse-only contract.

## Licensing

The underlying text is public-domain CBETA material. These fixtures are
distributed under the same MIT license as llmwiki itself.
