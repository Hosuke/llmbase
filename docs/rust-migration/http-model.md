# Native HTTP legacy model routes

薄映射在 [`http.rs`](../../crates/llmbase-cli/src/http.rs)，所有 query／compile／
repair／Xici／entities 演算法委派 Application／Gateway 及同一 Rust core。
此頁為當前實作 snapshot；6 組 model fixtures 與 6 組既有 SSE tests 已過。

| Route | Web envelope／defaults | Agent envelope／defaults | Native delegation |
|---|---|---|---|
| `POST /api/ask` basic | `answer`、有寫檔才 `output_path`；deep=false、file_back=true、tone=default | `status:ok,answer`；同樣 deep=false、file_back=true；舊 basic 不回 path | Web direct `legacy_query` with return_path；Agent `kb_ask` |
| `POST /api/ask` deep=true | `answer,consulted`，可有 output_path／promotion | `status:ok` 加原 workflow result | `kb_ask` → actual selector／answer／可選 promotion |
| `POST /api/compile` | `status:ok,articles_created`；new mode，舊 Web 忽略 full body | `status:ok` 加 compile result；full=false，true 指 all | `kb_compile`，只計已接受文章 |
| `POST /api/lint` | basic `results`；deep `report` | 同欄位加 status:ok | deterministic core／`maintenance_repair(mode:deep_lint)` |
| `POST /api/lint/fix` | `status:started,message,job_id`；durable enqueue＋精確 job_id background worker | 同步 `status:ok,fixes,fix_count`；partial 不改稱 ok | `maintenance_repair`，Web heal 兼寫真 health report |
| `POST /api/entities/extract` | actual entity result 原 envelope | 非 Agent route | `entities_generate` |
| `POST /api/xici/generate` | actual text/themes/lang/timestamp/count；語言按 Web profile，缺省通常 zh；顯式文言 base策略 | 非 Agent route | `xici_generate`，真正 model output／cache |
| `GET /api/tones` | 五個 native compat_workflows 支援的舊 metadata descriptors | 非 Agent route | `web_tones` |

Web deep default 以 frozen baseline `a7c4d9d:llmwiki/web.py:932` 的 **False**
為準。其與 `kb_ask` registry default deep=true 不同；HTTP adapter 顯式傳 false，
不能直接沿 registry default。新 `/api/v1/ask`／SSE 與 CLI `ask` 仍是獨立
English／read-only evidence contract，不改此 legacy file-back default。

Body 遞迴拒 api-key-like 欄位，包含 nested arrays／objects。`X-LLM-Key` 只在
body scan 後注入 legacy ask 的 transient args；Application 取出 SecretKey
之後才把剩餘 request 交 workflow／ledger。Selector 與 answer 用此 override，
promotion judge 保 server client，與舊 query.py 契約一致。Header 不授 server/library authority；
模型與寫入仍驗 bearer capabilities。Model override 限 string 200 bytes、key
500 bytes，model allowlist 仍由 `LLMBASE_MODEL_ALLOWLIST` 約束。

每個 legacy write/model route 保既有 scope，public／cookie 不取得 write/model
capabilities。Ordinary request disconnect 由 Application drop guard 取消 provider；
Web fix 是已授權 durable background job，回應斷線不取消。Job specific claim
不能偷跑其他 queued operation；process 中斷可由持久 worker 重試其 job。

有意差異：Web fix 新增真 job_id，改以 durable ledger 與 health report 提供
結果；`last_fix.json` 已由 core shared journal 保存舊 fixes／fix_count／status 三欄，
成功才為 completed。Private repair 另存 last_fix-private.json；失敗為 failed／partial，
不冒稱 completed。此 artifact 已經 HTTP背景job＋core成功／private／failure測試。Native repair
未知非字串 tags 保存、CANNOT_GENERATE 不造 placeholder、失敗／partial 不冒作
成功。詳見 [maintenance-repair.md](maintenance-repair.md)。Custom Python tone／
Xici callable／mutable Flask hooks 仍屬 facade/plugin host 契約，此路由不聲稱
由固定 descriptors 或 serialized prompt 取代可執行 callbacks。

[`http_model_contracts.rs`](../../crates/llmbase-cli/tests/http_model_contracts.rs)
已具 6 組真 provider HTTP fixtures，覆兩 factory defaults／不同 envelope、
header key 不入 DB、deep consulted/file-back opt-out、body key／scope gates、
真 accepted compile、Xici/entities/tones 及精確 background job。2026-09-20
`.rust-migration-runs/http-model-tests-v4.log`：model 6／6、SSE 6／6 passed；
後續 `.rust-migration-runs/http-ingest-tests-v2.log` 再驗 model6/read3 全過，
包含 last_fix 真artifact；core repair13亦過。
背景 worker 僅就開庫短暫 Busy 最多重試 100 次、間隔 20 ms，始終只 claim
原 job_id；持久 job 維持真狀態，未完成不作 succeeded 回報。
