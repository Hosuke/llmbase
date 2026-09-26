# Native repair pipeline

[`maintenance_repair.rs`](../../crates/llmbase-core/src/maintenance_repair.rs) 實作
有界的 cleanup、改名、模型修 metadata／tags／stubs、dedup、check-fix-recheck
及 deep lint。每次改 source 均走共同 journal；無 Python subprocess 或空成功。
此頁不代表 Python callable facade、所有 CLI／HTTP／MCP 舊 envelope 已接妥。

## API 與舊契約

`persist_last_fix=false` 為 native 預設；Web 背景修復顯式啟用。結果由同一
generated metadata journal 寫 `last_fix.json`，舊三欄 fixes／fix_count／status
保留，真成功才 completed，partial／failed 明示。Private inputs 另寫
`last_fix-private.json`，不覆 public receipt；read-only DeepLint 禁此 flag。
兩項新增成功／private／provider failure 測試使 repair target 共 13 項，
已在 `.rust-migration-runs/http-ingest-tests-v2.log` 全過。先前 core clippy
已過；新增 receipt changes 的 strict clippy 待下一 Cargo slot。

`Library::repair(Option<&Client>, &RepairRequest, access_private,
CancellationToken).await`。`access_private` 是 host 授權，不採信 request body；
Application 必須先驗 library scope、Read／Write，模型 stage 另需 Model，
私密輸入另需 Private。Pure `clean`／`heal_slugs` 不需 Client，其餘 mode 需
已配置的真 Client。原生 `RepairRequest` 拒絕未知欄位。

| Mode | 舊來源 | 實際 native 行為 |
|---|---|---|
| `clean` | [`clean_garbage`](../../llmwiki/lint/fixes.py:257)、[`check_stubs`](../../llmwiki/lint/checks.py:281) | 真查 template、placeholder、summary prompt leak、empty title、非 stub 短文；journal delete，原 revision 保存 |
| `heal_slugs` | [`heal_urly_slugs`](../../llmwiki/lint/fixes.py:164) | nested URL-shaped relative paths sanitize、collision 拒絕、journal rename、更新原有 slug field、重寫 wiki links 並保 display text |
| `dirty_tags` | [`fix_dirty_tags`](../../llmwiki/lint/fixes.py:116) | 移去 prompt-leak／過長字串；不足兩 clean tags 時真模型補 tags；unknown 非字串 metadata 保留供 review |
| `normalize_tags` | [`normalize_tags`](../../llmwiki/lint/fixes.py:33) | 五種以上 tags 才請模型；top 80 頻次、嚴格 old→canonical JSON；scope 分組，各篇以完整 aggregate inputs 的 CAS 寫回 |
| `metadata` | [`auto_fix`](../../llmwiki/lint/fixes.py:448) metadata pass | 缺 summary／tags 才生成，一篇完整成功後一次 journal write，正文及未知欄位保留 |
| `broken_links` | [`fix_broken_links`](../../llmwiki/lint/fixes.py:303) | alias resolution 後找真缺項，scoped 上下文作來源；真模型 stub、保存 source ID／revision／hash，拒答不建 placeholder |
| `deduplicate` | [`merge_duplicates`](../../llmwiki/lint/dedup.py:131) | cheap candidates 後模型確認同一概念；append 保雙方正文與來源／tags／aliases／merged_from、重寫 links，最後才 journal delete secondary |
| `auto_fix` | [`auto_fix`](../../llmwiki/lint/fixes.py:448) | heal_slugs→clean→dirty_tags→normalize_tags→metadata→broken_links→deduplicate，再視 uncategorized 與設定執行真 taxonomy generation |
| `heal` | [`lint_heal_cmd`](../../llmwiki/cli.py:530) | 真 check→auto_fix→recheck，持久 `health.json`，數目直接來自實際 lint result |
| `deep_lint` | [`lint_deep`](../../llmwiki/lint/checks.py:91) | scoped article metadata 的真模型報告，發布前再驗來源，無 article mutation |

Default language profile 是 English bare body；可顯式提供文言或其他 sections。
`system_prompt`、`stub_system_prompt`、`model`、`profile` 與 taxonomy profile 可
配置；不假稱 Python callable 可以由 JSON 代替。Requests 不接 arbitrary paths
或 credentials。

## 預設與 deliberate differences

- `private_inputs:false`、`allow_cjk_slugs:true`：原生不把正常漢字 slug 當垃圾。
  舊 lint 的 pinyin policy 可顯式設 false；同一選項亦用於 heal 前後檢查。
- `confirm_duplicates:true`：同題、相近、上下位或相反立場不是重複。模型須回
  boolean decision；false 真保兩篇。`false` 顯式重用舊 cheap-candidate merge
  方式，不把此模式描述為模型確認。
- `max_articles:1000`（最高 2000），超出明確錯誤，不暗截 inventory。
  `max_merges:15`、`max_stubs:10`、`max_candidate_pairs:100`；達限尚有工作則
  failures／partial 明示。`context_bytes:65536`，可 1024–524288。
- `fix_broken_links:true`、`regenerate_taxonomy:true`；單獨 mode 只做該工作。
  Taxonomy 沿用獨立 request profile，預設英文、不強制 tag sync、無自動假成功
  fallback。Locked taxonomy 不覆蓋；確有 uncategorized 而受 lock 阻止時明示。
- Tag JSON 必須全體合法；無效／截斷／空回答不被當空 list 成功。舊 normalize
  或 tag repair 的 swallowed provider error 已改為可見失敗。
- 無 `CANNOT_GENERATE` placeholder fallback。模型不足以生 stub 時不造內容，
  不將「has not been written」寫入 accepted wiki。成功 stub 亦標 `stub:true`，
  並保 consulted source revisions；不可視為已驗證的完整文章。
- Native dedup 原始正文只 append，不以模型摘要覆蓋雙方；contradictory prose
  即使被誤判 duplicate 亦留正文及舊 revisions。Primary 沿舊 ASCII preference／
  較長正文選擇；union 增保 secondary sources／aliases，免失去出處。

## Scope、journals、失敗與回復

Private model input 必 explicit 且具 trusted Private。Tag aggregation 依
public/private 分組，private canonical decisions 不寫入 public tags；dedup
禁跨 visibility。Stub 若任一 support private，整篇 private。Private-derived
link rewrites 不改 public articles。Private heal report 寫 `health-private.json`，
不覆蓋公開健康報告；其來源即使公開部分亦記 private aggregate。

每篇 body／metadata 修改、rename／delete 皆共同 source journal，按 ID／revision／
hash 及 disk bytes CAS。Normalize 對同組全部 model inputs 作 guard；完成一篇
只將該 guard 更新為那份確實接受的 bytes，其他 inputs 仍釘原 model snapshot。
Metadata edits 保正文 bytes；YAML 表面格式可能重排，original revision 不失。

Merge 先接受完整 primary，再重寫 links；任何 link rewrite 不全便留 secondary。
最後刪除前重驗預期 primary／secondary bytes，不能刪去期間新人工內容。流程
跨多篇，非單一原子交易；中斷時可能已合主文而尚未刪副文。重試 append 以
原 secondary content 是否已含於 primary 判斷，避免無條件重複追加。

Rename／merge 後修 taxonomy 引用；保原 locked、labels、children、未知 extras。
Accepted taxonomy 更新同 catalog revision history，不擅改其原生 DB authority；
legacy cache 走 metadata journal。人工已有 primary 分類優先，去死／重複引用，
新 public 漏項入 Other。Private reference 不能新投影至公開 legacy cache，
需要 scoped storage 的案例明示 partial；`taxonomy-private.json` 的完整 reference
maintenance 尚待接妥。此差異不以空刷新成功掩蓋。

每次 source mutation 的 lexical WriterSession 只跨同步操作，不跨 await；
`maintenance_repair_intents` 記該 job 對應的實際 write-intent IDs。Provider 等待時
其他 native writer 的意圖不算本 job。若先改一篇、後遇 provider error，結果
以實際 receipts 報 `partial`，不報全失敗／零副作用。Receipt association 與
source intent 非同一 DB transaction；process crash 可留已恢復 source intent
及 interrupted job，未宣稱整個 repair 自動 exactly-once resume。

模型等待共用 fenced heartbeat／cancel／truncation checks。已接受效果而另一項
失敗為 partial；未接受任何 requested repair 而失败則 error／failed job。
沒有需要改動且所有 checks／decisions 成功，可以 succeeded／0 fixes。Heal
workflow 成功不等於 `issues_after:0`；UI 應呈真 residual issue count。

## 驗證狀態與剩餘範圍

2026-09-20，Rust 1.98.0，最初 10 個
[`maintenance_repair.rs` tests](../../crates/llmbase-core/tests/maintenance_repair.rs)
全通過，log `.rust-migration-runs/fetch-check-v2.log`。覆 cleanup／nested URL、
unknown/body/revision preservation、multi-article normalization、private 分區、
真 provider 失敗後 partial receipt、concurrent private edit、confirmed merge／
negative decision、真 stub／拒答、health／deep lint、cancel/scope。其後第 11 項
taxonomy reference regression 及 final aggregate source check 已重驗：repair11／
catalog5／maintenance_model9 全過，`.rust-migration-runs/repair-final.log`；
core `cargo clippy --all-targets -- -D warnings` 亦綠，
`.rust-migration-runs/repair-clippy.log`，exit 0。相關 tmux 已結束。

尚欠 Python `check_stubs` monkeypatch／其他 callable hooks 的 host adapter、
舊 terminal wording／HTTP background envelopes 的逐 route parity、private
taxonomy cache reference refresh、跨整個 repair 的 durable resumable stage cursor。
Entity extraction 由另一原生 module 承接；本 pipeline 不宣稱包含 entity refresh。
