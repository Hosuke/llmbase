# Native deterministic Web mutations

[`web_write.rs`](../../crates/llmbase-core/src/web_write.rs) 持有本 slice 的業務邏輯，
[`http.rs`](../../crates/llmbase-cli/src/http.rs) 僅映射 DTO 到 Application。所有
操作須 scoped Write＋Read；研究 trails 另須 Private。Public reader／舊 cookie
不得觸發任何以下 mutation。

| 原有 Web route | Native operation | 真正行為 |
|---|---|---|
| `DELETE /api/articles/<slug>` | `web_write_article_delete` | exact visible article slug，shared source journal 刪除，保存原 revisions |
| `POST /api/wiki/clean` | `web_write_wiki_clean` | 舊 Web garbage predicates，逐篇 CAS journal delete，成功刪文後真 metadata rebuild |
| `POST /api/taxonomy/update` | `web_write_taxonomy_update` | 共用 accepted catalog taxonomy write，保存未知 JSON、locked 與 revision history |
| `POST /api/trails` | `web_write_trail_save` | 新建或 append 真步驟，server timestamp、原 name／unknown fields 保存 |
| `POST /api/trails/<id>/delete` | `web_write_trail_delete` | 固定 trails.json 中刪指定 accepted trail，journal 保存前後 bytes |

Article mutation 先核 scoped indexed snapshot 的實際磁碟 hash；外部未 sync 的
改文回 Conflict。真正刪除以來源 ID＋expected content hash 經共同 source
journal；不是直接 unlink。Private 來源只由相應能力持有人讀改。Cleanup
逐字採舊 Web `api_clean_wiki` 的模板／prompt leak／placeholder／正文少於
50 Unicode scalar 規則，並非完整 lint auto-fix 或語意垃圾判斷。原文章 bytes
仍留歷史 revision；不宣稱這些 predicates 適用所有自定文體。

Cleanup 回實際 `removed/slugs/failures`：部分已刪而別項衝突時 `status:partial`，
全敗回 Conflict。空候選且無錯為真 removed 0。Rebuild 只公開 public projection，
private owner 執行亦不把私密內容寫入無 scope 的四個 legacy index 檔。

Taxonomy update 延用 catalog 的 accepted SQLite/history，不另重寫 taxonomy
演算法。`taxonomy.json` 舊檔未被原地覆寫；native reader 採 accepted DB 優先。
此為 legacy filesystem write parity 的尚欠處，不能僅凭 endpoint 回應稱舊
JSON-file consumers 已看到更新。Category references、private nodes 與 bound
validation 比舊 route 更嚴。詳見 [`catalog.md`](catalog.md)。

Trails 保留固定 `trails.json` 路徑。共同 writer lock 下讀／改，再把原 bytes、
新 bytes 的 `maintenance_trail_revisions` 與 pending artifact intent 放入同一
SQLite transaction。沿用 maintenance recovery 的 CAS、stage、fsync、rename；
不另造第二套 journal。Library 重開先 recovery；如果人在 pending 期間另改
trails，保留其新檔並回 Conflict。SQLite backup 同時含 accepted history 與
pending intent。每次 mutation 保留未知 document/trail/step 欄位；incoming
`step.ts` 必由 server timestamp 覆蓋。刪不存在 ID 沿用舊 idempotent status ok。

Trail JSON 上限 4 MiB，trail／單 trail step 各最多 10000，name 最多 4096 bytes；
畸形舊 JSON 明確失敗，不假作空列表覆寫。新建無 type 的空步驟沿舊 contract
不加入 steps；append 則可加入空步驟及真 server timestamp。此 API 不接受任意
輸出檔名或路徑。

尚未由本 slice 完成：legacy model ask 的 file_back/promote/tone、compile accepted
pipeline、deep lint、完整 auto-fix、ingest/upload、entities／Xici generation。
這些不能直接把 native review draft 假裝已接受文章；callback／native orchestration
將按各自原契約接線。Full write failure 的 HTTP error 是統一 structured envelope；
missing article 返回404，舊 delete 曾回 HTTP200/error，差異明列。

驗證檔：[`core tests/web_write.rs`](../../crates/llmbase-core/tests/web_write.rs)
三項涵蓋 actual delete/clean、private 與 stale guard、taxonomy unknown JSON、trail
完整 cycle／timestamp／accepted history／pending crash recovery；
[`HTTP tests/http_write_contracts.rs`](../../crates/llmbase-cli/tests/http_write_contracts.rs)
一項串起 public denial→owner clean→taxonomy→trail save/delete→article delete。
2026-09-20 主線 `.rust-migration-runs/yaml-web-write-v8.log`：core web_write
**3 passed**、HTTP write **1 passed**；同輪 maintenance 9、HTTP read 3、
transports 6 亦過。全部 mutation 僅臨時 fixtures；未啟 production server。
