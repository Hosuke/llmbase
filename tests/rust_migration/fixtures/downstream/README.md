# 下游契約 fixture

此庫專為遷移測試手造，不取遠端文庫。`example.invalid` 乃標記，不可抓取。

- `library/` 明置非默認資料根，文言裸正文、未知巢狀 frontmatter、alias、來源章卷、非 BMP 字 `𠮷` 與組合字 `é` 俱存。
- 測試須先複製至 `tmp_path`，不得就此庫落索引、編條或變更原典。
- 原典句乃測試所定；引用、編纂、問答之預期不得隨被測實作變動。
- `test_downstream_contracts.py` 先固定舊 Python 邊界之行為；其通過不證 Rust 已接管，尚須候選 wheel、原下游測試及逐路徑 native trace 驗收。
