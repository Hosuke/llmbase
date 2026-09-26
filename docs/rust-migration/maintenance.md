# Native maintenance、匯出與 legacy metadata

本頁記錄已落實的 Rust maintenance slice，非整體 Python parity 完成聲明。
實作在 [`maintenance.rs`](../../crates/llmbase-core/src/maintenance.rs)，
fixture 在 [`tests/maintenance.rs`](../../crates/llmbase-core/tests/maintenance.rs)。
所有通用演算法均在 Rust；本模組不啟動 Python，不以空成功代替模型生成。

## 契約與 API

| 舊契約／原實作 | Native API | 已具行為／界限 |
|---|---|---|
| [`compile.rebuild_index`](../../llmwiki/compile.py:337) | `Library::rebuild_metadata()` | 同步真實來源後寫 configured meta 內四個既有檔案；回傳 public article count、all indexed count、generation、sync report |
| [`export.export_article`](../../llmwiki/export.py:18) | `export_article(slug, &LanguageProfile, include_private)` | 真實 article metadata、分語內容、出入鏈接、最多十個 shared-tag articles |
| [`export.export_by_tag`](../../llmwiki/export.py:114) | `export_tag(tag, include_private)` | 大小寫無關 tag matching；保留 tag 原字及 index entry 欄位 |
| [`export.export_graph`](../../llmwiki/export.py:139) | `export_graph(slug, depth, include_private)` | outgoing＋backlinks 廣度遍歷，depth 0–4、1000 nodes、每節點 200 neighbors，截斷明示 `truncated` |
| [`lint.checks.lint`](../../llmwiki/lint/checks.py:56) | `lint(LintOptions, include_private)` | 八個真實 deterministic checks，保留 arrays＋`total_issues` envelope |
| [`lint.fixes.auto_fix`](../../llmwiki/lint/fixes.py:448) 的局部 | `fix_deterministic(FixOptions, include_private)` | 僅現有 tags 的 trim／lowercase／去重／去空字串及可選 metadata rebuild；完整 auto-fix 仍欠 |
| [`xici.get_xici`](../../llmwiki/xici.py:201) | `xici(lang, include_private)` | 讀真正 `xici-<lang>.json`；缺檔、損壞、私密 aggregate 各有明確結果 |

`include_private` 是 Application 授權後的 trusted gate，不能直接採信 HTTP
body。`LintOptions` 為 `allow_cjk_slugs:false`；`FixOptions` 為
`normalize_tags:true, rebuild_metadata:true`，兩者均拒絕未知欄位。Native
reader 讀已索引的 current source revision；外部寫檔須先 index/sync，重建本身
會 sync。讀取 snapshot 上限 20000 篇可見文章及 64 MiB，pairwise duplicate
lint 另限 2000 篇，超限回明確錯誤。

## 原有檔案與公開 projection

舊 `rebuild_index` 真正寫於 `cfg["paths"]["meta"]`：

- `index.json`：每列 `slug/title/summary/tags/sources`。
- `_index.md`：Wiki Index frontmatter、UTC updated、Knowledge Base Index、
  sorted tag groups、`[[slug|title]] — summary`、article count。
- `aliases.json`：title/slugs／title 分片／metadata aliases、merged_from；
  另產生簡繁 variants。已有人工 alias override 若仍唯一指向可見文章即保留。
- `backlinks.json`：實際解析 outgoing wiki-links 形成 target slug → incoming slugs。
  亦保留 genuinely unknown target 的 lowercase／space→hyphen fallback keys，
  供舊 lint/stub workflow；可辨識 private 或歧義目標不投影。

未擅造或覆寫 `wiki/index.md`。文章正文、accepted taxonomy、手工關係亦不由
此 rebuild 改動。既有 alias 指向 private／不存在／歧義目標時不公開該條。
Malformed aliases JSON 會失敗，不被當作空字典抹除。

四檔永遠只載 public compiled articles；即使用 owner credentials 重建亦然。
這些 legacy/static 檔案沒有逐請求授權 gate，故不能安全投影 owner 私密視圖。
結果明列 `projection_visibility:public`、`article_count`（public）與
`indexed_article_count`（全部 current compiled）。同一 meta 下再寫
`projection-generation.json`，含 version 1、generation UUID、public count
及四檔 SHA256。這是新增驗證檔，不改四檔既有 envelope。

Alias resolver 使用 NFKC／t2s／大小寫正規化，space→hyphen／space 移除及
existing override；歧義回 Conflict。它尚非舊 `resolve_link` 完整 first-match
fuzzy cascade。Indexed catalog resolver 與本 legacy-file-aware export
resolver 的取捨另見 [`catalog.md`](catalog.md)。

## Durable projection batch

`maintenance_projection_batches` 是同一 library SQLite 中的 durable journal，
不是 accepted content 的替代。共同 WriterLock 下先存整批 payload、舊檔 hash
及 pending 狀態，再依序安裝四檔，最後安裝 generation manifest，方記 complete。
每檔以同目錄 create-new stage、file fsync、rename、directory fsync 完成。

Root store 已在 `Library::open` 與 sync 的既有 writer lock 內調用
`recover_projections_locked()`；不重入鎖。Recovery 對每檔只接受兩種情形：
現況 hash 仍與 journal 預期相同，或已完整等於 intended bytes。若人已另改
aliases 等 projection，回 Conflict，保留其新 bytes；不靜默覆寫。Replay
驗證 artifact 檔名、數量、manifest 最後次序及 64 MiB 上限。

四個固定 legacy 檔名不具跨檔原子性。Native open 恢復完成後才讀；直接讀
filesystem 的既有消費者，在崩潰至恢復期間仍可能讀到混合代次。新增消費者
應核對 manifest 及各 hash，失配則等待 recovery／重讀。這不等同改成 generation
pointer 的原子發布協議。Legacy rebuild journal 重播原已生成的 snapshot；
它不重新推導期間外部變更的來源，外部變更須隨後 sync＋rebuild。模型生成
artifacts 另有逐 source revision／hash guards，回復前重驗實際來源；詳見
[maintenance-model.md](maintenance-model.md)。

Compatibility 共用 `rebuild_metadata_compat()` 回同批次的真 index entry array，
及 `rebuild_backlinks_compat()` 僅 journal／更新 backlinks 一檔、回真 map。
後者不改 index、markdown 或 aliases，也不更新 full-rebuild manifest；新讀者
若需全代次一致性，須 full rebuild 後再驗證 manifest。`pub(crate)` aliases／
resolve helpers 共用同一 Snapshot，供 native compile facade，private scope
由 caller 明定；沒有另複製 alias 演算法或公開 Snapshot internals。

## 匯出、lint 與修復

Article envelope 保留 `slug/title/summary/tags/sources/created/updated/content/
backlinks/outgoing_links/related_by_tags`；export article/graph 排除 `category:`
tags。內容依顯式 `LanguageProfile` 分段，既有 English／中文／日本語映為
`english/zh/ja`，自定 key 原樣保存，空 header 的單段文言全文亦可用。
Backlinks 在 native export 由 live indexed links 推導；不沿用 stale
backlinks cache。Shared tags 大小寫無關，先依 shared count，平手依 slug，
最多十條。不存在 article/root 回 NotFound；舊 article 回 `None`、舊 graph
可回空 nodes，此處差異須由 compatibility entrypoint 映射。

Lint 實作八類：structural、broken_links、orphans、missing_metadata、dirty_tags、
duplicates、stubs、uncategorized。檢查實際檔案／metadata／links／taxonomy；
duplicates 移植 cheap slug/tag/CJK-name 候選分數，不宣稱語意重複判定。CJK
slug 警告有明確 option，預設仍沿用舊檢查；custom profile 可准 CJK slug。
uncategorized 遞歸尋找 `other` category，較舊 top-level-only check 廣。
缺 backlinks projection 仍回舊「Backlinks map not built yet」提示；存在時
以目前可見 incoming links 判 orphan。

Deterministic repair 僅格式化已有字串 tags。非字串 tag 保留，繼續由 lint
報告，避免把未知 accepted 值直接刪掉。修改保留未知 frontmatter JSON 與
正文 bytes；YAML 表面格式會重排，但原 revision 完整保存。每篇寫入使用
共同 `write_source_guarded` journal，加 `source_id/revision/content_hash` 與
target file hash guard；索引後的人工作改，不能被舊 snapshot 覆蓋。

回應含 `scope:deterministic_tag_formatting`、實際 fixes／failures／projection；
有改成功而另有失敗為 `partial`，沒有成功修改且出錯則回 Conflict，不能
全敗仍 exit 0。完全無需修改而無錯是實際 succeeded／fix_count 0。這個 API
不是舊 full `auto_fix`；不得由相同名稱造成已支援生成、合併或刪除的誤認。

## Xici 快取

檔名確為 `xici-<lang>.json`，不改成 underscore。Language code 限 1–64 bytes
字母／數字／`-`／`_`，拒絕 traversal，cache 限 4 MiB。正常快取保留 text、
themes、lang、generated_at、article_count 及未知欄位，附 `origin:legacy_cache`；
缺檔保留舊空狀態欄位，附 `origin:not_generated`。損壞 JSON／不合形態失敗。

公開快取不得標記 private/internal。若宣告 `source_ids`，需非空、唯一、數量
與 article_count 相等，每一 ID 均在此 library 可見且未刪除。舊 cache 未記
來源時，若 library 任一 compiled revision 有 private history，public 讀取
明確拒絕；無法由 aggregate prose 可靠拆出私密句。Private capability 可讀
完整原快取。這是比舊 unscoped cache 更嚴的公開界線，不宣稱來源宣告本身
構成模型生成的簽章或 evidence。

## 驗證及尚欠工作

2026-09-20，Rust 1.98.0，最初 maintenance 8＋catalog 5 通過；其後
`cargo test -p llmbase-core --test maintenance --test web_read` 再驗：
**9 maintenance＋3 Web read passed，exit 0**。最新 Log：
`.rust-migration-runs/web-read-core.log`，tmux `llmbase-web-read-tests` 已結束。
測試覆蓋：

1. 真實多語及 custom exports、tag/graph、private boundary。
2. 四個既有檔案、人工 alias、簡繁 alias、public/all count、manifest hash、
   原文章及手工 `wiki/index.md` 不變。
3. 持久 SQLite pending batch 在首檔前／首檔後的兩個 crash windows 重開恢復。
4. Recovery 不覆蓋新人工 alias bytes，衝突失敗。
5. Tag repair 走真 journal、正文／未知 metadata／舊 revision 保存、私密原檔不動。
6. 未索引的外部私密改文阻止舊 snapshot overwrite。
7. 真 broken links、missing metadata、duplicate 候選、CJK policy 與總數。
8. Xici 缺檔、private history、declared public provenance、traversal、corrupt JSON。

9. Legacy index array、unknown/private backlink keys、獨立 backlinks rebuild
   不變其餘三檔。

`write_compile_sources_compat` 另供 native compile facade 寫固定
`compiled_sources.json`，同 journal single-artifact CAS/recovery。此檔是
private operation state，不由 Web public projection endpoints 提供。Raw compiled
flag 與 source-log receipt 分別持久化，不宣稱跨兩者原子提交；compile facade
的恢復驗證由其專項測試承接。

尚未由本 slice 完成：舊 Python callable／hooks entrypoints 的 native 接線與
完整同 fixture parity；semantic tag normalization、垃圾文刪除、URL slug repair、
模型補 metadata／stubs、duplicate merge、deep lint 與自定 callable execution。
Taxonomy／Xici 原生生成已另成 [maintenance-model.md](maintenance-model.md)
slice，仍須專項驗證與舊 entrypoint 接線。上述不能以 deterministic fix 或
cached read 代替。Application／transport authorization 與路由整合由主線另驗。
