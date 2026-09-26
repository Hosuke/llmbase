# Python native candidate bridge

此候選仍沿用 `llmwiki==0.9.0`，未作 release。`llmbase-python` 為獨立
PyO3 `abi3-py311` extension；`llmwiki._core.dispatch(base_dir, library_id,
operation, args_json)` 直接進 `llmbase_core::application::Application`。
GIL 於 native work 期間釋放。核心、CLI、HTTP、MCP 不依賴 PyO3；Python
distribution 保留既有 Python 模組與 Flask factory。

建置依 [PyO3 0.27.2 官方文檔](https://pyo3.rs/v0.27.2/building-and-distribution)
及 [setuptools-rust](https://setuptools-rust.readthedocs.io/en/latest/)：
`pyproject.toml` 宣告 mixed package extension，`setup.cfg` 固定 wheel tag
`cp311-abi3`，`MANIFEST.in` 帶入 workspace manifests、Rust sources、lockfile
及 operation JSON。extension 非 optional；build 失敗不得偷偷產純 Python wheel。

## 已接管的搜尋邊界

`search()`、`search_raw()` 在 extension 可載入時總走 native。原文讀取、
frontmatter parsing、快照驗證、預設分詞、TF-IDF 計分、排序、snippet 與
raw 本機路徑遮蔽皆在 Rust。原生 FTS 是另一檢索 dialect；相容搜尋明確標
`storage=native_filesystem_snapshot`、`algorithm=legacy-tfidf-v1`、`indexed=false`，
不得將此收據冒稱為 FTS index 或 revision evidence。

快照不建 `wiki/_meta` 或 database；已有 native writer lock 時取 shared lock，
未有 lock 時重讀兩次並比完整 source ID/content hash manifest。raw 中解析或
讀取失敗的個別檔略過，compiled 中則報錯；空查詢先短路，避免增添原契約沒有的
解析錯誤。檔案每次重新讀取，無 import-time corpus cache。

預設搜尋每次送入當前 `STOPWORDS`、`CJK_STOPWORDS`。若設
`SEARCH_TOKENIZER` callable，Python 真正對查詢及 native 傳回的每份全文呼叫它，
再把 tokens、source ID、content hash、snapshot 一併送回 Rust。核心要求完整且
無重複的 corpus；改檔、漏檔或偽 hash 皆失敗，計分仍由 Rust 執行。callable
於進程中被替換，下一次搜尋即採新值；例外直接傳回，不忽略 callback。

`llmwiki._native.backend_info()` 包含 extension 路徑、ABI、application 名稱及
native 成功 dispatch 計數；`last_trace()` 是該 Python context 最後一次實際操作
收據，dispatch 失敗會先清空。只看啟動時載入 `.so` 不算搜尋接管證據。

## 能力不足時的行為

未建 extension 的 source checkout 暫容舊 Python 搜尋，`backend_info()` 明示
`native=false`。`LLMBASE_REQUIRE_NATIVE=1` 或安裝記錄帶 `_core` 的 candidate
若失去 extension，必報 `NativeCapabilityError`。extension 載入錯誤、callback
錯誤與 native execution 錯誤均不得轉回舊核心。正式候選驗證設此環境變數，且從
checkout 外以 `--import-mode=importlib` 執行，避免誤 import 工作樹。

## 驗證

`crates/llmbase-core/tests/compat_search.rs` 驗 native scores/keys、動態停用詞、
檔案變動、callback corpus 驗證、Unicode scalar、raw 錯誤略過、top-k 舊差異、
private capability 及不創建 database。`tests/rust_migration/test_native_search.py`
需真 extension；測試會封鎖舊 Python scorer、`frontmatter.load` 及預設 Python
tokenizer，以真 dispatch 計數與 request trace 證明接管，另與保存的舊函式逐欄
比較結果。第二版候選真安裝後，從 `/tmp` 執行上述 native suite、既有
`test_search_raw.py` 及 `test_downstream_contracts.py`：33 passed，4.01 s；Rust
compat suite 10 passed。以 `LLMBASE_REQUIRE_NATIVE=1`、`--import-mode=importlib`
執行，import 路徑確為 `.rust-migration-runs/candidate-venv/lib/python3.12/site-packages`。

機器收據：[python-native-search-v2.json](results/python-native-search-v2.json)。
wheel SHA-256 為 `83fbe489e472b7f1de71d981ccd6df286bd2bebe87cd15221462e90bed0b8d8b`；
extension SHA-256 為 `264f972d4754215f7b11997bcfb51ed2f366badffd14a1856e53119c4e08c413`。
收據另真做四次 native dispatch、三次 Python tokenizer callback，排名隨 callback
改變，且未建 database。wheel 及 sdist 已建成；sdist 解包後再建 wheel 未驗。
工具鏈曾警告 `rust-objcopy` strip debug info 遭 SIGABRT，但 build exit 0；上述
載入、執行及測試均用實際產物，未把該警告當作 runtime 成功證。

本候選此階段只證搜尋 facade；其他 Python 模組、斯文完整 compile fork、前端
與 bake 仍須各自驗證。Rust snapshot 拒絕 symlink 並有 64 MiB 來源上限；此為
native 邊界，並非宣稱所有不受限 Python 檔案行為已等價。目前 compiled
`title`／`summary` 要求字串，tags 要求字串或字串陣列；舊 Python 對部分非字串
metadata 的 f-string 隱式轉型未列為已等價。raw `title: null` 則依舊回 file stem，
raw 不讀 summary/tags，已另有差分測試。
