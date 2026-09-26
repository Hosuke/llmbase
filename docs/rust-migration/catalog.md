# Native catalog：分類、連結、關係與公開投影

實作位於 [`catalog.rs`](../../crates/llmbase-core/src/catalog.rs)，由同一
`Library`／SQLite library binding 提供；無 Python subprocess 或固定成功回應。
人定分類與人工關係是持久內容，圖與反向連結是由可見的既存來源重算的投影。

## API 與權限邊界

`Application` 已接下列 native operation；HTTP `/api/v1/op` 與 CLI `op`
循同一授權層。`include_private` 由可信 Principal capability 推導，絕非
任意 request-body 布林值。未列在舊 MCP schema 的 native operations 不因此
自動視為舊 tool 已相容。

| Operation | Args | 真正執行的核心 API |
| --- | --- | --- |
| `aliases` | `{}` | `Library::aliases(include_private)`：索引來源名稱、分語標題、aliases、merged_from |
| `backlinks` | `{target}` | `Library::backlinks(target, include_private)`：wiki-link 解析與反向查找 |
| `graph` | `GraphOptions` | `Library::graph(options, include_private)`：有界原生關係投影 |
| `taxonomy` | `{lang?}`，Application 預設 `en` | `Library::taxonomy(lang, include_private)`：人定／舊快取／tag fallback 階層 |
| `taxonomy_set` | `{document}` | `Library::set_taxonomy(document, include_private)`：交易式保存完整 accepted JSON |
| `entities` | `{}` | `Library::entities(include_private)`：真實舊 entity cache 讀取及可見性投影 |
| `relation_add` | `RelationInput` | `Library::add_relation(input, include_private)`：人工關係及證据版本保存 |
| `relation_remove` | `{relation_id}` | `Library::remove_relation(id, include_private)`：有歷史的 tombstone |

讀取需 Read，變更需 Write；跨庫在 Application 開庫前拒絕。各 SQL
lookup、alias、來源／證据解引用亦限 `library_id`。public projection 不列
private/internal 的節點、名稱、分類、entity aggregate 或證据。寫入 public
relation 時，其兩端及每一 evidence support 必須皆可公開讀取；有私密
evidence 的人工關係必須明定 private/internal，且呼叫者具 Private。

## Graph 與真實 provenance

`GraphOptions`：

```json
{"root":"source-id-or-alias","depth":1,"max_neighbors":50,"max_nodes":1000,"max_edges":5000}
```

`root` 可省略。深度 0..4，鄰居 1..200，節點 1..5000，邊 1..20000。
回應含 `version:1`、library、root、nodes、edges、`truncated`。超出輸出／
fanout 上限會明示 truncated；不把有界結果稱全圖。投影讀取另限 20000
visible sources、64 MiB visible text 及 64 MiB retained relation payload，
超限回明確 capability error。

邊的種類與 origin 分列：

- `wikilink`：從原文 `[[target|label]]` 實際解析，解析至唯一可見來源；
  evidence 為涵蓋該 wiki-link 的真實 indexed passages。
- `source_reference`：accepted article/note metadata 引用可唯一識別的 raw
  source；支援 source ID、path、URL、title、work ID 的真實比對。
- `shared_source`：兩文章 metadata 指向同一可見 raw source，支持可有多個。
- 任意人工 `kind`：origin 為 `manual`，與衍生邊分開；`supports` 與
  `contradicts` 等相反關係互不覆蓋。

來源引用／共享來源只聲稱「此兩文引用同一來源」，不是模型推論已被證明。
附帶的 source passage 可解回原文；引用存在和論斷受支持仍是兩種驗收。
目前 React 舊 Graph 頁由 shared tags 畫線，沒有 Python `graph.py`；native
投影不把 shared tags 自動聲稱為真實語義關係。

## 人工關係與刪除來源

`RelationInput` 含 `source`、`target`、`kind`、`supports: Evidence[]`、
`metadata`（預設 `{}`）、`visibility`（預設 public）。每一 support 先經
`resolve_evidence` 驗 library、revision、hash、passage ID、原文字節區間及
exact quote；偽造、跨庫證据在寫入前拒絕。

同一有向端點／kind 的再次加入累積支持，未知 metadata 欄位保留；visibility
只能持平或收緊。kind 不同則 ID 不同。metadata 限 16 KiB，support 限 128。
明確無來源的人工 accepted relation 以 `unconditional:true` 保存，不能混稱
為來源支持；有 supports 者只有在仍有非刪除支持時才投影。

證据原貌永存：更新來源後，graph 回應分列 `stale`、`deleted`、
`current_revision`，不把舊 quote 靜默改指新版。刪除一份 shared support，
另一份有效支持仍使關係存在。全部支持刪除後，accepted relation row 仍在，
僅停止列入 active projection。刪除人工關係寫 tombstone 與新 revision。

SQLite tables：`catalog_relations`、`catalog_relation_revisions`；每次 accepted
change 和 history 同一 transaction 提交，使用共同跨進程 WriterLock。
SQLite backup 可保全這些 accepted tables；reindex 不刪除它們。

## Taxonomy 與 entities

讀取優先序：native accepted `catalog_documents` → 真實
`wiki/_meta/taxonomy.json` → 從目前可見 article tags 生成 deterministic
fallback。沒有文章／沒有快取的空資料是可辨識的真實狀態；損壞 JSON 則
回錯，不假作空成功。分類寫入保留整個原 JSON 及未知欄位，置 `locked:true`，
與 `catalog_document_revisions` 原子提交。既有 legacy JSON 不被投影覆寫。

projection 解析 live article slug/title，children-first 去重，提供
`id/label/count/total/articles/children`。多語 label map 與自定 code 可用；
`metadata.titles[lang]` 優先，另保留常用雙語 title 分片。分類 ID 唯一、
深度 ≤32、category ≤4096、文件 ≤4 MiB。新 accepted taxonomy 的每個引用
須指向可见且唯一的文章。無 Private 的 writer 不能改寫或建立私密 taxonomy。

公開讀取採保守投影：private/internal 節點，以及自身 article references
含不可見／不明來源的 subtree，整個隱去；不能可靠地拆開其 aggregate label
來源。未知 raw JSON 欄位仍保留在 accepted storage，並不整包公開。

Entity reader 讀 `entities.json` 的 people/events/places；沒有快取會回
`origin:not_generated`，沒有假稱已執行 extraction。public entity 必須有
live article provenance 且全部 references 可見；混有私密來源的 aggregate
整條隱去。recomputed article count 不含私密文章。單類最多 10000 entries，
cache ≤4 MiB；corrupt／不合形態資料明確失敗。

## 驗證及尚欠契約

[`tests/catalog.rs`](../../crates/llmbase-core/tests/catalog.rs) 最近一輪 **5 tests
passed**（`.rust-migration-runs/maintenance-catalog.log`），包含 aliases／backlinks／
有界 traversal、雙來源刪一、stale evidence、contradictory kinds、重開保存、
relation revision history、跨庫及偽證据、private boundaries、完整 taxonomy
JSON extras、children-first 去重、entities filtering、corrupt-cache failure。
taxonomy mixed/private subtree 與 writer gate 已在該輪通過；同輪 maintenance
另 8 tests 通過。

精確尚欠如下，不能憑上述測試稱全部 legacy parity：

- 舊 `aliases.json` 人工 overrides、其 first-match fuzzy cascade、完整 s2t
  variant dictionary 在 Python compatibility snapshot 層另辦；native
  indexed aliases 採 NFKC／t2s／大小寫正規化、空格變形及歧義拒絕。
- 本模組不覆寫 legacy files；獨立 [`maintenance`](maintenance.md) 現可重建
  public `aliases.json`／`backlinks.json`。`taxonomy.json` 不被投影覆寫，
  accepted SQLite taxonomy 優先於之後人工改動的 legacy cache。
- 這裡沒有假實作模型 taxonomy generation、tag synchronization、entities
  extraction／dedup callable hooks、custom TAXONOMY_GENERATOR 或 Python
  formatter execution；這些須由其 native／compatibility 任務分別驗收。
- Category article membership 仍以 legacy slug 保存；文章搬名後需更新
  accepted taxonomy。manual relation 及 evidence 以 stable source ID 保存，
  不受純搬名重指影響。
- 有界 graph 不是完整時序 GraphRAG，不含無限制全圖推理或自动寫入。
