# 下游現況與候選驗收契約

盤點日：2026-09-20。此文據本機當前工作樹；兩下游僅讀，未改其源、未接生產文庫。此文之「須驗」皆非已成。

## 一、基線與保全

| 倉 | HEAD | 套件／依賴 | 工作樹 |
| --- | --- | --- | --- |
| Siwen | `692bc581a470e1bd618aad743fc5bd7b97b1ac2b` | `siwen==0.1.0`，宣告 `llmwiki>=0.7.8`，Python `>=3.11` | 初查 3 個 tracked 修改、18 個 untracked，詳下 |
| riddle | `0d1a9feea75027543e2ed49c721656cd261f03a1` | `riddle==0.3.0`，Rust edition 2021，`ureq 2.10` | clean |

Siwen 路徑為 `/Users/lanchair/Developer/Buddha/Github/siwen`。其 `.venv/bin/python` 經 `importlib.metadata.distribution("llmwiki")` 實查：

```json
{
  "version": "0.8.0",
  "distribution_path": "/Users/lanchair/Developer/Buddha/Github/siwen/.venv/lib/python3.12/site-packages/llmwiki-0.8.0.dist-info",
  "direct_url": null
}
```

無 `direct_url.json`，不可視作已用 0.9.0 或候選 editable。只看 `pyproject.toml` 下限更不足證已載何物。

Siwen 初查 `git status --short` 原樣記之：

```text
 M docs/taixu-v2-authority-and-ingestion.md
 M siwen/bake.py
 M siwen/schools/source_corpus.json
?? docs/architecture-review-2026-09.md
?? docs/daxingshan-2026-four-preliminaries.md
?? docs/daxingshan-2026-ingestion.md
?? docs/kukai-ndlocr-v3-handoff.md
?? docs/kukai-ndlocr-v3-manual-review.tsv
?? docs/kukai-t77-gap-report.md
?? docs/kukai-t77-ndl-handoff.md
?? docs/kukai-t77-volume-map.tsv
?? scripts/kukai_ndlocr_v3.py
?? scripts/kukai_ndlocr_v3_audit.py
?? scripts/kukai_t77_download.sh
?? scripts/kukai_t77_ndl.py
?? scripts/kukai_t77_ocr.sh
?? scripts/kukai_t77_ocr_audit.py
?? tests/test_bake_contract.py
?? tests/test_kukai_ndlocr_v3_audit.py
?? tests/test_kukai_t77_ndl.py
?? tests/test_kukai_t77_ocr_audit.py
```

本次審核末覆查，另有外部並行工作新增 `siwen/wenguan/ingest_four_preliminaries.py` 與 `tests/test_ingest_four_preliminaries.py`，合為 20 個 untracked；非本次遷移所寫。上表所核八個 Siwen 檔案 hash 仍悉同初值。候選測試複製時須固定當時 snapshot，不可擅清新檔以還原此表。

重要檔案 SHA-256，含既存未提交內容；驗收後須再核：

| 檔案（相對 Siwen） | SHA-256 |
| --- | --- |
| `siwen/patches.py` | `32c1c5e9ca95fe9c94ca03d7fbd81fa1512063f889e94d53e168eaa1ce185b5f` |
| `siwen/compile.py` | `65a28d7958fa71ffc5a1cc848dc97b4f5dec896af1f43ff033947405166f6f16` |
| `siwen/web.py` | `080180818aa02a136f03b9f1757f81cebf51ed6229a4489f4c9f4a31026430de` |
| `siwen/bake.py` | `89022a6b6ab2ef0a77ea0cec772f164764d3b1eb8ce97709b0f95117c41bdb0a` |
| `siwen/kb.py` | `a15813e49b123f81c388596470a4823738f793453a353f0a077ad06ce6616d9b` |
| `siwen/mcp_server.py` | `40cf39ce1e0c1abf891763253651b4a9dca8957f917ac60d18c4de58dfb750ee` |
| `siwen/wenguan/wenguan_book.py` | `4cbce93928191802ff876c19f6399f4ec3f1e749989653329cd474ceb83a9706` |
| `tests/test_bake_contract.py` | `7278e7702ef5e0f904aaff0bf38a0fc6c2816d66b6b6954756742e43a1428279` |

| 檔案（相對 riddle） | SHA-256 |
| --- | --- |
| `Cargo.toml` | `cf3fd5fae05a6141d18e242abdfcebb207bb38d56189f45d44b85c7d32b8ca36` |
| `src/oracle.rs` | `82b708a2cf60623170d1fd19cedab8fec4a99e714a47b26ad3c67f700f2cdd15` |
| `src/memory.rs` | `6cdac7fa52b89a14cd7ed31a1e0c8cb0ead54c5229fc75d2f6defcde8a0caadf` |

## 二、runtime override，須每次取其當前值

Siwen `patches.py` 真正所改如下。純資料可序列化成 profile；callable 須在明示 Python 擴展層執行，不能只送函式名、忽略返回或在 Rust 啟動時永久快取。

| 對象 | 現有動作 | 必守語義 |
| --- | --- | --- |
| `compile.SYSTEM_PROMPT`、`COMPILE_USER_PROMPT`、`COMPILE_ARTICLE_FORMAT` | 重新 assignment | 後續 compile／worker 用新 prompt；既有 placeholder 不變 |
| `compile.SECTION_HEADERS` | `[("文言", "")]` | assignment 勝 `languages` 設定；文言裸正文；不得 reassign `DEFAULT_SECTION_HEADERS` identity anchor |
| `query.SYSTEM_PROMPT`、`PROMOTE_SYSTEM_PROMPT` | assignment | 問答與升條皆守當前值；`TONE_INSTRUCTIONS` 亦屬 mutable dict 契約 |
| `taxonomy.TAXONOMY_SYSTEM_PROMPT` | assignment | 系统 prompt 動態有效；通用 `TAXONOMY_GENERATOR` callback 同須保留 |
| `xici.XICI_SYSTEM_PROMPT`、`LANG_STYLES` | assignment；styles 只留 `zh` | 移除語種有效，不可重新填回英文／日文 |
| `xici.chat` | 換 `_siwen_xici_chat` | 委 `safe_chat` 驗漢字比例 0.6、外層重試 3、讀 `SIWEN_XICI_MODEL`；須真回呼 |
| `entities.ENTITY_SYSTEM_PROMPT`、`ENTITY_PROMPT` | assignment | 模板 `{count}`、`{articles}` 可用 |
| `entities.ENTITY_ARTICLE_FORMATTER` | callable | 返 `list[str]`，逐條 `slug/title/summary/tags[:3]` 入 prompt |
| `entities.extract_entities` | callable 完整取代 | Python 擴展自管 150 條分批、逐批落 metadata；通用 native extractor 仍须由 Rust 實作 |
| `lint.checks.SYSTEM_PROMPT`、`lint.fixes.STUB_SYSTEM_PROMPT` | assignment | 深校與補缺 prompt 生效 |
| 三處 `check_stubs` | `lint.checks`、`lint.fixes`、`lint` package 全覆 | 不以 CJK slug 為垃圾；非 stub 短文界 30 字；不可單改 re-export 忽略已綁 callable |
| `llm.chat_with_meta` | 保存原 callable，再加剝思維標籤 wrapper | `(text, meta)` 不變；原函式仍可調；不可 facade 再查自身而遞迴 |
| `llm.chat` | wrapper | 返 `str`；signature 含 `prompt/system/model/max_tokens/api_key`；秘鑰不可記入 trace |

上游语言 profile 另有重要舊行為：`_patched` 的 `codes=["文言"]`，卻 `default_lang=api_default_lang="zh"`；Web `_resolve_lang` 對 patched profile 放行任意 lang，對 config profile 則驗 codes。此怪貌乃歷史相容，native 新接口可正規化，但 facade 不可暗改。

Siwen 自家 `taxonomy.generate_four_part_taxonomy` 不是將 `TAXONOMY_GENERATOR` 設為 callback。它直接產四部樹，以 `atomic_write_json` 寫 `_meta/taxonomy.json` 並鎖定。不得因同具「分類」二字便誤認已走 Rust。

## 三、實用 import 與私有 helper

| 模塊 | 下游實用符號 | 消費處 |
| --- | --- | --- |
| `llmwiki.compile` | `_find_uncompiled`, `_list_existing_concepts`, `rebuild_index` | `siwen/compile.py:21`；前者返 `list[Path]`，後者返 `list[str]`，形如 `slug (title)` |
| `llmwiki.entities` | `_parse_entity_response`, `_dedup_entities`, `_save_entities`, `get_entities`, prompt 常數 | `siwen/patches.py:132` 分批函式內延後 import |
| `llmwiki.config` | `load_config`, `ensure_dirs` | compile、web、learn、taxonomy、entities；`paths` 各項為已解析絕對路徑字串 |
| `llmwiki.resolve` | `build_aliases`, `resolve_link`, `load_aliases` | compile dedup、Web aliases |
| `llmwiki.agent_api` | `KnowledgeBase` | `SiwenKB` 繼承，依賴 `.base_dir` 與父類方法 envelope |
| `llmwiki.query` | `query_with_search` | `siwen/kb.py` 導入綁定，deep 問答要求 `return_context=True` |
| `llmwiki.llm` | `chat`, `chat_with_meta`, `extract_json`, `reasoning_budget` | `llm_safe.py`、patches、wenguan |
| `llmwiki.sections` | `parse_sections` | 原典 `/sections` API；返巢狀 sections 及 Python Unicode scalar offsets |
| `llmwiki.atomic` | `atomic_write_json` | 四部分類持久化 |
| `llmwiki.wikisource` | `READING_LISTS`, `fetch_subpages`, `_ingest_page` | `siwen/learn.py`；格式/抓取插件邊界仍须可用 |
| `llmwiki.chunk_cache` | `ChunkCache(base, subdir="chunks")`、`get/put` | wenguan 內容指紋續跑 |
| `llmwiki.split` | `split_by_heading` | wenguan 分段，不能換成啟發式標題猜測 |
| `llmwiki.normalize` | `HeadRule`, `normalize_heads`, `normalize_paragraphs` | 太虛 rule pack、差分稽核 |
| `llmwiki.web` | `create_web_app`, `require_auth`, `EXTRA_ROUTES` | Flask 擴展；不可換成僅同名 ASGI wrapper |
| `llmwiki.worker` | `register_learn_source` | 用 `wikisource` 同名覆內建 handler，handler 回 `result["ingested"]` |
| `llmwiki.mcp_server` | async `main` | `siwen/mcp_server.py` 先 patches 再 `asyncio.run(main())` |

原典根不能硬寫 `raw/`。Siwen Web `_get_raw_dir` 經 `current_app.config["llmbase"]["base_dir"]` 重新 `load_config`；外庫 `schools` 應從各自資料根讀。aliases 外庫入口帶完整 URL，本庫 key 優先，不能以同名 slug 無聲越庫。

### compile fork 之實情

`siwen/compile.py` 完整掌 prompt、parse、寫入與 merge，並非薄 wrapper：

- `_find_uncompiled` 取原典；`_list_existing_concepts` 供消歧義；空正文才讀同目錄 `.txt/.md` 附件，略 symlink。
- 預設 batch 10、max_content_chars 15000；model/max_tokens 從 compile config 覆 llm config。
- `safe_chat` 要 `===終===` 或 `===無可立條===`，中文比例及重試；失敗原典留未編。
- 來源 metadata 延 `plugin/url/title/work_id/canon/work/chapter/book`。
- `===無可立條===` 仍標 `compiled=True`、`compiled_at`、`skip_reason`，且此路不生概念；不能把它混作模型錯格式成功。
- 自家 parser 用 `===條目===`、`===續寫===`；CJK slug 方可收，dedup 依 slug、alias、CJK 題名。
- merge 新文長過舊文 1.2 倍才替換；否則若漢字重疊少於 50% 則 append；source 去重；未知 metadata 隨 existing post 留存。
- 直接 `frontmatter`／`Path.write_text` 寫概念與原典 compiled metadata，末後才呼上游 `rebuild_index`，再呼自家四部分類。

相容策略須把此 fork 明列 Python 擴展。其成功僅證擴展仍可用，不能宣稱 Python 所作 raw 狀態寫入已轉 Rust。Rust native compile、核心原典版本與索引整合仍須獨立驗證；若 facade 只在末端 `rebuild_index` 打一個 native marker，尚未滿全鏈路接管要求。新核心不得回寫原文正文；metadata 舊寫入副作用須映射並追蹤。

## 四、問答預設與類型

| 入口 | deep | file_back | promote | tone | 返值／注意 |
| --- | --- | --- | --- | --- | --- |
| `llmwiki.query.query` | — | false | — | `default` | 默 `str`；`return_path=True` 為 `{answer, output_path}` |
| `query_with_search` | 固定檢索 | false | false | `default` | 默 `str`；`return_context=True` 為 dict；promote 僅在 context 模式生效 |
| `KnowledgeBase.ask` | false | true | false | `default` | `{status:"ok", answer,...}` |
| Web `/api/ask` | false | true | false | `default` | 與 Agent API 兩 factory 分開盤點；不可統合時換 envelope |
| Agent HTTP `/api/ask` | false | true | false | `default` | 委 operation dispatch；維持 API 類 envelope |
| `SiwenKB.ask` | true | true | true | signature `wenyan` | 忽略呼入 tone，實際強制 `wenyan`；deep 傳 `return_context=True` |
| 新 native 問答 | 明確 DTO | false | false | profile | `write_policy="none"`；保存／compile／promote 要顯式選擇 |

Siwen deep exception 目前轉成 `status:"ok"` 並附文言錯答與 `promotion.promoted=false`；這是下游舊行為，native 錯誤型別仍應準確。deep/shallow 皆剝 `<think>…</think>`。其 `compile(full=False)` 回 `{status, articles_created, articles}`。`get_xici/generate_xici` 無論傳何 lang 皆轉 `zh`。

## 五、Flask 必須仍為 Flask

`create_siwen_web_app` 調 `llmwiki.web.create_web_app(base_dir)` 後：

1. 改 `app.json.ensure_ascii=False`（舊 Flask 用 `JSON_AS_ASCII`）。
2. 不論有無 frontend build，覆 `app.view_functions["api_compile"]`、`["api_branding"]`。
3. 遍 `app.url_map.iter_rules()`，按 URL 與 POST 定 feature gates，再改 `app.view_functions[endpoint]`。
4. 用 `app.config["llmbase"]` 的 `base_dir/cfg/api_secret/session_token`；compile handler 優先取當前 config 中 base_dir。
5. 若 static build 在，覆 `serve_favicon`、`serve_spa`，及 `api_aliases`；SPA 用 session token 設 `llmbase_auth` cookie。
6. EXTRAs 於 factory 前註冊，包含 classics、sections、outputs、schools、masters、books 等下游路由；既有路由不可遭 Rust catch-all 遮住。
7. `bake.main` 設 `app.testing=True`，`app.test_client().get(...)`，全程無需真 HTTP listener。

須分開測 Flask extension 生效与核心落 Rust。API route 包裝、before/after hooks、`require_auth` 讀當前 runtime config 均需保留。舊訪客 cookie 權限寬鬆不應被 native 授權邊界照抄；身份／庫別／付費寫入的安全要求仍為目標必達項，差異須明述而不能用相容之名掩過。

候選 v10 起的明示安全修正見 [Python HTTP auth](python-http-auth.md)：訪客 cookie 不再授權 promotion、模型或寫入；空 secret 亦須明示 local opt-in。三個標準 Flask 搜尋入口均先以 public 範圍取文件，再真呼 tokenizer 及 Rust scorer。下游自行覆寫的 route/模型函式仍為可信擴展，不得把該函式所做之事一概記作 Rust 核心。

## 六、bake 公開輸出之界

以當前未提交之 `siwen/bake.py` 為準。固定 GET 路由：

`branding, languages, stats, collections, aliases, entities, tones, healthz, articles, articles/lite, sources, trails, classics, kepan, masters, lineages, schools, persons, books, outputs`，前皆 `/api/`；另 `taxonomy?lang=zh`、`xici?lang=zh`。

列表 envelope 容許如下：articles 用 `articles/items`；sources 用 `documents/sources/items`；classics 用 `books/classics/items`；masters 用 `masters/items`；schools 用 `schools/items`；persons 用 `people/persons/items`；books 用 `books/items`；outputs 用 `outputs/items`；裸陣列亦容。枚舉 id 後烘詳頁、章節及 book chapter；article/classic sections 與 classic masters/kepan 的 404 為 optional。

外庫 school 有 `external_url` 者只作入口，不回請本庫詳頁，不烘作本庫資料。

`BakeRun.write` 對 JSON、HTML 內 inline JSON 再清理。遞迴刪內部鍵（含 `source_note/authority_note/source_level/evidence_level/curation_priority/ingest_strategy/compiled/compiled_at/usage_counts/limits/source_path/xml_path` 等）、`baizi_/simpfix_/split_/dry_run_/vN_/archive_/candidate_/wenguan_` 前綴；registry 的 `note` 另去。來源 URL 可公開，伺服器路徑、token-like 值不可公開。最後 `_verify_clean` 掃所有輸出；非法 JSON、可疑 NUL/編碼、禁發路徑皆 exit 2。

禁發路徑前綴：`api/master-works/audit`、`api/master-sources`、`masters/audit`、`masters/sources`。

**bake exit 0 不足驗收。** 單端點非 200 或 exception 僅進 `run.skipped`，主程式仍 return 0。候選驗收須另 assert 必需固定路由皆 200、預期列表與章節非空、`skipped` 僅具明定 allowlist；再核 public output 無 internal/private 新欄位。静態可讀不代表動態 ask 已部署。

既有 `tests/test_bake_contract.py` 為尚未追蹤用戶測試，涵蓋禁發前綴及內鍵清理；副本亦須收錄，不能僅 `git archive HEAD` 而漏掉。

## 七、wenguan 續跑與原字

`wenguan_book.py` 用 `split_by_heading`，整合後硬切保字；`reasoning_budget` 決 chunk 上限。調 `chat_with_meta(prompt, model, max_tokens, api_key)` 後，消費 `LLMMeta.finish_reason/truncated/usage["reasoning_tokens"]/attempts`；meta 為物件而非 dict。截斷視失敗；外層可重試，不能因有文字便成功。`chat` 仍須只返 str。

cache key 是 `cache_cid=f"{prompt_name}:{cid}"` 配 `sha256(chunk_utf8)`。cache disk layout：`<out>/chunks/<sha256(cid)>/<sha256(content_hash)>.md`。`fallback-original` 另 key，force/reuse_cache_on_force 語義有測試。改 chunk 邊界或內容後同 cid 必 miss；既有成功 cache 不得語言遷移即拋棄。

raw 與 `OUT_DIR` 整形稿分開，結果保原 metadata 並添 `wenguan_*`。有 fail/fallback 時可寫 staging output，但回 false；不能稱整卷成功。同步另經 `sync_validated_wenguan.py` 保字驗後執行。此批處理屬 Python 格式插件；通用 job/lease/cache/狀態由 Rust 接管之後仍需可讀舊續跑資料。

## 八、riddle 協議與本輪邊界

只讀 `/Users/lanchair/Developer/Hackathon/Github/riddle/src/oracle.rs`、`src/memory.rs` 及 README；不改設備、不燒入、不改 persona/font/display。

當前輸入為 PNG，HTTP oracle 送 image data-URI 與 user text 至 `/chat/completions`，`stream=true`；帶近期 `(transcript, reply)` history 及每回合新 catalog。`TurnContext` 只有 `history/catalog_lines/catalog_ids`。無 library/session/evidence/cancel 契約。

SSE 只認 `data:` 與 `choices[0].delta.content`、`[DONE]`；累積全文交 `StreamParser`。輸出 `Ink(String)` 逐句、`Show(u64)` 記憶頁、`Transcript(String)`。`⟦show:N⟧` 是設備記憶指令，`⁂` 後為轉錄；此兩種私有文法不得滲入通用知識問答正文。已有 connect 10 秒、read 90 秒 timeout；沒有顯式 provider cancel，也無結構化 citation。README 對 HTTP history、是否保存 reply 有舊述，判實際協議以源码為準。

本輪提供 `tests/rust_migration/fixtures/downstream/transcription.json`，純合成輸入。映射如下：

| 轉錄欄 | 型別 | 問答欄／語義 |
| --- | --- | --- |
| `version` | integer，現 1 | 轉錄協議版本 |
| `transcription_id` | string | 可追溯輸入 id，非 source id |
| `input_kind` | string | `synthetic_transcription`，不可聲稱實際 OCR 驗過 |
| `text` | UTF-8 string | `question` 原樣傳遞 |
| `library_id` | string | 必帶 scope、必校權限 |
| `session_id` | string | 問答會話 scope |
| `answer_preferences.short_answer` | boolean | `short_answer` |
| `answer_preferences.max_sentences` | positive integer | `max_sentences`，此例 2 |
| — | enum | `write_policy="none"` |

`expected_question` 固定映射，`expected_evidence` 另存預期原典 path/quote/URL/chapter，不供生成模型自判。source id、revision/hash、passage id 與原文 span 須由核心索引產生且解析回此原句；其非轉錄所能造。待 native 入口接通後，須實測整個 fixture 到問答與 citation 回查。對真正圖像輸入尚無 adapter 時，明報能力錯誤，不回假 OCR。

## 九、已加之固定契約測試與初跑

`tests/rust_migration/test_downstream_contracts.py` 共 11 項，fixture 具 custom roots、文言裸正文、未知巢狀欄、原典 URL/章、非 BMP 與組合字。

```text
/Users/lanchair/Developer/LLM-infra/Github/llmbase/.venv/bin/python -B -m pytest -q -p no:cacheprovider tests/rust_migration/test_downstream_contracts.py
11 passed in 1.43s
exit 0
```

當次實載 `/Users/lanchair/Developer/LLM-infra/Github/llmbase-rust/llmwiki/__init__.py`，該 interpreter metadata 仍 `0.8.0`。此證固定契約可在現上游 Python 實作通過，**不證候選 wheel 或 Rust facade 已成**。首寫測試曾把 `LLMMeta.truncated` 誤作自動 property，初跑 10 pass/1 fail；查明 dataclass 需顯式 `model/truncated`，改測真正 `chat_with_meta` 回值後全過。非待測碼回歸，未放寬截斷要求。

所覆：runtime prompt/dict/SECTION_HEADERS，factory 後 language override，identity anchor，private helper 型別/custom path，舊 ask 自動 file_back，taxonomy 真 callback 及換代，Flask registry/hooks/url_map/config/test_client，讀 API envelope、raw byte 不變，Unicode scalar offsets，舊 chunk cache 磁碟續讀，LLM 截斷 metadata。

下游既有測試已按函式名盤點，其覆蓋須在候選上保留：`test_compile.py` 驗 CJK slug、文言記號、source dedup、原典 symlink、no-substance、模型失敗不標成功；`test_taxonomy.py` 驗四部與 locked；`test_llm_safe.py` 驗空文重試、token 擴大、validator 與 JSON；`test_wenguan_chunk_book.py`、`test_wenguan_hard_split.py` 驗章序與硬切；`test_wenguan_chunk_cache.py` 驗 cid/hash cache；`test_masters_api.py` 驗資料根 schools 與來源反查；`test_sister_schools.py` 驗外庫入口不烘成本庫；`test_cli.py` 驗英／文雙命名；`test_bake_contract.py` 驗全公開樹與禁發路由。此為盤點，不作已跑聲稱。

## 十、候選 wheel 實驗與後續差異

2026-09-20 候選 v7 已補下列實驗，機讀證據見
[siwen-candidate-v7.json](results/siwen-candidate-v7.json) 與
[siwen-snapshot-baseline.json](results/siwen-snapshot-baseline.json)：

- 複製當前 Siwen 工作樹 87 個程式／測試／套件資料檔，逐檔 hash 相符；另複製既有公開 `static/dist`。原 `.venv` 未動，原 87 檔與 `git status` 驗後均同。合成文庫、烘焙產物及 interpreter 全在遷移倉 `.rust-migration-runs`。
- 隔離安裝真 wheel `llmwiki==0.9.0`，SHA-256 `d489a46f94d1d9a0f4eb48170197f7f5722ed57af721ee1ef49ed456a97f79c9`；實載 `_core.abi3.so` SHA-256 `9de6fece1fae6f7c4be076fe39ba2d43f41618da42ae28cfbba0e2f5dc37b578`。Siwen 仍 `0.1.0`，本次未發版。
- Siwen 既有 compile、patches、prompts、taxonomy、llm_safe、wenguan cache／split、masters API、sister schools、bake 契約測試共 89 項，全過（9.02 秒）。上游候選全套另 544 項全過。
- 真 `_find_uncompiled`、`_list_existing_concepts`、`rebuild_index` 均有實際 Rust receipt。動態換 taxonomy callable 兩次，皆真執行並回新樹；動態換 query prompt 兩次，SiwenKB 真呼當前模型回呼，保文言與剝思維標籤。此處用明示離線 Python 模型回呼，禁止外網，未冒稱供應商模型或完整 native query 已驗。
- 真 Flask `url_map`、`view_functions`、`config`、`after_request` 與新增 route 可用；22 個 bake 必需 GET 端點逐一 assert HTTP 200；`/api/sources` 有 Rust 讀取 receipt。
- 含 4 條目、原典與一完整書章之文庫，以原 Siwen BakeRun 烘得 133 JSON、112 HTML；`skipped=[]`，`_verify_clean` 通過，原典 bytes hash 未變。不是以空假 client 或 bake exit 0 充數。

重跑入口為 `scripts/rust-migration/prove_siwen_candidate.py`，須用已裝候選之隔離 interpreter、當前樹副本與新 run root。首次自寫 proof 把 private helper 傳入 cfg 而非 Path，次次 taxonomy fixture 缺 `id`；皆修正驗證腳本後重跑，未改 Siwen 或放寬 assertion。

界線仍明：Siwen 自家 compile 原典與概念寫入、四部分類、Flask 路由與靜態烘焙仍屬 Python 擴展；v7 query facade 尚待後續 Rust 主導流程之驗證。上述 native receipt 僅證其具名操作，不能將整個下游 fork 算作已 Rust 化。下列原驗收步驟供後續候選沿用。

**v9 復驗**：上述 Siwen 89 項於 v9 全過（9.42 秒）；實際 Flask／bake 再過，仍為 133 JSON、112 HTML、`skipped=[]`，原 87 檔、git status 與合成原典 bytes 悉未變。新 [Siwen receipt](results/siwen-candidate-v9.json) 已具 `query_last.orchestrator="rust"`、`stages=["index","selector","answer"]`：SiwenKB 的真 deep 問答流程入 Rust，仍真呼逐次更換的 Python prompt／model callback。外網維持禁止，非真模型品質評分。

v9 上游全套另 552 項全過（7.71 秒），含原始 query tests、真 fixture HTTP 問答／升條、失敗回復、Python callback 返回 None 不暗觸寫入，以及 JSONL `NaN`／正負 Infinity 相容。候選 wheel SHA-256 `9d97f0edf6bb6e338f49c636f6a5108cff1be6d22851166f4b26cbc16b4eb6f1`，extension SHA-256 `a80386c9940076e3ef022e35b2cf4a57a2c1cf8cd7d01eed99fdd2a30e426579`；[安裝與執行 receipt](results/python-native-candidate-v9.json) 保留實載路徑、12 個 facade 模組 source hash 與真 native 呼叫。v8 曾 550 過／2 敗：一為新 judge 誤將 `(text, LLMMeta)` 作單物件，另一為 wheel 尚未含 None-returning callback 修正；修後 v9 完整重建而過，未用 source shadow 規避。

**v10 安全修正復驗**：上游 installed wheel 569 項全過（7.57 秒；5 個 PyMuPDF deprecation warnings），隔離 Siwen 89 項全過（9.15 秒）。[v10 receipt](results/python-native-candidate-v10.json) 記 15 個 facade 的源／安裝 hash 相符。fetch、URL 原件 revision 與 Flask scope/public-search 新測試均在此套。獨立 bake proof 則實敗於 `/api/trails` HTTP 401：新權限規則禁止匿名讀研究歷程，舊固定 manifest 無條件請求。當次 fixture 無 `trails.json`；後續候選只准 absent／真空 trail 回公開空列表，非空舊 trail 仍須 private，待重建驗證。不能以本輪 pytest 全過宣稱 bake 全過。

**v11 修復驗收**：Rust trail read 僅對 absent／真空列表開放，非空仍須 private；全套 569 項過（8.65 秒），Siwen 89 項過（9.10 秒）。獨立 bake 的 22 必需 GET 全 200，再得 133 JSON、112 HTML、`skipped=[]`、clean，原 87 檔/status/raw bytes 未變。[v11 安裝 receipt](results/python-native-candidate-v11.json) 與 [Siwen receipt](results/siwen-candidate-v11.json) 已保存。此輪未更改 Siwen factory 或向 bake 加凭證；測試曾實敗之紀錄亦保留。

1. **複本**：保留原下游 status/hash，`rsync` 複製 Siwen 代碼、tests、必要公開 templates/static fixture；包含未 tracked bake test。略 `.venv`、生產/私密資料、`.env`，不要由 HEAD archive 偽代當前樹。
2. **隔離安裝**：新 venv 中裝候選 wheel；用 `pip install --no-deps <siwen-copy>` 保候選不被依賴解析換走。測試 cwd 在副本，無上游 source 的 `PYTHONPATH`／editable 汙染。
3. **載入證據**：記 Python executable、wheel SHA-256、installed distribution version/files、`llmwiki.__file__` 與 native extension／binary 路徑、native build id/hash。重覆舊 `.venv` pytest 不算候選驗證。
4. **下游真測試**：至少 patches、prompts、compile、taxonomy、llm_safe、learn、wenguan chunk/cache/hard split、masters API、sister schools、bake contract；均以 temporary fixture，模型/網路用離線 adapter，不能 import scripts 就碰遠端 raw。
5. **動態擴展**：import smoke 要含 `kb/compile/web/mcp_server/wenguan_book`。用真正 Siwen factory 驗 custom routes、middleware、feature gates、compile replacement、runtime base_dir、Flask test_client 與 bake 必需清單；只用本檔模仿不足。
6. **逐路徑 Rust 證據**：對下表每條收 `request_id/library_id/operation/input digest/result digest/native build id`，並在同一請求證可觀察 native 效果；不記 API key/完整私密 prompt。能設定明確 legacy core failure trap 者，啟用 trap，防隱性回退。trace 應由 Rust application service 真事件產生，Python 自寫 marker 不算。
7. **不變核查**：再核原 Siwen/riddle HEAD/status/關鍵檔 SHA-256；原 fixture raw byte hashes；所有程序收束，沒有本機 listener 留存。

| 必達核心路徑 | 要看的 native 證據 | Python 邊界另證 |
| --- | --- | --- |
| storage/source get | 原文 hash、revision、library scope 由 Rust 返回；改庫名/越庫被拒 | Flask 只轉 response，格式插件可 parse |
| search/index | index generation/count/實際 matched passages 與 Rust trace 同 request | tokenizer callback 真執行後結果回核心 |
| query | 檢索 evidence → provider delta → usage/done 全過 Rust service；no-write 預設明驗 | 當前 prompt/tone、chat override 由 plugin host 呼回 |
| compile | native draft、驗證、atomic apply、revision、來源 metadata、hooks | Siwen fork 自寫部分逐一標明，不能只拿 rebuild trace 代全編纂 |
| tasks | queued/running/terminal、lease/重启/取消從 Rust 持久冊可回讀 | custom job/learn handler 是真 callback，不是成功空殼 |
| bake | read requests 對應上述 Rust get/list，結果真的烘為可讀 JSON/HTML | Siwen 標準 test_client、欄位清理及 forbidden output scan |

上述第十節未有候選／native 跑證前，遷移相容狀態只能稱「舊契約已固定，候選驗收待成」。
