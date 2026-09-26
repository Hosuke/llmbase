# KnowledgeBase 原生讀寫切片

`KnowledgeBase.get_article/list_articles/health_report`、operations 的
`kb_get/kb_get_sections/kb_list/kb_stats` 與 `ingest.list_raw` 在有 extension 的
候選中進入 Rust `compat_kb`。舊函式形狀與 registry 留存；custom operation
handler 仍於呼叫時查當前 registry，Flask factory、view functions 及 hooks 未封死。

`compat_kb_*` 操作在 `Library::open` 之前分派，讀取不建 SQLite 或 index。
`compat_kb_get` 支持 `no_alias`，以保存 `KnowledgeBase.get_article` 不做 alias
解析、而 `operations.kb_get` 做解析的差異。alias 讀既有 `aliases.json`，依
exact／空格轉連字／去空格／已啟用 OpenCC／fuzzy 的舊順序處理，保留 JSON
檔案 insertion order 的 first-match 行為。標準 metadata 欄保留 JSON 原型別，
含 null、number、bool、array、object；不先強轉字串。

章節樹由 Rust 計算，保留 Python scalar offsets、SHA-1 anchor、祖先 chain、
重名 suffix、fenced code 排除及 Unicode splitlines。此相容 API 不把 native
evidence 的 UTF-8 byte offsets 混入章節 DTO。
相容 body 另依 Python text-mode 規則把 CRLF／CR 轉 LF，章節位置亦對應該 DTO；
source 原字節不改。此差異曾由新增差分測試實際抓出，修正後已過候選驗證。

`KnowledgeBase.cfg["paths"]` 原為可變 cache，故本地 PyO3 dispatch 另接受
optional `paths_json`，用當次 Python 配置建立 Application。此欄不在 public
operation arguments 中，HTTP/MCP 呼叫不能用它重設 library roots。

本地 `.md/.markdown/.txt/.py/.json/.csv` ingest 經
`compat_kb_raw_write` 持久化。外部來源讀取、Markdown adapter、metadata 增補與
`_slugify` 等 Python extension points 保留，原生側負責 scoped path、accepted
Markdown 驗證、同一 `llmbase.writer.lock`、temporary file、fsync、rename 及
optional expected hash。純文字附件以 byte array 傳輸，保留非 UTF-8、CRLF、NUL
原字節；新增 metadata companion 仍是獨立一次寫入。source permissions 保留。
寫入成功後才發原 `ingested` hook，callback 失敗仍依舊 best-effort。

原生單筆 ingest 不掃 unrelated raw，不因別檔壞 frontmatter 而新增拒收，也不
暗中建 index。其 trace 明示 `indexed=false`。

`operations.dispatch` 已把原先須 `worker.job_lock` 的操作再包入
`_native.with_writer`；實際 Python handler 在同 OS thread 持 native lexical
WriterSession 執行。nested native 寫入及相容讀取共借該 lease，不自鎖；其他
thread 或 process 寫入返回 busy。Python callback 的回傳物件 identity、例外
型別保留，例外退出會釋 lease；`backend_info.writer_sessions` 僅計鎖邊界，
不冒作領域運算 trace。

已建 database 的 write-intent 串接、binary/PDF connector 寫入、web ingest、
direct query/file-back 寫入及 `atomic.py` arbitrary paths 尚有後續整合；不得冒稱
所有直接 Python API 或整條 compile/ingest pipeline 已由原生交易包住。未盲改
舊 StageLock 的 PID/TTL 契約。

Rust 專測六項已通過：metadata 型別、alias/file-order/scope、section scalar、
raw redaction、共用寫鎖與 CAS、stats/health。v5 candidate 真安裝後，native KB、
search、model、下游固定契約及 raw/LLM metadata 專測共 92 passed（1.87 s）。
`test_native_kb.py` 封鎖 Python 內部 frontmatter/file reads 與 persistence，並
替換 registry、cfg、slug、hooks；以真子進程驗 writer 排斥，非只 monkeypatch
「取得鎖」函式。完整套件另於只含 `tests/` 的隔離 harness 執行，保 multiprocessing
spawn 可導入 tests，同時不讓 checkout 的 `llmwiki` 遮蔽已安裝 wheel。

完整套件已實跑 **521 passed，3.47 s**；首輪僅缺 optional `mcp`，於候選環境
補 `mcp==2.2.0` 後全通。收據見
[python-native-candidate-v5.json](results/python-native-candidate-v5.json)。wheel
SHA-256 `335941f36385f20c0a8539c59a26af0ee6da3f259d8facc430b484cb07a3f013`，
extension SHA-256 `4947b80d35af6be960511a1e8ba80e7b349acf815687ac59f8e3116304aaa815`。
收據另有四次實際 dispatch、一次 writer session、當時六個 Python facade 模組
與 source 完全相同之 hash 核對，以及字節不變、metadata 型別、未建 database 證明。
