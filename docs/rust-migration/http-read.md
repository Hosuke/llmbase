# Native HTTP 讀取契約

實作：[`http.rs`](../../crates/llmbase-cli/src/http.rs) 僅作路由、授權入口、DTO
與 HTTP cache header；[`web_read.rs`](../../crates/llmbase-core/src/web_read.rs)
持有 legacy Web read 邏輯，透過共同 Application scope/capabilities 執行。
未改 React UI，也未把資料庫或 Markdown 處理另寫一套到 HTTP handler。

## 路由與映射

| Web 路由 | 共同 Application operation | DTO／狀態 |
|---|---|---|
| `GET /api/branding` | `web_config` 的 branding field | 舊 name/nameShort/tagline/poweredBy；只讀白名單 |
| `GET /api/languages` | `web_config` 的 languages field | sections/views/codes/defaults/single_section/bare_body |
| `GET /api/stats` | `web_stats` | raw_count/article_count/output_count/total_words/link_count/health_score |
| `GET /api/collections` | `web_collections` | 真 tag groups＋config labels |
| `GET /api/articles` | `web_articles` | 無 query 舊 `{articles}`；limit/cursor/tag/q/fields mode 含 total/count/next_cursor/filters |
| `GET /api/articles/lite` | `web_articles {lite:true}` | 真 index.json membership，current visible title；缺檔真空列表 |
| `GET /api/articles/<slug>` | `web_article` | status、完整正文、sanitized source URLs、真 backlinks |
| `GET /api/articles/<slug>/sections` | `web_article {sections:true}` | 共用 native compat section parser 的 scalar offsets／stable anchors |
| `GET /api/aliases` | `web_aliases` | 包含有效人工 overrides 的 scoped native aliases |
| `GET /api/taxonomy` | `web_taxonomy` → catalog | 驗 lang 後依 profile default 投影 |
| `GET /api/entities` | `web_entities` → catalog | 真 cache／明示 not_generated；public provenance filtering |
| `GET /api/xici` | `web_xici` → maintenance | 真 hyphen filename cache；profile lang validation |
| `GET /api/search` | `web_search` → native search | query/results；補 metadata summary 與實際 normalized matched_terms |
| `GET /api/sources` | `web_sources` | directory groups，index.md 優先，relative path＋metadata |
| `GET /api/sources/<slug>` | `web_source` | raw index.md、metadata strings、configured scalar character cap |
| `GET /api/export/article/<slug>` | `web_export_article` | 共用 maintenance language-profile export |
| `GET /api/export/tag/<tag>` | `web_export_tag` | 共用 maintenance tag export |
| `GET /api/export/graph/<slug>` | `web_export_graph` | 共用有界 actual-link export |
| `GET /api/wiki/export` | `web_wiki_export` | visible article slug → metadata/content，真 count |
| `GET /api/health` | `web_health` | persisted report 或 null；另 `withheld` 明示 private cache 隱去 |
| `GET /api/trails` | `web_trails` | 真 accepted trails.json；須 Private capability |
| `GET /api/worker/status` | `web_worker_status` | 真 native persistent running jobs；須 Private capability |
| `GET /api/tones` | `web_tones` | 五個舊 tone metadata，實際 query 已支援 |
| `GET /api/refs/plugins` | `web_refs_plugins` | 三個舊 builtin reference descriptors，獨立於 process adapters |
| `POST /api/lint`，deep=false | `web_lint` → maintenance | results 八類 deterministic checks |

`/api/healthz` 仍是無檔案／model I/O 的即時 liveness。既有
`/api/v1/op`、evidence、ask、SSE/cancel 路由及 protocol lifecycle 不被替換。
`POST /api/index/rebuild` 既有授權路徑仍委 native maintenance。

Web 與 Agent factory 不混用 envelope：Agent articles/search/sources/stats/health
保留 `status:ok` 與 Agent stats `articles/raw_documents/filed_outputs`；Web
stats 使用前端原需欄位。Web-only routes 只註冊於 Web surface。

## Defaults 與安全界線

舊 Web language routes 在無配置時仍為 trilingual：en/zh/ja 加 zh-en view，
display default zh-en、API default zh。顯式 `languages` 可設英文、單段裸文言或
自定多語；view codes、show、primary、default 均驗證。這不改新 native ask
的英文／read-only 預設。Python import-time SECTION_HEADERS monkey patch 不會
跨進 standalone Rust process；須傳既有有效 profile，動態 host parity 另驗。
`web_config` 不回 config.yaml 原文、paths、model settings 或 secrets。

所有 content read ops 先在 library writer lock 下核 current indexed visible
source 的 actual file SHA256。外部檔案刪改（尤其 public→private）未 sync 時，
回 Conflict；若新檔已屬 private，scope guard 先回 NotFound。兩者均先於 response/304，
不公開舊 snapshot。HTTP read 不暗中做全庫
sync；管理者明確 index 後恢復讀取。單 snapshot ≤20000 sources、64 MiB；
metadata cache ≤4 MiB。HTTP/auth 在 Application 開庫前校驗。

未帶 bearer 的 public read 只授配置中的 default library；cookie 不授 Private／
Write／Model。跨 library header、無效 bearer、未授 private read 均拒絕。
health aggregate 沒有 evidence；若庫曾有 private revision，public report 隱去，
stats health_score=0 並附 `health_status:private_cache_withheld`，不冒充已通過
health check。Private bearer 可讀真 report。其他 caches 使用 catalog／maintenance
既有 provenance 策略。詳細 raw metadata 的列印保留舊文字 DTO，並非 config dump。

ETag 使用「URL slice＋完整可見 response JSON」SHA256；articles、lite、taxonomy
支援 If-None-Match 列表、weak/strong equivalence 與 `*`。先授權、驗 query、取真
資料再判 304。固定 `Cache-Control: private, no-cache` 與
`Vary: Authorization, X-Library-ID`；不把 private bearer response 放入 public cache。

## 明確差異與尚欠

- Native HTTP content 讀取須已有索引；外部直接改文後需 sync。舊 Flask 多讀
  live filesystem，可在尚未 rebuild 時直接見新文。原 Python compat readonly
  API 不受此 indexed Web policy 取代。
- Config/profile 是 Application 載入時的有效設定；目前需重建 Application／
  重啟服務才載入 config.yaml 外部改動，舊 Flask 每 route 重讀 config。
- Native ETag 非舊 mtime-based 值，未提供 Last-Modified；未採
  LLMBASE_LITE_CACHE_MAX_AGE 的 public caching opt-in。
- Lite 只列 cache 中仍可見 live articles，title 用 current revision；不回已失效
  title／private metadata。Native backlinks 是 live indexed derived links，非
  舊 stale cache。原 fuzzy first-match aliases 改為歧義拒絕。
- Native search 是 passage/evidence 排名，其 query normalization 與 matched_terms
  不冒稱逐分數相同的舊 ranker；同來源可有多 passage。Agent legacy search 機制
  未因 Web DTO enrich 改寫。
- Graph depth 上限 4，舊 Web clamp 至 5；超限明錯。Raw 缺檔為 HTTP404，舊路由
  有 status:error/HTTP200。Nested raw metadata strings 以 JSON 表示複合值，非
  Python repr；flat bool/None 保持 True/False/None。
- `web_worker_status` 查 native durable running jobs，並附 scope，非舊 Python
  `threading.Lock`。不宣稱觀測其他 Python process 的 job_lock。
- tones 與三個 builtin refs metadata 已接 native read operation；動態 Python
  plugin discovery／callable tone mutation 仍是 host 契約。
- Siwen 自定 `/api/classics`、kepan、masters、lineages、schools、persons、books、
  outputs／Flask mutable endpoint hooks 由 downstream facade/plugin parity 另辦。
- Deterministic clean/delete、taxonomy update、trails mutation 已於後續
  [`HTTP write slice`](http-write.md) 實作。Legacy ask、compile、deep lint、full
  repair、entities／Xici 已接線並有 6 組 model fixtures，見
  [HTTP model slice](http-model.md)。Ingest／upload 已接線，5 組新測試待首驗，
  見 [HTTP ingest slice](http-ingest.md)；各 slice 之具體差異仍須分別驗收。

## 驗證

`tests/web_read.rs` 3 案例最初於 `.rust-migration-runs/web-read-core.log` 全過；
其後包含 Web search/lint delegate、health withheld marker 與 api-default fallback
的最新 combined-v7 再驗亦全過，maintenance 9 同過。

[`tests/http_read_contracts.rs`](../../crates/llmbase-cli/tests/http_read_contracts.rs)
3 案例已於 `.rust-migration-runs/combined-v7.log` 全過：前端 read sequence
真資料／兩 factory DTO、pagination＋conditional cache＋scope、Private routes＋
外部私密改文。同輪既有 SSE 6、transports 6、compile transports 3 亦過。
先前 React baseline typecheck/build 已過，見
[`frontend-baseline.md`](frontend-baseline.md)。本輪不修改 UI，也未啟 production server。
