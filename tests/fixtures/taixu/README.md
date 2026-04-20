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

| File | Size | Shape (h1 / h2 / h3 / h4 / h5) | Use |
|------|------|-------|-----|
| `sanming_lun.md` | 16 KB | 1 / 3 / 0 / 3 / 0 | Small full book: `緣起分第一` / `名義分第二` / `界別分第三` at h2, each with h4 sub-items directly (no h3 layer) — structural edge where taixu-pack's intermediate level is absent |
| `xinjing_shiyi.md` | 45 KB | 0 / 1 / 7 / 37 / 0 | Duplicate-title edge: frontmatter `book: 般若波羅密多心經釋義` + first body heading `## 般若波羅密多心經釋義` (no h1). Exercises the parse-only contract — `normalize_heads` must not "promote" body h2 to h1 to dedupe; that's a downstream concern. |
| `focheng_zongyao_lun_head50kb.md` | 46 KB | 1 / 6 / 24 / 14 / 2 | Typical mid-size book, truncated first ~45 KB. Multi-編 cross-boundary: h2 chapter numbering restarts (`第一章` appears twice — sections 1 & 5), so `split_by_heading(body, level=2)` must yield 6 distinct sections without dedup-by-title. |

## Intended use

- `tools.split.split_by_heading(body, level=2)` → should yield 3 sections in
  `sanming_lun` (緣起分第一 / 名義分第二 / 界別分第三).
- `tools.normalize.normalize_heads(body, TAIXU_PACK)` where `TAIXU_PACK` is
  the siwen-side head-pattern list — should re-level `### 一、` to h4 etc.
  per 科分 / 總論 / 第N節 / [一二三…] / [甲乙丙…] / [子丑寅…] pattern ladder.
- Edge: `xinjing_shiyi.md` has `## 般若波羅密多心經釋義` that matches the
  book frontmatter's `book` field (no h1 in the body). siwen's
  post-processor promotes this to h1; upstream `normalize_heads`
  (parse-only) does not. Useful fixture for verifying the parse-only
  contract holds.
- Edge: `focheng_zongyao_lun_head50kb.md` restarts chapter numbering at
  each 编 boundary, so `第一章` / `第二章` appear multiple times at h2.
  `split_by_heading(body, level=2)` must yield every occurrence (6
  sections) — no title-based dedup.

## Licensing

The underlying text is public-domain CBETA material. These fixtures are
distributed under the same MIT license as llmwiki itself.
