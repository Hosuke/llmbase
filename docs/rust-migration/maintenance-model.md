# Native taxonomy 與 Xici generation

本頁限於 [`maintenance_model.rs`](../../crates/llmbase-core/src/maintenance_model.rs)
的真 provider-backed core；transport 接線須另驗，不宣稱全舊契約完成。
舊實作為 [`taxonomy.py`](../../llmwiki/taxonomy.py) 及
[`xici.py`](../../llmwiki/xici.py)。Rust 不呼叫 Python，不以範例文字或空成功
冒充模型生成。Client、credentials 及授權來自 host；request 無 API key 或
任意 cache path。

## API 與預設

`Library::generate_taxonomy(&Client, &TaxonomyRequest, access_private,
CancellationToken)` 及 `generate_xici(&Client, &XiciRequest, access_private,
CancellationToken)` 回真 JSON result。Application 須先驗 library scope 及
Read／Write／Model；私密輸入還須 trusted Private gate。`access_private` 不可
直接來自 body。

| 項目 | Native 預設 | 舊流程及差異 |
|---|---|---|
| Taxonomy labels | `["en"]`，可顯式指定最多 16 種 | 舊 `["en","zh","ja"]` 及可改 `TAXONOMY_LABEL_KEYS` |
| Taxonomy prompt | domain-neutral system／template，可顯式覆寫 | 舊 constants 及 callable `TAXONOMY_GENERATOR` 尚未由本模組接 facade |
| Taxonomy budget | 8192 tokens、65536 context bytes | 舊 `min(config.max_tokens*2,16384)` |
| Assignment | 重複 slug 留首次、漏項進 `other`，未知／越權 slug 拒絕 | 另驗 ID 唯一、depth 32、4096 nodes、cache 4 MiB |
| Model failure | 回錯，不改舊 cache | 顯式 `fallback_to_tags:true` 才 fallback，`generated:false` 且 job `partial`；舊自動 fallback |
| Category tag writes | 預設 false，須 `sync_category_tags:true` | 舊總會改 tags；新每篇 source journal，真 partial 明列 failures |
| Xici language | English (`en`) | 舊 `zh`；custom style、system、translation instruction 可顯式指定 |
| Xici strategy | `direct`，目標語一個真請求 | `legacy_zh_base` 顯式先文言再翻譯；舊總採此流程 |
| Xici budget | 1024 tokens、32768 context bytes | 舊 1024 tokens；新 context 有顯式上限 |
| Cached base | source ID／revision／hash fingerprint 相同才重用 | 舊只比文章數，正文已改亦可能沿用舊文言 |
| Empty library | 真空結果、`reason:no_articles`，無 provider call | 沒有來源，非模型成功的替代 |

共同限制：output 1–16384 tokens、context 1024–524288 bytes、system 及各
style/template 最多 64 KiB。Language code 為 Unicode 字母／數字／`-`／`_`，
1–64 bytes；Xici 的 `private-` prefix 保留給 cache namespace。

## Scope、來源及人工內容

生成先 sync，再持 writer lock 擷取目前 compiled sources；預設只有 public。
即 owner 亦須顯式 `private_inputs:true` 才使用私密來源。模型只收 scoped
title／summary／tags／slug，非不受限 filesystem scan。沿用 live reader 的
20000 sources／64 MiB 上限。

Output 記 `source_ids`、`source_fingerprint`、`article_count`、`visibility` 及
`generated_at`。Fingerprint 含 ID、revision、hash；發布前重驗 current state
及實際檔案 hash。生成途中人工改文、刪文、改 private 或 input set 變更，
均 Conflict，不發布舊 snapshot。

Accepted 或 legacy taxonomy 若 `locked:true`，回 scoped projection，無模型
請求或寫入；模型執行中才 lock 亦阻止發布。人工 `set_taxonomy` 保存 catalog
revision 並鎖定，priority 高於 generated file。

Public output 寫原有 `taxonomy.json`、`xici-<lang>.json`。Private output 寫
獨立 `taxonomy-private.json`、`xici-private-<lang>.json` 並標 private，不改
public cache。前者尚缺完整 transport read route；Xici 舊 reader 可在 Private
gate 下顯式讀私密 cache，public gate 拒絕。舊 cache 的來源證明限制詳見
[maintenance.md](maintenance.md)；來源宣告本身不是 cryptographic attestation。

Private generation 不容 `sync_category_tags:true`，免將私密衍生分類寫入公開
articles。Public tag sync 只改 `category:` tags，保留其他值、未知 frontmatter 及正文 bytes。
每篇以 ID／revision／hash 作 SourceGuard，完整 originals 存 source journal；
YAML 表面格式可能重排。Cache 與後續各篇 tag write 非跨檔原子交易：後者失敗
或取消，job 明示 `partial`。Cache fingerprint 保留生成時的 revisions，不
重指向後來 tags revisions 冒稱新證據。

## Provider、jobs 與回復

Client 發送真 HTTP chat-completions；每 15 秒更新 fenced lease，lease 60 秒。
取消中止請求；HTTP error、timeout、truncated、空文字、taxonomy 非 JSON／
非法 shape 均失敗。`generations` 含真 model／finish／usage／attempts；ledger
只記 profile hash、來源 fingerprint、sanitized error，不記 provider 錯誤正文。

Xici 翻譯成功後才提交新文言及目標語同一 durable batch；失敗不把文言偽稱
目標語、不覆蓋舊 cache。所有目標檔 CAS 先驗，再於同一 SQLite transaction
存舊／新 bytes 的 `maintenance_generated_revisions` 及 pending intent。

Generated artifacts 另帶 input source guards。Recovery 在重放每檔前重驗
indexed revision／hash 及 disk bytes；來源變更即 fail closed。此時開庫亦
Conflict，須 operator 核 pending intent／新檔後調解，不自動刪意圖或覆寫人作。
每檔 stage／fsync／rename／directory fsync；兩語檔並非跨檔原子發布。直接
filesystem consumer 在 recovery 前仍可能讀混合代次。有 pending intent 的
錯誤 job 為 `partial`；尚未發布的錯誤 `failed`，取消 `cancelled`。

## 驗證及待辦

[`tests/maintenance_model.rs`](../../crates/llmbase-core/tests/maintenance_model.rs)
有 9 個真 loopback HTTP-backed cases，2026-09-20 Rust 1.98.0 全通過：assignment／
tag history、locked accepted、invalid／unknown／truncated／provider errors、
途中來源或 lock 變更、public/private Xici、文言重用與翻譯、timeout／失敗保檔、
recovery private guard、取消及顯式 fallback（含 private taxonomy tag-sync 拒絕）。
同批 maintenance9／catalog5 亦通過；public-list 新 visibility guard 引起的
mutation error mapping 已修正，web_write3 回歸通過。Log 為
`.rust-migration-runs/maintenance-model-v2.log` 及 `maintenance-model-v3.log`，
最後 exit 0；相關 tmux 已結束。未將此 core 驗證冒稱 transport 驗收。

尚欠：舊 >100 articles taxonomy two-phase／batch assignment、舊 fallback tree
逐 fixture parity、Python callable／hooks adapters、定期 worker 及 legacy
transport envelopes。Single-pass 超 context 會明確失敗，不截斷 inventory
而假稱全庫完成。Xici compact overview 為真 tag frequencies／samples，尚非舊
排序逐字一致。完整 auto_fix／semantic normalization／dedup／heal／deep lint
仍待另作，不因本模組已能生成而視為完成。
