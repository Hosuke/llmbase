# Rust 遷移契約矩陣

2026-10-10：C1 已驗雙站診斷之真實庫 ID；六項回歸、已裝候選之錯庫拒絕、
兩站固定副本各 24 路由及 159 JSON／135 HTML 烘站皆過。
此為 [#10 的一小塊](results/c1-dual-identity-2026-10-10/README.md)，未閉合
完整接入／私密授權／發版契約；原下游其後之並行修改亦未驗。本單保存後停。

**2026-09-27 最新決定：收存交接，暫停開發。** 後續以
[issues 清單](issues/README.md)及 [ADR 0002](../adr/0002-clean-rust-major-and-extensions.md)
為準：通用核心與資料／隔離／恢復保證仍須完成；舊 Python 公開契約逐項列明
保留、替代或退役及接入方法。歷史 private helper／全域 monkey-patch 的逐例
等價不再自動成為原生大版阻塞。下表保留實跑證據與原盤點，不表示已完成
此項分類，亦不抹去既有差異；實作待用戶日後恢復。

盤點及更新日：2026-09-20。舊實作基線為 `a7c4d9da764462e09c0769ecd8ba548a46d27e83`、`llmwiki 0.9.0`。本文件是遷移驗收清單，不是完成報告。**狀態欄已按目前實作／實跑證據更新**；「已驗 slice」只指所連結測例，不能當作整列所有歷史例外已閉合。「已接／待驗」是已有程式、尚無該版本測試；「仍缺」是必達工作，並未降格。舊欄中的待補 gate 保留，只有明示證據覆蓋者才視為進展。候選 Python 測試通過不等於全部通用核心已轉 Rust。

證據代碼均連到含原生 source、具體 tests、命令與結果／限制的頁：
[E-core]、[E-transport]、[E-read]、[E-write]、[E-http-model]、[E-ingest]、
[E-catalog]、[E-maint]、[E-repair]、[E-model]、[E-query]、[E-python]、
[E-jobs]、[E-ground]、[E-retrieval]。直接 facade 的逐函式 installed binding
與 code source SHA256 在 [E-live]，重跑腳本為
[audit_live_bindings.py](contracts/audit_live_bindings.py)。舊 frozen baseline 不覆寫。

[E-core]: status.md
[E-transport]: architecture.md
[E-read]: http-read.md
[E-write]: http-write.md
[E-http-model]: http-model.md
[E-ingest]: http-ingest.md
[E-catalog]: catalog.md
[E-maint]: maintenance.md
[E-repair]: maintenance-repair.md
[E-model]: model-adapter.md
[E-query]: compat-workflows.md
[E-python]: python-native-bridge.md
[E-jobs]: jobs.md
[E-ground]: results/grounding-acceptance-review-v1.md
[E-retrieval]: retrieval-baseline.md
[E-live]: contracts/live-bindings-v9.json

## 尚缺的必達通用核心：當前優先清單

2026-09-26 接續記：下列為完整待辦，不因接入 pilot 而縮減。現按使用者
指示，每次僅委派一小單，審查、獨立驗證、Git 存檔後再進下一單。
B4 寫入日誌已驗收為 `40d8bde`，見
[原始證據與界限](checkpoints/atomic-journal-2026-09-26/README.md)。
`6f2b5d9` 已驗收 helper 登記修復：新候選 575 tests、單站 taxonomy／bake
通過；當前 B6 修復原生 workspace gate，隨後驗無 Python 生命週期。
依使用者最新指示，以穩健通用核心為先，Siwen／Taixu 可循明示新接口適配。
不合理舊耦合可按具體缺陷重設，須記替代接口及驗證；不可靜默刪除必達能力。

1. **直接 Python lint facade**：B11 新候選已驗 `lint()` 與 `_load_articles`
   原生預設：35 focused／590 full Python、實際舊版八類結果一致、Siwen 真實
   cfg-only callback 一次調用與 hook 次序。見
   [B11 證據](checkpoints/native-lint-facade-2026-09-26/README.md)。
   獨立 check 函式、lint_deep、fixes/dedup 及 taxonomy 生成仍待委派。
   新候選 binding audit 已核 `resolve.build_aliases/resolve_link`
   具有原生預設分支，不再列為尚未接線；此為 binding 證據，非全語義驗收。
   Native maintenance/catalog 已存在；須保 live constants、三處 check_stubs
   identity／callables，而將預設演算法与寫入委給 Rust。此組仍待後續分單。
2. **其餘直接 Python 純核心 helpers**：`config.load_config/ensure_dirs`
   原生讀檔／路徑／逐鍵建目錄已於 B16 驗收並存 `0229032`：native26+4、
   focused63／full Python651、actual-old 型別／alias graph／路徑差分皆過，
   Python 保留明示 PyYAML 格式插件與 live profile policy，見
   [B16 證據](checkpoints/native-config-2026-09-27/README.md)。
   relative config root 重複拼接已於 B15 修復，
   原缺陷5項重現、新候選40 focused／628 full Python、真日誌與scope拒絕皆過，
   已存 `af16b2d`，見 [B15 證據](checkpoints/canonical-config-roots-2026-09-27/README.md)。
   B14 已驗 normalize_heads 的 Rust 掃描／改寫、原生規則及 Python matcher 邊界，
   60 frozen／547 獨立比對、八處 live parser 覆寫及候選618 tests 已過，
   已存 `a36d463`，見 [B14 證據](checkpoints/native-headings-2026-09-27/README.md)。
   B12 已驗 normalize_paragraphs 的
   Rust 實作、live 標點設定、229 frozen／1200 獨立差分案例及新候選595 tests，
   見 [B12 證據](checkpoints/native-paragraphs-2026-09-26/README.md)。
   當前原始碼的 `atomic_write_*`、
   `anchor`、`sections`、`split` 已有直接 native facade；此接線不再列作
   尚未實作。B4 原生 journal 已獨立通過 7 項兼容及 24 項核心測試，
   新候選六項 utility tests 與 primary exact-byte journal／conflict proof
   已過，見 `helper-registration-2026-09-26` checkpoint；其餘路徑仍逐項驗。
   installed v9 的 config／sections／atomic 無 native trace 紀錄屬歷史，
   不可拿舊候選結論覆蓋當前源碼狀態，也不可反過來以新源碼冒充舊輪驗收。
3. **Python taxonomy／Xici／entities 生成流程與 CRUD** 仍有原核心路徑。
   B13 已驗七個 taxonomy 讀取／值 helper 的 Rust 預設入口，保六處 live helper
   與遞歸 wrapper、例外 identity；native9／focused57／full Python604、357 組
   獨立差分及10 項邊界檢查皆過，已存 `24f34f5`，見
   [B13 證據](checkpoints/native-taxonomy-read-2026-09-26/README.md)。
   taxonomy 生成、assign_new_articles 及獨立 mutable helpers 尚未遷完。
   Native生成流程及私密 cache 已驗，但僅把 chat 換成 Rust 不足以遷移編排／
   儲存／dedup。须保 TAXONOMY_GENERATOR、Xici chat、ENTITY_ARTICLE_FORMATTER
   真 callback，補直接 facade 的 trace、failure、private revocation 測試。
4. **worker 舊排程與各 emitter**：native durable worker 已驗，Python
   `run_worker/start_worker_thread` 仍是既有 scheduler。CUSTOM_JOBS/LEARN_SOURCES
   可留 P handler，預設排程／state 必轉原生；10 個 hooks 的 call order／kwargs／
   exception continuation 仍需逐事件 native emission 證據。
5. **舊專用 connector CLI wrappers**：Wikisource／CBETA／ctext 的 work/list/
   status/catalog commands 尚未在新 main.rs 逐命令承接。Parser 可留 P，原典／
   companion metadata／progress 與庫 scope 不可丟；不可因 native 缺 host 就刪命令。
6. **完整 transport 相容邊界**：19 tools schema 與 native處理存在，但逐 tool 的
   stdio結果／錯誤／取消、全部CLI flags／退出碼尚未一一封存。HTTP cache的
   Last-Modified、config熱載入、graph depth 5及若干舊200-error差異仍明列於 [E-read]。
   Root正補 search serve 與 Web static/SPA；尚需真前端閱讀／問答全流。
7. **終驗／量測**：新版Linux release無Python chroot全生命週期、最新workspace
   fmt/clippy/tests、當前候選、fresh Siwen/Taixu trace/bake、copy-first rollback舊版實读、
   cold/RSS/disk/增量成本尚待收斂。合成 transcription 入口已有實作與測試，
   終驗仍須覆當前版本，不另起重複開發。真embedding語義品質
   未驗，須保持未驗，不用mock engineering或lexical滿分代替。
8. **發布** 已獲使用者後續授權；README/changelog/tag/GitHub Release/PyPI/
   ClawHub需在上述 gates 後依倉規完成。生產切換與改真文庫仍未授權。

完整逐符號清冊在 [contracts/inventory.md](contracts/inventory.md)，機讀快照在 [contracts/baseline.json](contracts/baseline.json)。快照由 [collect_contracts.py](contracts/collect_contracts.py) 靜態抽取，含 42 個 CLI command/group nodes、57 條 HTTP route declarations（Web 42、Agent 13、legacy search 2）、19 個 operation/MCP tools、222 個 Python 公開符號／re-exports／已知私有 helpers，以及 424 個現有 test functions。每列帶來源行號；JSON 另保留完整 schema、defaults、檔案 SHA-256。這是基線快照，遷移後勿重新覆寫它以掩去舊契約。

分類用語：

- **Rust 原生（N）**：儲存、索引、檢索、問答／編纂調度、圖投影、任務狀態及所有通用核心能力。CLI／HTTP／MCP 共用 application service；Python facade 不得自己執行舊核心。
- **Python 擴展邊界（P）**：Flask 相容外殼、runtime monkeypatch／callback、現有網站與特殊格式 connector。P 仍是必達相容項；只把外殼或插件放 Python，不把核心降格。
- **明確非目標（X）**：僅原規格明示排除者；見末節。沒有一项既有通用核心能力被列 X。

表中測試欄是既有回歸起點或明確待補的 gate；未表示該檔案已測遍一整列。完整 test function／來源索引見附錄。所有既有測試須先固定基線，允許安全修正或明確 bug 修正，但須留下新舊行為與理由，不能默默改預設。

## 必先保住的入口差異

| 入口 | deep | file_back | promote | 回傳形狀／副作用 | 分類、狀態、驗證 |
|---|---|---|---|---|---|
| [CLI query](../../llmwiki/cli.py#L342) | false | false | 無直接 flag | Rich 人類答案；`--deep` 時 `--format` 不傳入 deep helper；需保存才寫 outputs | N / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |
| [query.query](../../llmwiki/query.py#L77) | 不適用 | false | 不適用 | 預設 `str`；`return_path=True` → `{answer,output_path}` | N + P facade / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |
| [query_with_search](../../llmwiki/query.py#L137) | deep helper | false | false | 預設 str；`return_context=True` 才回 dict 並執行 promote；空 index 仍回 str | N + P facade / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |
| [operations/MCP kb_ask](../../llmwiki/operations.py#L114) | true | false | false | `{answer,consulted?,output_path?,promotion?}`；deep 總請求 context | N / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |
| [KnowledgeBase.ask](../../llmwiki/agent_api.py#L47) | false | true | false | `{status:'ok',answer,...}`；未經 operations dispatch | N + P facade / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |
| [Agent POST /api/ask](../../llmwiki/agent_api.py#L198) | false | true | false | `{status:'ok',**op_result}`；走 dispatch；此 wrapper 未轉傳 model/api_key | N + P facade / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |
| [Web POST /api/ask](../../llmwiki/web.py#L914) | false | true | false | `{answer,output_path?}`；deep 再加 consulted/promotion；同步 request | N + P facade / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |
| 新 native ask | 明確檢索政策 | false | false | 版本化 evidence／answer；保存／compile／promote 另明示，不能繼承 Web 寫入預設 | N / partial；defaults 已按 native HTTP6／CLI workflow tests 對照，直接facade與每例外仍逐項驗 [E-query] |

`promote=True` 在舊 `deep=False` 路徑被忽略；deep helper 又以 `return_context=True` 為門檻。facade 須保已記錄的舊行為，新 API 可拒絕矛盾輸入。`kb_ask.writes=False` 並非無副作用；registry 只為 `promote is True` 升級鎖，`file_back=True` 未升級。`kb_lint(fix=True)` 也升級鎖。新共同排他機制須修此不足，不能以 bool metadata 判斷權限。

促成升條的 judge **不沿用** request `model`／`api_key`，selector 與 answer 才沿用，見 [query_with_search](../../llmwiki/query.py#L137)。不可改成把使用者 key 傳到所有模型操作而不立契約。

## CLI：名稱、參數、退出碼與副作用

所有 flags／choices／型別／defaults 已逐個列於 [CLI 清冊](contracts/inventory.md#cli)；以下逐命令補責任与目前驗證狀態。此段描述舊 Click 契約：`--base-dir` 預設 `.` 且須已存在，轉為 absolute；`-v/--verbose` 為 count（INFO／DEBUG+HTTP／wire）。每層保 `--help`。Python 安裝入口保 [`llmbase = llmwiki.cli:main`](../../pyproject.toml)，native binary 另有獨立入口。一般 Click usage error 為 2；無另設退出者即使印缺失或 lint issues 仍成功，不能假定「有警告必非零」。

| 命令 | 必守行為／差異 | 分類 | 既有測試／待補 | 狀態 |
|---|---|---|---|---|
| `ingest url URL` | 網站轉 markdown／來源 metadata／images；儲存交核心 | P connector + N | connector HTTP fixture、SSRF gate 待補 | 已接；core URL／原件測試過，CLI新路徑待驗 [E-ingest] |
| `ingest file FILE_PATH` | 接本地原檔，unknown frontmatter 保留；舊回傳 Path | N | ingest parts in test_v064_features；raw bytes gate 待補 | 已接；core bytes 測試過，CLI suffix parity 待驗 [E-ingest] |
| `ingest pdf PDF_PATH --chunk-pages 20` | 0 表整文；PDF parser 可插件，來源／jobs 原生 | P + N | parser fixture／缺能力錯誤待補 | 已接明確 parser；HTTP5過，CLI parser flags待逐項核 [E-ingest] |
| `ingest dir DIR_PATH` | 只掃一層，`.md/.markdown/.txt/.pdf/.py/.json/.csv` | N | 目錄／suffix／排序待補 | 已接；逐 suffix／nested/退出碼尚待驗 [E-ingest] |
| `ingest browse URL` | opencli，失敗 fallback HTTP；舊路徑硬寫 base/raw，與 config 不一致須顯式修正 | P + N | fake adapter／custom root 待補 | 已接 process 邊界；真 browser fallback 契約待驗 [E-ingest] |
| `ingest wikisource-learn --list ... --batch 3` | reading-list choice 與 batch；插件依然可用 | P | 無專項測試；source adapter 待補 | 仍缺 native CLI wrapper／host契約測試；connector P 能力不可刪 [E-ingest] |
| `ingest wikisource-work TITLE` | 分頁原典入庫 | P + N | 插件測試待補 | 仍缺 native CLI wrapper／host契約測試；connector P 能力不可刪 [E-ingest] |
| `ingest cbeta-learn --category ... --batch 5` | 逐批進度與去重不能丟 | P + N | progress 恢復 fixture 待補 | 仍缺 native CLI wrapper／host契約測試；connector P 能力不可刪 [E-ingest] |
| `ingest cbeta-status` | total_ingested／last_run／ingested_works | P | 舊 metadata fixture 待補 | 仍缺 native CLI wrapper／host契約測試；connector P 能力不可刪 [E-ingest] |
| `ingest cbeta-work WORK_ID` | 已有／找不到目前印提示而非報錯 | P + N | 缺資料／已有待補 | 仍缺 native CLI wrapper／host契約測試；connector P 能力不可刪 [E-ingest] |
| `ingest ctext-book BOOK_NAME BOOK_PATH --delay 1.5 --browser` | HTTP／browser 二路；逐章入庫 | P + N | 外部 process adapter 待補 | 仍缺 native CLI wrapper／host契約測試；connector P 能力不可刪 [E-ingest] |
| `ingest ctext-catalog CATALOG --delay 1.5 --browser` | choice 特有 `histories`，不可與 wikisource `history` 混併 | P | catalog fixture 待補 | 仍缺 native CLI wrapper／host契約測試；connector P 能力不可刪 [E-ingest] |
| `ingest list` | raw title/type/compiled/ingested_at | N | raw metadata fixture 待補 | 已接；新增 CLI gate 待驗 [E-ingest] |
| `compile new --batch-size None` | None 落 config compile.batch_size 或 10；hooks、dedup、merge、來源 metadata | N + P hooks | [test_compile](../../tests/test_compile.py)、languages；失敗寫入 gate 待補 | 原生 accepted compile 已驗；每 flag 對照仍部分 [E-query] |
| `compile all` | 現行 reset flags、batch_size=999，卻未清 compiled_sources；屬不能沿襲的完整重編缺陷 | N | 完整來源重編／不改原文 gate 待補 | 原生已接；receipt 重編修正 9 workflow tests 過 [E-query] |
| `compile index` | 回建 index.json、_index.md、aliases、backlinks；有 hook | N | compile/resolve；原子切換待補 | 原生已驗；9 maintenance tests [E-maint] |
| `query QUESTION --format markdown --tone default --file-back --deep` | format choices markdown/marp/chart；tone 5 個固定 choices，任意新增 tone 不自動進 CLI | N + P overrides | query_promote/query_prefilter；退出码待補 | 原生已驗基本／deep／format／tone／file-back；全 flag 矩陣仍部分 [E-query] |
| `search query QUERY_TEXT --top-k 10 --json-output` | 舊 JSON honour top_k；人類路徑 search_cli 未轉 top_k（固定 10），須記錄修正 | N | [test_search_raw](../../tests/test_search_raw.py)；CLI 對照待補 | 原生已驗 lexical／JSON；全舊人類輸出差異待封存 [E-transport] |
| `search serve --port None` | port 取 config.search.port 或 5555；第三個 search Flask factory | N + P Flask | route/HTML 小 UI 待補 | 正在接 native search_router／HTML；待驗 [E-transport] |
| `lint check` | structural issues 不改 exit 0；categories + total_issues | N | [test_lint](../../tests/test_lint.py)；CLI 退出码待補 | 原生 core 已驗；CLI 接線待新批驗 [E-maint] |
| `lint deep` | 模型品質報告 | N + P prompt | deterministic provider 待補 | 原生 core／HTTP 已驗；CLI 新接線待驗 [E-repair] |
| `lint fix` | 自癒 pipeline 真修內容、metadata、links、dedup、taxonomy | N | test_lint；整链與失敗待補 | 原生 repair 13 項過，含新 receipt／private／failure [E-repair] |
| `lint normalize-tags` | 模型判同義；修改文章 tags | N | test_lint；模型 fixture 待補 | 原生 normalize 已驗；CLI 接線待新批驗 [E-repair] |
| `lint clean` | clean_garbage、需要時 rebuild | N | test_lint；metadata preservation 待補 | 原生 clean 已驗；CLI 接線待新批驗 [E-repair] |
| `lint dedup` | check_duplicates，再 merge_duplicates；合併與關聯不丟 | N | test_lint；來源支持與合併證據待補 | 原生證據／合併已驗；CLI 接線待新批驗 [E-repair] |
| `lint heal` | check→fix→recheck→health.json；零 issues 不生成新 report | N | 周期流程／退出碼待補 | 原生 check-fix-report 已驗；舊零 issue 差異明列 [E-repair] |
| `stats` | raw 子項數、concepts `*.md`、outputs `*.md`、whitespace word count | N | 不把新 CJK token 計數暗換舊 total_words；待補 | 原生已驗；舊 words 計數保留 [E-transport] |
| `serve --port 5556` | Agent factory 路由／envelope | N server + P factory | 獨立 contract suite 待補 | 原生 Agent routes 已驗；全逐路徑 parity 仍部分 [E-transport] |
| `web --port 5555` | Web factory；dev 本身不啟 worker | N server + P factory | web tests；前端閱讀／問答 gate | native Web API 已驗；static/SPA 接線待驗 [E-read] |
| `mcp` | 改為 stdout 僅 protocol；舊程式先印 Rich banner，且 argparse 再讀 CLI argv 有衝突，不得保留該缺陷 | N | 真 stdio 初始化／tools/list／call 待補 | 官方 SDK stdio lifecycle 已驗；全部 tool 個別 subprocess 待補 [E-transport] |
| `export article SLUG` | JSON；not found 舊 CLI 印提示仍 exit 0 | N | [test_export](../../tests/test_export.py) | 原生 export 已驗；舊缺失退出碼待對照 [E-maint] |
| `export tag TAG` | tags case-insensitive；回 `{tag,count,articles}` | N | test_export | 原生 tag export 已驗 [E-maint] |
| `export graph SLUG --depth 2` | alias-aware root；outgoing+backlinks BFS | N | test_export；bounded native depth 待補 | 原生 bounded graph 已驗；depth 上限差異未閉合 [E-maint] |
| `ops list` | 列 runtime registry，含下游新註冊與替換 | N + P registry | registry mutation test 待補 | native 靜態 registry 已驗；動態 P registry 另保留 [E-transport] |
| `ops call NAME --json-args '{}'` | JSON result；unknown/invalid JSON/non-object exit 1，TypeError 2，busy 3；stderr errors | N + P registry | subprocess exit/stdout contract 待補 | native DTO／scope 已驗；所有舊退出碼映射待補 [E-transport] |

新 JSON 輸出、doctor、migration CLI 必另增，不能只保 `ops call` 而移除常用子命令。README 的「三端全由 operations 驅動」比現狀強；實際多數 CLI 直接 import helpers。`kb_search_raw` 沒有專用 `search raw` 子命令，既有 CLI 是 `ops call kb_search_raw`。

## HTTP Web factory（42 條 route declarations）

Factory 舊契約為 [`create_web_app(base_dir=None)`](../../llmwiki/web.py#L224)，是 **P 外殼／N 核心**；不得改回 non-Flask object。每條 handler 名稱與 source link 見 [HTTP 清冊](contracts/inventory.md#http包括第三個-legacy-search-factory)，供 `app.view_functions` 相容。除列出 status 者，成功 payload 無通用 status envelope。`A` 表舊 `@require_auth`；ask 額外自行驗證。目前 native rows 已驗狀態見末欄；P factory仍保持Flask。

| 方法／路徑 | 參數／回應／副作用 | 舊 auth | 測試起點／待補 gate | 狀態 |
|---|---|---|---|---|
| GET `/api/healthz` | `{status:'ok'}`，禁止檔案／模型／worker I/O | 無 | 無 I/O liveness 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/branding` | name/nameShort/tagline/poweredBy；每次 load config | 無 | downstream branding replacement | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/languages` | normalized profile；invalid config `{error}`/500 | 無 | test_web_languages | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/stats` | raw_count/article_count/output_count/total_words/link_count/health_score | 無 | payload snapshot 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/taxonomy` | lang profile；`{categories}`，ETag/Last-Modified/304；直接改 concept 也失效 | 無 | test_web_languages + cache 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/collections` | tags 分組，空 tag→uncategorized；config labels；`{collections}` | 無 | tag/string型別與 labels 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/articles` | 舊無 query→`{articles}`；任一 limit/cursor/tag/q/fields 啟新 envelope `{articles,total,count,next_cursor,filters}`；limit 1..1000，cursor slug-exclusive；ETag | 無 | test_web_articles；不假稱 Agent 同形 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/articles/lite` | `{articles:[{slug,title}],total}`，只讀 index.json；tag exact；missing→空/200，malformed→空/500；ETag／cache env | 無 | test_web_articles，route precedence | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/articles/<path:slug>/sections` | alias-aware `{status:'ok',slug,title,sections}`；invalid path 400，missing 404 | 無 | test_sections；Unicode scalar offsets | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/articles/<path:slug>` | alias-aware `{status:'ok',slug,title,summary,tags,sources,content,backlinks}`；source 非 http(s) URL 清空 | 無 | alias/path/source sanitization；downstream bake | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/aliases` | `{aliases:{name:slug}}` | 無 | test_resolve + downstream view replacement | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/export/article/<path:slug>` | structured export payload；not found `{status:'error',message}`/404 | 無 | test_export + HTTP snapshot 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/export/tag/<tag>` | `{tag,count,articles}` | 無 | test_export | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/export/graph/<path:slug>` | depth default 2、夾 0..5；bad int 400；{root,depth,nodes,count} | 無 | test_export + clamping 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/entities` | people/events/places 的持久結果 | 無 | test_entities | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| POST `/api/entities/extract` | 同步抽取、儲存、emit hook；回結果 | A | test_entities + auth 待補 | 已驗 native model/read slice；HTTP model6及last_fix真artifact過 [E-http-model] |
| GET `/api/refs/plugins` | `{plugins:[{id,name}]}`；P connector registry | 無 | dynamic plugin lookup 待補 | 3 builtin descriptors 已接；dynamic P registry另驗 [E-ingest] |
| GET `/api/trails` | `{trails}`；private 研究軌跡 | A | 邊界／備份／公開 bake 排除待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| POST `/api/trails` | trail_id/name/step；server ts 覆蓋；新 id 12 hex；回 `{trail}`；step 非 dict 400 | A | create/append/missing/concurrent 待補 | native write已驗；HTTP write1/core3；逐條錯誤差異明列 [E-write] |
| POST `/api/trails/<trail_id>/delete` | idempotent 刪除，`{status:'ok'}` | A | 原子資料／library scope 待補 | native write已驗；HTTP write1/core3；逐條錯誤差異明列 [E-write] |
| GET `/api/xici` | lang 預設 profile api_default_lang；返回快取 | 無 | test_web_languages | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| POST `/api/xici/generate` | lang 驗 profile、LANG_STYLES；同步模型生成與儲存 | A | test_web_languages + failure 待補 | 已驗 native model/read slice；HTTP model6及last_fix真artifact過 [E-http-model] |
| GET `/api/search` | q='',top_k=10；`{query,results}` | 無 | search raw tests 僅 scorer 起點；Web snapshot 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| POST `/api/ask` | defaults 見上表；model string <=200、strip；X-LLM-Key <=500；nested body key variants 拒 400；deep busy 409 | 自行 gates | test_xllm_key、test_v064_features；新 auth 修正另立 gate | 已驗 native model/read slice；HTTP model6及last_fix真artifact過 [E-http-model] |
| GET `/api/tones` | `{tones}`；固定 5 款與 TONE_INSTRUCTIONS 交集，自定新 id 不自動列出 | 無 | 動態 dict mutation snapshot 待補 | 已驗 native model/read slice；HTTP model6及last_fix真artifact過 [E-http-model] |
| GET `/api/sources` | `{documents}`，list_raw 已 sanitizes path/private metadata | 無 | test_v064_features，公開資料 gate | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/sources/<path:slug>` | index.md preview；content cap default 50k，null→500k，int 夾 0..500k；metadata 全 string；missing 現行 200 error | 無 | cap／traversal／公開 metadata gate 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| POST `/api/ingest` | source；dispatch kb_ingest；`{status:'ok',path}`；busy 409 | A | SSRF、scope、connector fixture 待補 | native 薄映射已驗；5 ingest HTTP tests 過 [E-ingest] |
| POST `/api/upload` | multipart file；PDF 20 pages/chunk→`{status:'ok',chunks,filename}`；其他 `{status:'ok',path,filename}` | A | filename/path/size/cleanup/failure 待補 | native 薄映射已接；5 新 ingest HTTP tests 待驗 [E-ingest] |
| DELETE `/api/articles/<slug>` | 成功 `{status:'ok',deleted}`；缺失 200 error；舊無 rebuild，native 須即移衍生片段 | A | tombstone/index no-stale gate 待補 | native write已驗；HTTP write1/core3；逐條錯誤差異明列 [E-write] |
| POST `/api/wiki/clean` | garbage templates/<50 chars 清理；{status:'ok',removed,slugs}；有刪重建 | A | fixture/no-unrelated-deletion 待補 | native write已驗；HTTP write1/core3；逐條錯誤差異明列 [E-write] |
| POST `/api/taxonomy/update` | `{categories:[...]}`；強設 locked=true，回 counts；此為人工權威內容 | A | preserve custom metadata／lock 待補 | native write已驗；HTTP write1/core3；逐條錯誤差異明列 [E-write] |
| GET `/api/worker/status` | `{busy: job_lock.locked()}`，非持久 job 狀態 | A | test_v064_features；native mapping 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| POST `/api/compile` | 不收 full，dispatch 空 args；僅回 `{status:'ok',articles_created}`，不回 articles list | A | Web/Agent 差異待補 | 已驗 native model/read slice；HTTP model6及last_fix真artifact過 [E-http-model] |
| POST `/api/lint` | deep false→`{results}`；true→`{report}` | 無 | test_lint + paid-op auth 待補 | 已驗 native model/read slice；HTTP model6及last_fix真artifact過 [E-http-model] |
| POST `/api/lint/fix` | 非同步 daemon thread，立回 `{status:'started',message}`；last_fix.json 輸出；舊可能 busy 還回 started | A | 新 persisted job/failure/legacy envelope 待補 | 已驗 native model/read slice；HTTP model6及last_fix真artifact過 [E-http-model] |
| GET `/api/health` | `{report: null|health_json}` | 無 | missing/persisted fixture 待補 | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| GET `/api/wiki/export` | `{articles:{slug:{metadata,content}},count}`；unknown metadata 全收；不能等同完整 backup | 無 | private/internal/new DB fields public exclusion gate | native read已驗；HTTP3過，core新增empty-trails gate待重驗 [E-read] |
| POST `/api/index/rebuild` | dispatch，`{status:'ok',article_count}`；busy 409 | A | atomic switch／metadata contract 待補 | native write已驗；HTTP write1/core3；逐條錯誤差異明列 [E-write] |
| GET `/favicon.svg` | base/static/favicon.svg 優先，後 static_dir；P static facade | 無 | downstream endpoint replacement | native static/SPA與scoped roots已驗3tests；P Flask hooks保留 [E-read] |
| GET `/` | serve_spa，static index；舊會派 auth cookie；P static facade | 無 | test_client bake + 新權限邊界 | native static/SPA與scoped roots已驗3tests；P Flask hooks保留 [E-read] |
| GET `/<path:path>` | 實檔或 SPA fallback；P static facade | 無 | static_dir containment + downstream routes | native static/SPA與scoped roots已驗3tests；P Flask hooks保留 [E-read] |

Web cache 還包含 `LLMBASE_LITE_CACHE_MAX_AGE`；`If-None-Match` 需保 wildcard、weak/strong、多 tag。`total` 是全庫檔數，不是 filter 後數。`fields` 只影響 entries，不改 pagination envelope。[test_web_articles.py](../../tests/test_web_articles.py) 有針對性 assertions，不能只驗 200。

安全差異屬原 goal 明定修正：Web `require_auth` 目前接受公開 SPA 所派 `llmbase_auth`，普通 ask 也會付費／file_back；Agent factory 完全沒有 auth gate。新遠端服務和 facade 部署都須另驗 caller scope/capabilities；訪客 cookie 不能授權寫入／付費。舊安全測試期待 cookie promotion 的部分，須作明確新契約測試，不假裝 byte-for-byte 延續。原典／metadata/LLM 輸出不得拼 shell 或任意 path。

## HTTP Agent factory（13 routes）與 legacy search（2 routes）

[`create_agent_server(base_dir=None, port=5556)`](../../llmwiki/agent_api.py#L175) 回 Flask app，不直接 listen；port 未進 app config。構造 `KnowledgeBase` 時 load config + ensure_dirs。全部 **N 核心／P Flask**。初始倉無專門 Agent factory tests；現有 native HTTP model/read/ingest tests 已補兩factory對照，未覆各例外仍明列。

| 路徑 | 方法 | 原 payload／參數／錯誤 | 狀態 |
|---|---|---|---|
| `/api/ingest` | POST | required source；`{status:'ok',path}`；RuntimeError→409 busy | 已驗 native；兩factory ingest及upload5 tests過 [E-ingest] |
| `/api/compile` | POST | full=false；`{status:'ok',articles_created,articles}`；409 busy | 已驗 HTTP兩factory defaults/envelopes；model6過 [E-http-model] |
| `/api/ask` | POST | required question；deep=false,file_back=true,tone=default,promote=false；`{status:'ok',**result}`；409 busy | 已驗 HTTP兩factory defaults/envelopes；model6過 [E-http-model] |
| `/api/search` | GET | q='',top_k=10；`{status:'ok',results}`，沒有 Web query 欄 | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |
| `/api/lint` | POST | deep=false→`{status:'ok',results}`；deep=true→`{status:'ok',report}` | 已驗 HTTP兩factory defaults/envelopes；model6過 [E-http-model] |
| `/api/lint/fix` | POST | 同步 dispatch；`{status:'ok',fixes,fix_count}`；不是 Web started | 已驗 HTTP兩factory defaults/envelopes；model6過 [E-http-model] |
| `/api/health` | GET | `{status:'ok',report}`；未生成再有 message | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |
| `/api/sources` | GET | `{status:'ok',documents}` | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |
| `/api/articles` | GET | `{status:'ok',articles}`；忽略 Web pagination params | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |
| `/api/articles/<slug>` | GET | 直接 filename，未 alias-aware；`{status:'ok',slug,title,summary,tags,content}`；missing 200 `{status:'error',message}` | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |
| `/api/index/rebuild` | POST | `{status:'ok',article_count}`；409 busy | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |
| `/api/op/<name>` | POST | JSON args；`{status:'ok',result}`；unknown 404、RuntimeError 409、TypeError 400、other 500，error 欄 | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |
| `/api/op` | GET | `{status:'ok',operations:[{name,description,params,writes,category}]}`；runtime registry | native Agent DTO已接已驗；registry custom P另驗 [E-transport] |

[`search.create_search_app`](../../llmwiki/search.py#L227) 為第三 factory：GET `/` 回 inline search HTML；GET `/api/search?q=&top_k=10` 回 `{query,results}`。須保 `llmbase search serve`，但不需全新前端。現已有原生 search_router＋search HTML，static_ui3 tests 已過；CLI flags全對照仍待驗。

## MCP／operations（19 tools）

全 schema、required/default/enum/writeOnly 在 [baseline.json](contracts/baseline.json) 與 [逐 tool 清冊](contracts/inventory.md#mcpoperations)。以下全 **N 核心**，當前已保留 19 schemas 並接原生工具，完整逐 tool subprocess contract 仍部分。舊 `register(Operation(...))` 支持 last-write-wins，不驗 JSON Schema；dispatch 實際以 Python kwargs 呼叫，未知 args/缺 required 通常 TypeError。自定 handler 是 **P callable**，native 無 host 時須能力錯誤，不可略去。

| Tool | 回應／特有行為 | 現有測試起點／缺口 | 狀態 |
|---|---|---|---|
| `kb_search` | query required,top_k=10；`{results}` concepts | scorer tests；golden lexical 待補 | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_search_raw` | query required,top_k=10；`{results}` raw；勿冒充 hybrid | test_search_raw | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_ask` | defaults 見上；api_key writeOnly metadata，不代表傳輸自動 redacts；真正 core 用 key | test_query_promote/test_xllm_key | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_get` | required slug，可 section；found bool、alias-aware；missing section 返回 section_found=false 且無 content | test_sections | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_get_sections` | required slug；found/slug/title/sections；Python offsets | test_sections | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_list` | optional tag exact-match；`{articles}` | 新契約待補 | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_backlinks` | required slug；`{slug,cited_by}`；無檔多 note；舊未 resolve alias | test_resolve/scoped graph 待補 | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_taxonomy` | lang=zh；`{categories}`；無 Web profile 校驗 wrapper | test_languages | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_stats` | articles/raw_documents/filed_outputs/total_words；與 Web keys 不同 | 新契約待補 | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_ingest` | source 或 legacy url；兩者皆缺 TypeError；`{path}` | ingest fixture 待補 | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_compile` | full=false；`{articles_created,articles}`；writes=true | test_compile + transport 待補 | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_lint` | deep=false,fix=false；fix 優先；普通回 `{report:dict|string}`，與 HTTP results 不同 | test_lint + mutation gate | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_lint_fix` | `{fixes,fix_count}`；writes=true | test_lint | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_export_article` | required slug；export payload 或 `{found:false,slug}` | test_export | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_export_tag` | required tag；`{tag,count,articles}` | test_export | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_export_graph` | required slug,depth=2；`{root,depth,nodes,count}` | test_export | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_export` | legacy type article/tag/graph,slug,depth=2；tag 分支把 slug 作 tag；invalid TypeError | legacy alias contract 待補 | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_rebuild_index` | `{article_count}`；writes=true | test_compile | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |
| `kb_xici` | lang=zh；get cached dict | test_languages | 19舊schema保留、native tool已接；逐tool真stdio覆蓋仍部分 [E-transport] |

[`mcp_server.create_server`](../../llmwiki/mcp_server.py#L73) 使用官方 SDK `Server('llmbase')`，tools/list 每次讀 registry，tools/call 回 `[TextContent(type='text',text=...)]`，dict 以 ensure_ascii=False／indent=2／default=str JSON 化；string 原樣。未知 tool raises ValueError；busy 回一般 text `Busy: ...`，shim [`handle_tool`](../../llmwiki/mcp_server.py#L46) 又是 `Unknown tool: ...`／`Another write operation is running. ...`。facade shim 格式保留；native MCP 須合法協議的 result/error，不得自己拼方言或回空成功。官方 SDK 的 initialize→initialized→tools/list→call/error→EOF 與純stdout subprocess 已由 transports8驗；每tool個別結果、所有取消／錯誤互動仍待覆。handle_tool 單元測試本身不等於 stdio證據。

## Python 語义、runtime customization 與下游

222 個逐符號簽名、預設與來源在 [Python 清冊](contracts/inventory.md#python-公開符號與已知私有消費者)。以下補應驗語義；所有 **N** 表中的 Python import 仍須存在，只是核心換 Rust。候選 wheel 要在隔離 env 裝入，在 siwen fixture 副本跑測試，證明實際 Rust 路徑；不可只讀版本或 marker。詳見 [downstream.md](downstream.md)（下游審計）及 [11 個下游基線測試](../../tests/rust_migration/test_downstream_contracts.py)；舊 Python 基線通過不表示 native 已達標。斯文自有 compile fork 直接寫文，單一 rebuild Rust marker 不能證明 compile 核心落入 Rust。

| 契約／來源 | 必守語義／能力責任 | 分類 | 既有測試／待補 | 狀態 |
|---|---|---|---|---|
| config.load_config / ensure_dirs | config.yaml 位於 base；只有無檔才用完整 defaults，現有檔不 deep merge；5 paths 按 base resolve，可 absolute/custom roots；base_dir str | N + P format/profile | test_native_config、test_native_config_roots；actual-old graph/path/mkdir 對照 | B16 原生 IO／順序／路徑已驗；PyYAML 與 live policy 為明示邊界；原相對根／journal／scope 回歸仍過 [B16](checkpoints/native-config-2026-09-27/README.md) |
| compile sanitize_slug / compile_new / compile_all / rebuild_index | generic 編纂、dedup、metadata、merge、hooks、taxonomy assignment；不是 RPC 回舊 Python | N | test_compile、languages；native dispatch evidence | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| compile._find_uncompiled / _list_existing_concepts | siwen 直接 import，Path/list 型別及 filename 規則 | N facade | siwen compile tests；unknown frontmatter fixture | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| compile._parse_compile_response / _write_article / _merge_into / _split_sections / _assemble_sections | 模型格式解析、既有 helper kwargs（headers=）、bare-body 與 merge | N facade | test_compile、test_languages；empty/truncation failure | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| query/query_with_search/promote_to_concept | str/dict 返回差異、output_path、consulted 真含義、promote judge、去重重建、寫入政策 | N | test_query_promote、test_query_prefilter | default orchestration已遷、live callback保留；v9已驗 [E-query] |
| search/search_raw/search_cli | tokenizer、scores/snippet/source fields、raw/compiled 分型、top_k | N | test_search_raw；固定 lexical 基準／CJK matrix | native TF-IDF＋callable tokenizer；v9過，public新case待v10 [E-python] |
| resolve.build_aliases/save_aliases/load_aliases/resolve_link | one library aliases、title/slugs/explicit aliases/繁簡；fallback 順序；None missing | N | test_resolve；跨库同名拒誤解 | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| taxonomy generate/build/load/assign_new_articles | hierarchical category tree、labels、locked 人工覆蓋、category tags、副作用 | N + P generator | test_languages/web_languages；manual relation backup | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| xici generate/get | cached language intro；LANG_STYLES／chat override；儲存是需保存內容 | N + P model callback | test_web_languages；siwen patches | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| entities extract/get + _parse_entity_response/_dedup_entities/_save_entities | 下游 import 私有 helpers 且 replace extract_entities；persons/events/places metadata | N + P override | test_entities；siwen batch extractor | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| lint package reexports/checks/fixes/dedup | lint,lint_deep,all checks,auto_fix,normalize_tags,fix_dirty_tags,clean_garbage,fix_uncategorized,fix_broken_links,merge_duplicates,_find_duplicate_candidates | N + P overrides | test_lint；三处 check_stubs identity | B11 已驗 lint/load 原生預設及 live callback；獨立 checks/deep/fixes/dedup 仍待委派 [B11](checkpoints/native-lint-facade-2026-09-26/README.md) |
| llm.chat / chat_with_meta / LLMMeta | str vs (str,meta)，finish_reason/usage/reasoning_tokens/attempts，length-cut 不由 primitive raise；override 每呼叫生效 | N + P callbacks | test_llm/test_llm_meta/test_xllm_key | 真native HTTP adapter＋P wrapper；v9候選已驗 [E-model] |
| llm.chat_with_context / extract_json / reasoning_budget / strip_surrogates | context 限額、JSON fence/reasoning stripping、caller tokens_per_char、合法 Unicode | N facade | test_llm_meta/test_v064_features + siwen | 真native HTTP adapter＋P wrapper；v9候選已驗 [E-model] |
| llm.get_client/get_default_model/get_fallback_models | Python SDK client 是可保留的 P 相容物件；production generation/embedding/retries/cancel Rust 原生；勿讓核心偷用舊 client | P + N adapter | test_llm；native fixture HTTP gate | 真native HTTP adapter＋P wrapper；v9候選已驗 [E-model] |
| anchor.Anchor / locate_span / normalize_text | exact/head_only/None；empty head ValueError；offset 為原 Python string codepoint，normalize 不改原文 | N facade | test_anchor；非 BMP/combining/variant mapping | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| sections.Section / normalize_title/make_anchor/parse_sections/find_section/extract_section_text | TypedDict tree，含 subtree start/end；ATX/fence 規則；`h{level}-{slug20}-{sha1_6}` 與 duplicate suffix | N facade | test_sections；JS UTF-16 不可當 Python offset | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| split.Section / split_by_heading | dataclass（不是 sections.TypedDict）、指定 level 平分、preamble/line offset 規則，不加 heuristics | N facade | test_split | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| normalize.HeadRule / normalize_heads / normalize_paragraphs | rule pack、regex capture、安全結構、bytes/span 映射在衍生層 | N + P rule capability | test_normalize；native regex 明列 dialect；Python matcher 真回呼 | B12 paragraph、B14 heads 已驗；保 live 標點／八處 parser 覆寫及 Python regex，原生規則獨立執行 [B14](checkpoints/native-headings-2026-09-27/README.md) |
| atomic.atomic_write_json / atomic_write_text | tempfile+rename；讀者不見半檔；權威內容需 journal／DB coordination | N facade | test_atomic；雙寫／crash 待補 | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| ChunkCache(base,subdir).root/get/put/clear | cid+content_hash full SHA-256 key；str output；missing/OSError get→None；clear 僅 cid、unknown idempotent | N facade | test_chunk_cache；旧 cache 導入重用 | native storage/lease/codec已遷；v9過，v10最新待驗 [E-python] |
| pipeline.run_stage/StageContext/StagePartialExit/StageBusyError | contextmanager；ok/failed/partial terminal guarantee；custom log/artifact/meta_update；callback 在 P，狀態在 N | N + P handler | test_pipeline；Rust/Python 同鎖/重啟 | native storage/lease/codec已遷；v9過，v10最新待驗 [E-python] |
| pipeline.StageState/rebuild_state/RESERVED_EVENTS | last round、last terminal、attempt count、all-round artifacts union、last-round meta；JSONL 原格式 | N facade | test_pipeline；torn bytes、PID reuse | native storage/lease/codec已遷；v9過，v10最新待驗 [E-python] |
| agent_api.KnowledgeBase 所有方法 | __init__ ensure_dirs；返回均 status 包裝，get_article/export missing 200-like dict；方法清冊逐列 | N + P facade | 新完整 Python contract tests | 部分built-in core已遷；動態P保留；其他helper仍缺 [E-python] |
| operations.Operation/register/get/all_operations/dispatch | 可替換 handler，保 insertion order，unknown KeyError；schema 自省與 args errors；共同跨進程鎖 | N + P callback | sections/query_promote/xllm_key/v064 部分；動態註冊待補 | 部分built-in core已遷；動態P保留；其他helper仍缺 [E-python] |
| hooks.register/emit/clear | register append，emit 按序 kwargs，單 hook exception warning swallowed，其後續跑；clear(event=None) | P callbacks + N event source | 10 個事件逐一 fixture 待補 | P舊hook/connector保留；對應原生emitter與每hook trace仍待驗 [E-live] |
| worker.run_worker/start_worker_thread/job_lock | enabled=false 默认；曾使用 in-process Lock；once-per-process worker；原生 jobs 狀態／lease，P 自定 handler | N + P callbacks | test_v064_features/test_languages；persisted retry/cancel | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| browser/cbeta/ctext/wikisource/pdf/sync/refs | 仍可用的外部 parser/connector/plugin，不要求 Rust 重寫第三方格式；原生入口缺 host 明確 error | P | adapter contract fixture，不碰生产／网络付费 | P舊hook/connector保留；對應原生emitter與每hook trace仍待驗 [E-live] |
| web create_web_app/require_auth/derive_session_token | 真 Flask + test_client；module decorator 讀 current_app.config；app.view_functions/url_map/config 可修改 | P + N operations | web tests + siwen bake/middleware/full native tracing | P舊hook/connector保留；對應原生emitter與每hook trace仍待驗 [E-live] |

### Runtime 可變設定逐項矩陣

每次 facade 呼叫須讀當前有效 override（含 dict/set 原地修改、callable 重新賦值）；不能僅启动時序列化一次。純設定可轉 profile；以下 callable 須真正回呼或 P host 執行。native-only profile 不含 Python callbacks；缺必要 host 回能力錯誤。compile/query/llm/search/pipeline 已有候選證據，其他直接核心 facade 缺口見逐列。

| 可覆寫對象 | 分類／優先序與驗證 | 狀態 |
|---|---|---|
| compile.SYSTEM_PROMPT | P→serializable native profile；call-time 值 | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| compile.COMPILE_USER_PROMPT | P→profile；保 `{title,content,existing,article_format}` template | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| compile.COMPILE_ARTICLE_FORMAT | P→profile；languages 只管結構，不能代替 prompt | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| compile.DEFAULT_SECTION_HEADERS | P identity anchor；不可 reassign，此 identity 是 patch detection | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| compile.SECTION_HEADERS | P assignment 優先於 config languages，再 builtin；`[("文言","")]` 為裸正文 | default core與live prompt/callback已遷；v9候選已驗 [E-query] |
| config.BUILTIN_LANGUAGES / get_language_profile / get_section_headers | N resolver+P runtime；三語 en/zh/ja、view zh-en、default_lang zh-en、api_default_lang zh | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| config languages.default/profiles | N 可序列化；sections/views/codes/defaults/single_section/bare_body，invalid 欄 ValueError | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| taxonomy.TAXONOMY_SYSTEM_PROMPT | P→profile；模型語境 | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| taxonomy.TAXONOMY_LABEL_KEYS | P→profile；下游 labels key 不一定 en/zh/ja | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| taxonomy.TAXONOMY_GENERATOR | P callable `(articles,cfg)->tree`；core 管存取／events | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| lint.checks.ALLOW_CJK_SLUGS | P→profile；不得硬寫 pinyin 規則 | B11 lint 路徑已驗逐次讀取並傳 Rust；兩種設定與凍結舊版結果一致 |
| lint.checks.SYSTEM_PROMPT | P→profile | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| lint.fixes.STUB_SYSTEM_PROMPT | P→profile | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| lint.checks.check_stubs、lint.fixes.check_stubs、lint.check_stubs | P callable；siwen 三處 patch 均須 honor，不能重 import 固定函式覆蓋 | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| search.SEARCH_TOKENIZER | P callable(text)->list[str]；core scoring 仍原生 | native TF-IDF＋callable tokenizer；v9過，public新case待v10 [E-python] |
| search.STOPWORDS | P mutable set→profile at call；不要 cache 舊 set | native TF-IDF＋callable tokenizer；v9過，public新case待v10 [E-python] |
| search.CJK_STOPWORDS | P mutable set→profile at call | native TF-IDF＋callable tokenizer；v9過，public新case待v10 [E-python] |
| query.SYSTEM_PROMPT | P→profile | default orchestration已遷、live callback保留；v9已驗 [E-query] |
| query.TONE_INSTRUCTIONS | P mutable dict→profile；query 任意 tone，CLI 仍固定 choices | default orchestration已遷、live callback保留；v9已驗 [E-query] |
| query.PROMOTE_SYSTEM_PROMPT | P→profile | default orchestration已遷、live callback保留；v9已驗 [E-query] |
| query.PROMOTE_CONTENT_EXAMPLE | P→profile；None 依 sections 派生 | default orchestration已遷、live callback保留；v9已驗 [E-query] |
| query.PROMOTE_TITLE_EXAMPLE | P→profile；None 依 sections 派生 | default orchestration已遷、live callback保留；v9已驗 [E-query] |
| query.prefilter_threshold/top_k config | N；default 500/200，guard invalid config；不是模型向量召回 | default orchestration已遷、live callback保留；v9已驗 [E-query] |
| xici.XICI_SYSTEM_PROMPT | P→profile | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| xici.LANG_STYLES | P mutable dict；自定 code 需 matching style | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| xici.chat | P callable；siwen 實際 replace，須呼叫當前 module binding | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| entities.ENTITY_SYSTEM_PROMPT | P→profile | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| entities.ENTITY_PROMPT | P→profile template | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| entities.ENTITY_ARTICLE_FORMATTER | P callable(articles)->list[str] | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| entities.extract_entities | P override 真執行；core CRUD/job 保持 N | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| llm.chat / llm.chat_with_meta | P wrapper 保存 original 再包 `<think>` stripping；避免 facade 遞迴，自定 callback 仍可下調原生 adapter | 真native HTTP adapter＋P wrapper；v9候選已驗 [E-model] |
| normalize.SENTENCE_TERMINATORS | P→profile，CJK+ASCII 出廠值 | B12 已驗逐次讀取、實際 Rust 運算、替換生效 |
| normalize.CLOSING_WRAPPERS | P→profile，保標點合段規則 | B12 已驗逐次讀取、Unicode wrapper、替換生效 |
| web.EXTRA_ROUTES | P list `(rule,handler,options?)`，factory 前註冊，handler 顯式需要 core gateway | P Flask live hooks保留；Siwen89/bake已驗；最新auth待v10 [E-python] |
| web.BEFORE_REQUEST_HOOKS | P list callbacks，返回可 short-circuit | P Flask live hooks保留；Siwen89/bake已驗；最新auth待v10 [E-python] |
| web.AFTER_REQUEST_HOOKS | P list response callbacks | P Flask live hooks保留；Siwen89/bake已驗；最新auth待v10 [E-python] |
| app.config['llmbase'] | P live dict含 base_dir/cfg/api_secret/session_token；勿影印不可變 snapshot | P Flask live hooks保留；Siwen89/bake已驗；最新auth待v10 [E-python] |
| app.view_functions / app.url_map / app.test_client | P Flask 本體；siwen 替換 api_compile/api_branding/serve_favicon/serve_spa/api_aliases | P Flask live hooks保留；Siwen89/bake已驗；最新auth待v10 [E-python] |
| web.static_dir config | P static serving；resolve under base，不能任意宿主檔案 | P Flask live hooks保留；Siwen89/bake已驗；最新auth待v10 [E-python] |
| worker.LEARN_SOURCES/register_learn_source | P mutable dict；call `handler(batch_size=N,base_dir=Path)`；both 分配既有 cbeta/wikisource | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| worker.CUSTOM_JOBS/register_job | P list `{id,interval_hours,handler}`；handler(base)，<=0/invalid interval skip；排程與狀態 N | 仍缺直接facade原生委派；live installed bindings已核 [E-live] |
| operations.register(Operation(...)) | P registry capability；runtime 增加/替換，MCP list 與 Agent ops list 反映 | 部分built-in core已遷；動態P保留；其他helper仍缺 [E-python] |
| wikisource.READING_LISTS / fetch_subpages / _ingest_page | P connector；siwen 直接讀寫，不能藏掉私有入口 | P舊hook/connector保留；對應原生emitter與每hook trace仍待驗 [E-live] |
| refs PLUGIN_ID/PLUGIN_NAME/get_source_url/search_references? | P ref module protocol；來源 URL resolution與fallback仍可用 | native TF-IDF＋callable tokenizer；v9過，public新case待v10 [E-python] |

`SECTION_HEADERS is not DEFAULT_SECTION_HEADERS` 才視 patch（[config.py](../../llmwiki/config.py#L98)）。原地改同 identity 的 list 與 assignment 不同，須以測試明立，勿自行把二者合一。`_patched` profile 仍 default_lang/api_default_lang='zh'，即使其 `codes` 是 `['文言']`；Web lang任意值放行；worker intro 仍循 zh/en/ja/zh-en。顯式 config profile 才檢查 codes、worker 按 profile codes 生成。裸正文 split/assemble/merge/export 要沿同一有效 profile；export 節名預設 english/zh/ja，自定 文言 key 原樣。参見 [test_languages](../../tests/test_languages.py)、[test_web_languages](../../tests/test_web_languages.py)、[siwen-migration](../siwen-migration.md)。

### Hooks 逐事件

全為 P callback／N emitter，逐事件原生trace gate尚未全覆；可依 [hooks.py](../../llmwiki/hooks.py) 保 best-effort。callback 清冊不是發送外部通知的授權；測試使用隔離 spy callbacks。

| event | kwargs | 核心 emitter | 狀態 |
|---|---|---|---|
| ingested | source,title,path,url? | ingest | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| before_compile | batch_size,titles（預覽最多 5） | compile | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| compiled | source,work_id,raw_type,title,metadata | compile | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| after_compile_batch | count,articles（最多 10 path） | compile | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| index_rebuilt | article_count | compile/rebuild | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| taxonomy_generated | category_count,article_count,generated | taxonomy | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| after_lint_check | total_issues,results | lint/checks | B11 已驗 Rust emit stage 觸發一次 host hook，順序／kwargs 正確，check 失敗不發成功事件 |
| after_auto_fix | fix_count,fixes | lint/fixes | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| xici_generated | lang,article_count | xici | P callback保留；逐事件native emitter與spy證據待補 [E-live] |
| entity_extracted | people_count,events_count,places_count,article_count | entities | P callback保留；逐事件native emitter與spy證據待補 [E-live] |

## 檔案、metadata、索引與恢復契約

以下皆 **N**，callbacks／外部 parser 例外已明列。新 SQLite 不能把人作内容、分類、手工關聯、trails、jobs/audit/revisions 全稱 cache。已驗原生 slice 如狀態欄；完整舊→新→export/restore→rollback 仍須在最新版 copy-first fixture 終驗。

| 舊位置／格式 | 權威性與須守細節 | 來源／測試 | 狀態 |
|---|---|---|---|
| `<base>/config.yaml` | 5 configured paths可與預設不同；本機真文庫不作測試；新 profile migration 保 unknown config | test_native_config／test_native_config_roots；unknown rich YAML／custom-root fixtures | B16 shared native bytes reader、facade 順序／路徑已驗；Python YAML parser 保留型別及 alias identity [B16](checkpoints/native-config-2026-09-27/README.md) |
| `<raw>/<slug>/index.md` | source 原典+frontmatter，source/title/type/ingested_at/compiled 等；不得編纂改原文 bytes；舊 compiled flag 應轉狀態／sidecar | [ingest](../../llmwiki/ingest.py#L51)、[compile](../../llmwiki/compile.py#L174)；hash gate 待補 | native source/original asset已驗；全connector source metadata待核 [E-ingest] |
| `<raw>/<slug>/*.{md,markdown,txt,pdf,py,json,csv}` | 原檔與 companion metadata；ingest_file 舊已重寫 markdown frontmatter，native 要保原 bytes 及獨立 metadata | ingest_file/ingest_directory；all suffix fixture | native source/original asset已驗；全connector source metadata待核 [E-ingest] |
| raw 子目錄 images／PDF chunks | binary 不解碼重寫；來源關係、頁碼、parser version/hash；plugin輸出另層 | pdf/browser/ingest；connector fixtures 待補 | native source/original asset已驗；全connector source metadata待核 [E-ingest] |
| `<concepts>/*.md` | 人工／已接受生成內容皆權威；unknown nested frontmatter round-trip；source links/aliases/tags/status/revision 保留 | compile/export/lint；unknown metadata fixture | native journal及compat compile/query已驗；完整legacy混用仍部分 [E-query] |
| `<outputs>/*.md` | 保存問答、markdown/marp/chart 內容是權威，不因生成而丟；secrets 不寫入 | [query._file_output](../../llmwiki/query.py#L597)；query promote tests | native journal及compat compile/query已驗；完整legacy混用仍部分 [E-query] |
| `<meta>/index.json` | list[{slug,title,summary,tags,sources}]；可重建但不能丟 content | [rebuild_index](../../llmwiki/compile.py#L337)；web_articles | native projections及journal已驗；公開範圍差異明列 [E-maint] |
| `<meta>/_index.md` | Obsidian 可讀的 grouping index；可重建，wikilinks 與 titles | rebuild_index；format fixture 待補 | native projections及journal已驗；公開範圍差異明列 [E-maint] |
| `<meta>/aliases.json` | 衍生 alias map可重建；文章 aliases／人定 alias overrides 為權威需保存；只在 library 內解 | [resolve.py](../../llmwiki/resolve.py)；test_resolve | native projections及journal已驗；公開範圍差異明列 [E-maint] |
| `<meta>/backlinks.json` | slug→list[referrer]，alias-aware wikilinks projection；刪源不殘留；手動 typed relations 另權威 | [compile._build_backlinks](../../llmwiki/compile.py#L738)；graph gate | native projections及journal已驗；公開範圍差異明列 [E-maint] |
| `<meta>/taxonomy.json` | categories/tree/labels，locked true 是人工覆蓋，不能重建丟掉 | [taxonomy](../../llmwiki/taxonomy.py)、web taxonomy update；manual fixture | native artifact/history/private已驗；Python直接生成/保存待遷 [E-maint] |
| `<meta>/compiled_sources.json` | sorted source-key log，source URL或title 去重；不是 content hash，需 migration receipt保史／新 revision 驅動 | compile_new；重編 bug／source change 待補 | native journal及compat compile/query已驗；完整legacy混用仍部分 [E-query] |
| `<meta>/health.json` | checked_at/issues_before/issues_after/results/fixes_applied；操作 report保備份政策 | cli lint heal/worker；health restore fixture | native report已驗；latest receipt／restart批次待驗 [E-repair] |
| `<meta>/last_fix.json` | `{fixes,fix_count,status:'completed'}`；舊 Web非持久 thread結果；新 job兼容投影 | web lint/fix；failure/restart gate | core journal已驗exact receipt/private分檔／failure；repair13過 [E-repair] |
| `<meta>/entities.json` | 抽取 entities與來源 metadata；人定 amendments不能在重抽時無聲覆蓋 | entities._save_entities；test_entities | native artifact/history/private已驗；Python直接生成/保存待遷 [E-maint] |
| `<meta>/xici-<lang>.json` | 已接受導讀內容須保存；profile language與model prompt區分；檔名用 hyphen | [xici._save_xici](../../llmwiki/xici.py#L217)；read/write restore fixture | native artifact/history/private已驗；Python直接生成/保存待遷 [E-maint] |
| `<meta>/trails.json` | private research trails，id/created/updated/steps與server ts權威；不得公開 bake | web._load_trails/_save_trails；privacy gate | native持久＋private已驗；真empty publictrail分支新test過 [E-write] |
| corpus progress 與 sync 狀態 | CBETA/Wikisource ingest/compiled進度不可丟，connector仍可用；不得真同步生产 | cbeta/wikisource/sync；isolated receipt fixture | P原格式保留；native排程/進度接線仍缺 [E-live] |
| `<base>/<subdir=.chunk_cache>/<sha256(cid)>/<sha256(content_hash)>.md` | 舊 cache全 64 hex，content_hash字串又 hash；已持久成功結果重用，prompt/model revision用 versioned subdir | [chunk_cache.py](../../llmwiki/chunk_cache.py)；test_chunk_cache | native bridge已驗；v9候選／舊byte formats [E-python] |
| `<base>/.pipeline/<stage>/<sha256(key)>.jsonl` | append-only log為舊管道權威；ts/event，stage safe regex；reserved events start/ok/failed/partial/interrupted/artifact/meta_update | [pipeline/log.py](../../llmwiki/pipeline/log.py)、state；test_pipeline | native bridge已驗；v9候選／舊byte formats [E-python] |
| pipeline `.lock`／stale recovery | 舊 hostname+PID，不核 PID birth；TTL 僅資訊不強拆live；SIGKILL next acquire emits interrupted；native 須共同排他及 PID reuse guard | [pipeline/lock.py](../../llmwiki/pipeline/lock.py)；雙語兩writer測試 | native bridge已驗；v9候選／舊byte formats [E-python] |
| JSONL torn尾／invalid UTF-8 | iter_events 逐行跳 malformed；下一 append 先補 newline保已有好行；不會把壞尾與新event粘合 | pipeline/log.py；test_pipeline | native bridge已驗；v9候選／舊byte formats [E-python] |
| SQLite source revisions/jobs/audit/tombstones/migration receipts | 新權威 DB-only資訊有 backup/export/restore，不可只 export wiki文章即宣稱備份完成 | 新 storage/backup tests 待補 | native storage/migration工程已驗；latest Linux全鏈待驗 [E-core] |
| FTS/chunks/embeddings/graph projection | 依 content hash/parser/chunker/provider/model/revision/dimension fingerprint重建；世代原子切換，不混維度 | 新 index-generation tests 待補 | native storage/migration工程已驗；latest Linux全鏈待驗 [E-core] |
| static/dist 或 config web.static_dir | public build/read API contracts；自定 root需 containment；private/internal不可混 public export | web factory + siwen bake + frontend | typecheck/build與bake已驗；native static3tests過，實際browserflow仍待验 [E-read] |

舊 atomic JSON helper 只原子單檔，不保 file+DB／多檔共同交易；新 staged write/journal/outbox 須有故障注入測試。舊 worker `job_lock` 是 `threading.Lock`，operations、Web、worker 也未全都使用；不能假作跨 Python/Rust writer 排他。歷史 pipeline per-stage 鎖亦不能直接保整个 library 全部寫入。

## 新能力逐項驗收（不得用相容清冊縮小原目標）

所有下列均 Rust 原生必達；已有的工程／相容測試與未閉合項分列，不用部分成功替代原規格第七／九節的完整 gate。

| ID | 能力與證明要求 | 必需證據 | 狀態 |
|---|---|---|---|
| N01 | 無 Python interpreter/package 的 release binary 完整原生鏈路 | 隔離環境 ingest→draft/compile→index→search→ask→citation→update/delete→export/restore，三端實用 | 已驗目前 B8 release：Linux chroot CLI31步、實際 HTTP/MCP、held-gate SSE、完整 JSON 回應及清潔退出；primary 獨立重驗與逾時反例過，944acae [三端證據](checkpoints/native-transports-2026-09-26/README.md) |
| N02 | shared application service | CLI/HTTP/MCP/Python相同核心 DTO/errors/library/write policy；非四套 business logic | 部分：Application 共用已驗；直接 Python 通用 helper 仍待遷 [E-live] |
| N03 | SQLite local store + lexical index | 無外 DB、無模型憑證可 ingest/index/title/alias/chapter/exact-quote search/get/evidence | 已驗 native lexical／storage／evidence slice；最新全 workspace 待重驗 [E-core] |
| N04 | 真 HTTP embedding adapter + vector retrieval | offline fixture server觀察實際 request；vector數學/cache/isolation/融合測試；禁lexical冒充hybrid | 已驗真 HTTP fixture/cache/vector；真語義品質未驗 [E-model] |
| N05 | hybrid fusion + 顯式降級 | lexical/vector各候選與score來源、RRF、無embedding降級旗標；raw/compiled/both與token budget | 已驗 RRF／explicit downgrade／raw-compiled scopes [E-model] |
| N06 | CJK/多語/罕字/異體/繁簡 | 固定中英日／文言裸正文fixture、同名／篇卷名／精確句；原文bytes不變 | 部分：CJK/罕字/繁簡與固定 lexical 已驗；anchor 已有原生 facade，B12 文言段落及B14標題規則已驗，終驗須覆當前整體候選 |
| N07 | 證據 schema與版本解析 | library_id/source_id/revision或hash/passage_id/span/exact_quote/source_url/定位；normalized→original offset | 已驗版本／span／quote解析；正文推論支持另有審核 [E-ground] |
| N08 | 分庫全域邊界 | source/chunk/article/session/cache/job/edge/export皆library；同名alias不跨庫；外庫origin明標 | 部分：native library/privacy guards 已驗；facade 全路徑 revocation 待新候選 [E-core] |
| N09 | authentication/authorization | 未授權跨庫read/write/model denied；visitor cookie無能力；SSRF/traversal/secret/protocol injection | 部分：native HTTP/SSE/fetch scope 已驗；新版 Flask 候選 auth/privacy 待驗 [E-python] |
| N10 | byte原典與版本／內容權威 | 原稿bytes hash、不覆寫OCR/標點/摘要、unknown frontmatter、人工內容/關聯保留 | 部分：Markdown／binary source assets 原件已驗；全 connector companion 元資料仍待核 [E-ingest] |
| N11 | 冪等增量 | 相同fingerprint不重片；單文變更僅受影響索引；已持久embedding重用；provider crash window成文 | 已驗 source增量／embeddingcache；模型 crash 窗口已明列 [E-core] |
| N12 | update/delete/rename/修訂 | source刪後無可搜片，shared relation其他支持保留；舊引用解析舊revision或stale | 已驗 journal update/delete/rename及 historical evidence／shared support [E-catalog] |
| N13 | index世代原子切換 | model/revision/dimension改變新generation；讀者不見半成品；切換failure rollback | 已驗 vector generation CAS/activation；最新 integration 待重驗 [E-model] |
| N14 | crash／並發／重啟恢復 | staged filesystem+DB journal，兩進程Python/Rust共鎖、stale/PID reuse、torn JSONL、reader during cutover | 部分：journal恢復／跨進程writer／tornJSONL 已驗；完整SIGKILL跨投影窗口仍待證 [E-core] |
| N15 | durable jobs | ID/queued/running/succeeded/failed/partial/cancelled/interrupted、lease/retry/backoff；舊status投影 | 已驗 jobs／exact-id worker7；尚欠旧worker direct facade native delegation [E-jobs] |
| N16 | 續跑／舊管道 | run_stage JSONL与ChunkCache相容，partial cache resume；不可丟歷史狀態 | 已驗 native pipeline／ChunkCache＋v9 installed candidate；v10待驗 [E-python] |
| N17 | 統一模型層 | text/embedding/timeouts/cancel/rate retry/usage/finish_reason/truncation；可配置HTTP模型路徑 | 已驗 HTTP model/embedding/retry/cancel/truncation工程；付費真provider未驗 [E-model] |
| N18 | 真生成串流 | provider delta未完前已發HTTP SSE；progress/evidence/usage/done/error帶request_id/sequence/version | 已驗目前 release HTTP precompletion delta、evidence/usage/done、request/version/sequence；逾時後才送達之反例被拒，944acae；取消/故障另見 [E-http-model] |
| N19 | 串流故障／取消 | provider SSE碎包、斷線、timeout、中途error/cancel；heartbeat非首字；nonstream capability明示 | 已驗 disconnect/cancel/timeout/error；SSE6過 [E-http-model] |
| N20 | 據典問答與寫入政策 | retrieve→evidence→answer；candidates/consulted/used evidence不同；native ask不默存 | 已驗 read-only native ask＋evidence；grounding7與人工審核已封存 [E-ground] |
| N21 | compile/promote草稿／原子apply | metadata/dedup/merge/language/taxonomy/hooks；empty/格式错/truncated/retry耗盡/partial不寫成功 | 部分：draft/apply及accepted compat compile/promote已驗；全Python taxonomy/lint/hooks委派待補 [E-query] |
| N22 | 既有圖能力 | wikilinks/backlinks/alias/evidenced typed relations及bounded邻接；矛盾版本不覆蓋 | 已驗 native graph/backlinks/alias/typedrelations5；Python direct resolve待遷 [E-catalog] |
| N23 | 插件能力邊界 | native profile／serializable config／Python callback／external process分清；scope檢查；缺host明錯 | 部分：process parser＋caller scope已驗；全部舊connector CLI wrappers仍缺 [E-ingest] |
| N24 | Python動態相容 | 候選wheel實際module來源與native traced；每次override現值／callable真回呼 | 部分：v9 552 tests與real traces已驗；直接helpers/maintenance仍繞回Python [E-live] |
| N25 | Flask/siwen相容 | view_functions/url_map/config/test_client/middleware/private helpers；bake公開內容過關 | 部分：installed Siwen89＋bake133JSON/112HTML；最新版native全路径仍待驗 [E-python] |
| N26 | 現前端契約 | typecheck/build與閱讀/ask/languages流程；不改全新前端 | 部分：原前端9errors重現並fix；read HTTP3過、model HTTP6過；staticbrowser整流待驗 [E-read] |
| N27 | 設備預備 | session/library/short answer/structured citations/cancel/progress；圖像/筆跡→轉錄獨立邊界及合成transcript fixture | 部分：session/shortanswer/cancel存在；合成transcript入口root/model agent在作 [E-ground] |
| N28 | copy-first migration | dry-run讀性/unknown metadata/ID碰撞/檔數/bytehash/容量；新目標；失敗續跑receipt | 最新release實跑已驗：自定root、unknown metadata、bytehash、asset及模擬未完成receipt之衝突拒絕／續跑，a27595c；不是SIGKILL實驗 [遷移證據](checkpoints/copy-first-legacy-reader-2026-09-26/README.md) |
| N29 | verify/rebuild/backup/restore/rollback | 副本全鏈演練；rollback後舊版實際能讀；若新格式不兼容保留舊副本与切换，不偽稱原地回退 | 最新copy/backup/restore/rollback與真舊版回讀已驗，a27595c：34步、原庫10檔不變、完整job/刪源/Evidence/profile保留、篡改拒絕；此reader使用Python，無Python核心另見B9，索引重建另見 [E-core] |
| N30 | 固定 held-out lexical品質 | 同fixture舊/新真scorer，必要案例全過、Recall@k/MRR或nDCG不低於舊基線；不得改題調baseline | 已驗 B8同一release，raw/compiled各30heldout+6tuning，Recall@5/MRR@10均1.0，60unique引文可解析，無退步；原始評測存944acae [E-retrieval] |
| N31 | hybrid工程／真語義分列 | mock只驗工程；真embedding品質ablation、延遲與成本獨立報；未跑明標未驗 | 工程已驗；真semantic ablation未驗，明示不作完成證據 [E-model] |
| N32 | 引文解析率／推論支持分列 | 指定revision exact quote與span程式驗；答案推論由明定cases獨立審核，不能等同字串命中 | 已驗 deterministic citation解析289/289＋grounding專項；非真模型泛化證據 [E-ground] |
| N33 | 效能與成本測量 | cold/warm p50/p95/RSS/index大小/增量成本/model calls，模型與本地retrieval latency分開 | 已驗目前release：5搜尋重複/20效能重複，startup/fresh-index/warm/增刪改/child RSS/index bytes/model calls分項存944acae；cold僅fresh index，未清OS cache，真provider latency未驗 [E-retrieval] |
| N34 | checks/CI | fmt/clippy/workspace tests/release build/Python候選相容/所改frontend；原失敗須獨立重現 | 部分：d937550目前Rust fmt/clippy/workspace --no-fail-fast/release全過；575 Python為較早B5.1候選，新wheel及最終frontend整合仍待驗 [E-core] |
| N35 | 交付與運行證據 | architecture/authority docs、runbook、parity、eval report、limits、commands/exit/logs、background status；無提交發布生产切換 | 部分：docs/runbooks/logs在；授權release待終驗/tag/PyPI/ClawHub [E-core] |

可選 query expansion／reranker 若加入，另對無功能 baseline 報品質、延遲、成本；未選用不用建平台。真模型 smoke 僅已有明確授權與憑證及可公開fixture時最小請求；此項缺測可明標「未驗」，不能算已過語義評分。

## 明確非目標與已知舊缺陷

| 項目 | 分類／理由 | 狀態 |
|---|---|---|
| 生產切換與真文庫原地升級 | X；使用者尚未授權此等操作 | excluded |
| GitHub Release／PyPI／ClawHub、commit/push/tag、版本更新 | 後續使用者已授權；須先完成遷移 gates 與倉庫發布規範 | pending publication |
| 全量重抓／真文庫改動、未授權主機傳機密 | X；使用fixture与副本，不能以缺真樣本停獨立工作 | excluded |
| riddle driver、硬體燒入、真OCR、人設／字體／消墨動畫 | X；只本輪設備問答契約與合成轉錄是必達 | excluded |
| 全新frontend、訓練模型、Neo4j/distributed job cluster | X；保 React/TS與modular monolith | excluded |
| 完整temporal GraphRAG/dream、大規模自動抽圖/社群摘要 | X；既有圖＋證據typed relation仍必達 | excluded |
| Rust dynamic ABI/hot-plug/WASM sandbox | X；首版明確插件接口足夠 | excluded |

舊缺陷不得當作新完成證據：compile 空模型回覆仍可能標 compiled；compile_all 受 compiled_sources 阻擋；常規來源preview／Agent get_article等缺統一路徑邊界；ingest image fetch未全面沿URL驗證；多寫入路徑未鎖；CLI mcp banner污染stdout；Web lint/fix可回started卻未取得鎖。這些應各寫新失敗／安全測試與migration note。上述僅是源碼審計所見，尚未執行漏洞重現，勿把推斷說成已驗攻擊。

完成更新規則：每個 N/P row 附 native 實作連結、具體 assertion／命令／exit code、原始結果及必要下游證據後才可 done。仅 schema、空實作、mock runtime、import smoke、route 200、某一窄測試綠燈均不足。進度保持於 [plan.md](plan.md)／[status.md](status.md)，但它們不代替本表逐項 gate。
