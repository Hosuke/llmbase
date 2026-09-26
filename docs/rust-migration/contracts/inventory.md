# 源碼抽取之完整契約清冊

由 `collect_contracts.py` 以 AST 抽取；未 import 舊 runtime，未觸文庫。每列遷移狀態皆 pending。分類描述目標責任，不證明實作。函式表保留簽名、型別、defaults；回傳與副作用以 parity.md 及源碼為準。

測試欄為同模組既有回歸測試起點，並非逐符號覆蓋證明；須逐案檢查 assertion 並補 native 對照測試。全部 CLI 仍缺獨立相容快照，全部 MCP 仍缺 stdio 協議全鏈測試。

基線 HEAD：`a7c4d9da764462e09c0769ecd8ba548a46d27e83`。

## CLI

| 命令 | 參數／預設（源碼） | 分類／理由 | 測試 | 狀態 | 來源 |
|---|---|---|---|---|---|
| `llmbase` (group) | ['--base-dir']: {'type': 'click.Path(exists=True)', 'default': '.', 'help': 'Project base directory'}; ['-v', '--verbose']: {'count': True, 'help': 'Increase log verbosity (-v INFO, -vv DEBUG+http, -vvv wire).'} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:51](../../../llmwiki/cli.py#L51) |
| `llmbase ingest` (group) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:62](../../../llmwiki/cli.py#L62) |
| `llmbase ingest url` (command) | ['url']: {} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:70](../../../llmwiki/cli.py#L70) |
| `llmbase ingest file` (command) | ['file_path']: {'type': 'click.Path(exists=True)'} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:81](../../../llmwiki/cli.py#L81) |
| `llmbase ingest pdf` (command) | ['pdf_path']: {'type': 'click.Path(exists=True)'}; ['--chunk-pages']: {'type': 'int', 'default': 20, 'help': 'Pages per chunk (0 = single doc)'} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:92](../../../llmwiki/cli.py#L92) |
| `llmbase ingest dir` (command) | ['dir_path']: {'type': 'click.Path(exists=True)'} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:105](../../../llmwiki/cli.py#L105) |
| `llmbase ingest browse` (command) | ['url']: {} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:116](../../../llmwiki/cli.py#L116) |
| `llmbase ingest wikisource-learn` (command) | ['--list', 'reading_list']: {'type': "click.Choice(['confucianism', 'daoism', 'mohism', 'legalism', 'military', 'history', 'poetry', 'divination', 'zhuzi'])", 'default': None}; ['--batch']: {'type': 'int', 'default': 3, 'help': 'Number of works per run'} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:162](../../../llmwiki/cli.py#L162) |
| `llmbase ingest wikisource-work` (command) | ['title']: {} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:177](../../../llmwiki/cli.py#L177) |
| `llmbase ingest cbeta-learn` (command) | ['--category']: {'type': 'str', 'default': None, 'help': 'Category: agama, bore, fahua, huayan, chanzong, jingtu, etc.'}; ['--batch']: {'type': 'int', 'default': 5, 'help': 'Number of sutras per run'} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:189](../../../llmwiki/cli.py#L189) |
| `llmbase ingest cbeta-status` (command) | 無額外參數；Click --help | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:209](../../../llmwiki/cli.py#L209) |
| `llmbase ingest cbeta-work` (command) | ['work_id']: {} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:223](../../../llmwiki/cli.py#L223) |
| `llmbase ingest ctext-book` (command) | ['book_name']: {}; ['book_path']: {}; ['--delay']: {'type': 'float', 'default': 1.5, 'help': 'Delay between requests (seconds)'}; ['--browser']: {'is_flag': True, 'help': 'Use opencli browser instead of HTTP (handles anti-scraping)'} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:240](../../../llmwiki/cli.py#L240) |
| `llmbase ingest ctext-catalog` (command) | ['catalog']: {'type': "click.Choice(['confucianism', 'daoism', 'mohism', 'legalism', 'military', 'histories', 'medicine'])"}; ['--delay']: {'type': 'float', 'default': 1.5, 'help': 'Delay between requests (seconds)'}; ['--browser']: {'is_flag': True, 'help': 'Use opencli browser instead of HTTP'} | Python 擴展邊界（connector 可插件；CLI/儲存原生） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:254](../../../llmwiki/cli.py#L254) |
| `llmbase ingest list` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:268](../../../llmwiki/cli.py#L268) |
| `llmbase compile` (group) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:292](../../../llmwiki/cli.py#L292) |
| `llmbase compile new` (command) | ['--batch-size']: {'type': 'int', 'default': None, 'help': 'Max documents to process'} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:300](../../../llmwiki/cli.py#L300) |
| `llmbase compile all` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:315](../../../llmwiki/cli.py#L315) |
| `llmbase compile index` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:325](../../../llmwiki/cli.py#L325) |
| `llmbase query` (command) | ['question']: {}; ['--format', 'output_format']: {'type': "click.Choice(['markdown', 'marp', 'chart'])", 'default': 'markdown'}; ['--tone']: {'type': "click.Choice(['default', 'caveman', 'wenyan', 'scholar', 'eli5'])", 'default': 'default', 'help': 'Response tone: caveman (primitive), wenyan (文言文), scholar (academic), eli5 (simple)'}; ['--file-back']: {'is_flag': True, 'help': 'Save the answer back into the wiki'}; ['--deep']: {'is_flag': True, 'help': 'Use multi-step search for complex queries'} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:342](../../../llmwiki/cli.py#L342) |
| `llmbase search` (group) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:361](../../../llmwiki/cli.py#L361) |
| `llmbase search query` (command) | ['query_text']: {}; ['--top-k']: {'type': 'int', 'default': 10}; ['--json-output']: {'is_flag': True, 'help': 'Output as JSON (for LLM tool use)'} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:371](../../../llmwiki/cli.py#L371) |
| `llmbase search serve` (command) | ['--port']: {'type': 'int', 'default': None} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:387](../../../llmwiki/cli.py#L387) |
| `llmbase lint` (group) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:403](../../../llmwiki/cli.py#L403) |
| `llmbase lint check` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:410](../../../llmwiki/cli.py#L410) |
| `llmbase lint deep` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:433](../../../llmwiki/cli.py#L433) |
| `llmbase lint fix` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:445](../../../llmwiki/cli.py#L445) |
| `llmbase lint normalize-tags` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:461](../../../llmwiki/cli.py#L461) |
| `llmbase lint clean` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:481](../../../llmwiki/cli.py#L481) |
| `llmbase lint dedup` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:502](../../../llmwiki/cli.py#L502) |
| `llmbase lint heal` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:530](../../../llmwiki/cli.py#L530) |
| `llmbase stats` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:591](../../../llmwiki/cli.py#L591) |
| `llmbase serve` (command) | ['--port']: {'type': 'int', 'default': 5556} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:625](../../../llmwiki/cli.py#L625) |
| `llmbase web` (command) | ['--port']: {'type': 'int', 'default': 5555} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:640](../../../llmwiki/cli.py#L640) |
| `llmbase mcp` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:651](../../../llmwiki/cli.py#L651) |
| `llmbase export` (group) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:661](../../../llmwiki/cli.py#L661) |
| `llmbase export article` (command) | ['slug']: {} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:669](../../../llmwiki/cli.py#L669) |
| `llmbase export tag` (command) | ['tag']: {} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:683](../../../llmwiki/cli.py#L683) |
| `llmbase export graph` (command) | ['slug']: {}; ['--depth']: {'type': 'int', 'default': 2} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:694](../../../llmwiki/cli.py#L694) |
| `llmbase ops` (group) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:704](../../../llmwiki/cli.py#L704) |
| `llmbase ops list` (command) | 無額外參數；Click --help | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:711](../../../llmwiki/cli.py#L711) |
| `llmbase ops call` (command) | ['name']: {}; ['--json-args', 'json_args']: {'default': '{}', 'help': 'JSON-encoded arguments'} | Rust 原生（通用能力） | CLI 契約測試缺口 | pending | [llmwiki/cli.py:728](../../../llmwiki/cli.py#L728) |

## HTTP（包括第三個 legacy search factory）

Flask factory 為 Python 擴展邊界，通用路由底層皆須 Rust 原生；connector 處理可用明示插件。False 僅表無 require_auth decorator，非斷言整條路由無權限邏輯。

| Factory | 方法／路徑 | Endpoint | require_auth decorator | 分類／理由 | 測試起點 | 狀態 | 來源 |
|---|---|---|---|---|---|---|---|
| `llmwiki.agent_api` | `POST /api/ingest` | `api_ingest` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:188](../../../llmwiki/agent_api.py#L188) |
| `llmwiki.agent_api` | `POST /api/compile` | `api_compile` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:193](../../../llmwiki/agent_api.py#L193) |
| `llmwiki.agent_api` | `POST /api/ask` | `api_ask` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:198](../../../llmwiki/agent_api.py#L198) |
| `llmwiki.agent_api` | `GET /api/search` | `api_search` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:215](../../../llmwiki/agent_api.py#L215) |
| `llmwiki.agent_api` | `POST /api/lint` | `api_lint` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:221](../../../llmwiki/agent_api.py#L221) |
| `llmwiki.agent_api` | `POST /api/lint/fix` | `api_lint_fix` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:226](../../../llmwiki/agent_api.py#L226) |
| `llmwiki.agent_api` | `GET /api/health` | `api_health` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:230](../../../llmwiki/agent_api.py#L230) |
| `llmwiki.agent_api` | `GET /api/sources` | `api_sources` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:234](../../../llmwiki/agent_api.py#L234) |
| `llmwiki.agent_api` | `GET /api/articles` | `api_articles` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:238](../../../llmwiki/agent_api.py#L238) |
| `llmwiki.agent_api` | `GET /api/articles/<slug>` | `api_article` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:242](../../../llmwiki/agent_api.py#L242) |
| `llmwiki.agent_api` | `POST /api/index/rebuild` | `api_rebuild_index` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:246](../../../llmwiki/agent_api.py#L246) |
| `llmwiki.agent_api` | `POST /api/op/<name>` | `api_op` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:252](../../../llmwiki/agent_api.py#L252) |
| `llmwiki.agent_api` | `GET /api/op` | `api_op_list` | False | Rust 原生（通用 route；Flask 轉譯） | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:267](../../../llmwiki/agent_api.py#L267) |
| `llmwiki.search` | `GET /` | `index` | False | Python 擴展邊界（Flask 外殼／静態站） | [test_search_raw](../../../tests/test_search_raw.py) | pending | [llmwiki/search.py:294](../../../llmwiki/search.py#L294) |
| `llmwiki.search` | `GET /api/search` | `api_search` | False | Rust 原生（通用 route；Flask 轉譯） | [test_search_raw](../../../tests/test_search_raw.py) | pending | [llmwiki/search.py:298](../../../llmwiki/search.py#L298) |
| `llmwiki.web` | `GET /api/healthz` | `api_healthz` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:293](../../../llmwiki/web.py#L293) |
| `llmwiki.web` | `GET /api/branding` | `api_branding` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:304](../../../llmwiki/web.py#L304) |
| `llmwiki.web` | `GET /api/languages` | `api_languages` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:318](../../../llmwiki/web.py#L318) |
| `llmwiki.web` | `GET /api/stats` | `api_stats` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:328](../../../llmwiki/web.py#L328) |
| `llmwiki.web` | `GET /api/taxonomy` | `api_taxonomy` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:373](../../../llmwiki/web.py#L373) |
| `llmwiki.web` | `GET /api/collections` | `api_collections` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:405](../../../llmwiki/web.py#L405) |
| `llmwiki.web` | `GET /api/articles` | `api_articles` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:440](../../../llmwiki/web.py#L440) |
| `llmwiki.web` | `GET /api/articles/lite` | `api_articles_lite` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:557](../../../llmwiki/web.py#L557) |
| `llmwiki.web` | `GET /api/articles/<path:slug>/sections` | `api_article_sections` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:614](../../../llmwiki/web.py#L614) |
| `llmwiki.web` | `GET /api/articles/<path:slug>` | `api_article` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:642](../../../llmwiki/web.py#L642) |
| `llmwiki.web` | `GET /api/aliases` | `api_aliases` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:705](../../../llmwiki/web.py#L705) |
| `llmwiki.web` | `GET /api/export/article/<path:slug>` | `api_export_article` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:714](../../../llmwiki/web.py#L714) |
| `llmwiki.web` | `GET /api/export/tag/<tag>` | `api_export_tag` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:722](../../../llmwiki/web.py#L722) |
| `llmwiki.web` | `GET /api/export/graph/<path:slug>` | `api_export_graph` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:727](../../../llmwiki/web.py#L727) |
| `llmwiki.web` | `GET /api/entities` | `api_entities` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:736](../../../llmwiki/web.py#L736) |
| `llmwiki.web` | `POST /api/entities/extract` | `api_extract_entities` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:743](../../../llmwiki/web.py#L743) |
| `llmwiki.web` | `GET /api/refs/plugins` | `api_ref_plugins` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:750](../../../llmwiki/web.py#L750) |
| `llmwiki.web` | `GET /api/trails` | `api_trails` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:775](../../../llmwiki/web.py#L775) |
| `llmwiki.web` | `POST /api/trails` | `api_trails_save` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:784](../../../llmwiki/web.py#L784) |
| `llmwiki.web` | `POST /api/trails/<trail_id>/delete` | `api_trail_delete` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:826](../../../llmwiki/web.py#L826) |
| `llmwiki.web` | `GET /api/xici` | `api_xici` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:835](../../../llmwiki/web.py#L835) |
| `llmwiki.web` | `POST /api/xici/generate` | `api_xici_generate` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:847](../../../llmwiki/web.py#L847) |
| `llmwiki.web` | `GET /api/search` | `api_search` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:867](../../../llmwiki/web.py#L867) |
| `llmwiki.web` | `POST /api/ask` | `api_ask` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:914](../../../llmwiki/web.py#L914) |
| `llmwiki.web` | `GET /api/tones` | `api_tones` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1056](../../../llmwiki/web.py#L1056) |
| `llmwiki.web` | `GET /api/sources` | `api_sources` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1069](../../../llmwiki/web.py#L1069) |
| `llmwiki.web` | `GET /api/sources/<path:slug>` | `api_source_detail` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1074](../../../llmwiki/web.py#L1074) |
| `llmwiki.web` | `POST /api/ingest` | `api_ingest` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1103](../../../llmwiki/web.py#L1103) |
| `llmwiki.web` | `POST /api/upload` | `api_upload` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1114](../../../llmwiki/web.py#L1114) |
| `llmwiki.web` | `DELETE /api/articles/<slug>` | `api_delete_article` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1147](../../../llmwiki/web.py#L1147) |
| `llmwiki.web` | `POST /api/wiki/clean` | `api_clean_wiki` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1159](../../../llmwiki/web.py#L1159) |
| `llmwiki.web` | `POST /api/taxonomy/update` | `api_update_taxonomy` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1186](../../../llmwiki/web.py#L1186) |
| `llmwiki.web` | `GET /api/worker/status` | `api_worker_status` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1201](../../../llmwiki/web.py#L1201) |
| `llmwiki.web` | `POST /api/compile` | `api_compile` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1219](../../../llmwiki/web.py#L1219) |
| `llmwiki.web` | `POST /api/lint` | `api_lint` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1228](../../../llmwiki/web.py#L1228) |
| `llmwiki.web` | `POST /api/lint/fix` | `api_lint_fix` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1240](../../../llmwiki/web.py#L1240) |
| `llmwiki.web` | `GET /api/health` | `api_health` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1273](../../../llmwiki/web.py#L1273) |
| `llmwiki.web` | `GET /api/wiki/export` | `api_wiki_export` | False | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1284](../../../llmwiki/web.py#L1284) |
| `llmwiki.web` | `POST /api/index/rebuild` | `api_rebuild_index` | True | Rust 原生（通用 route；Flask 轉譯） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1300](../../../llmwiki/web.py#L1300) |
| `llmwiki.web` | `GET /favicon.svg` | `serve_favicon` | False | Python 擴展邊界（Flask 外殼／静態站） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1312](../../../llmwiki/web.py#L1312) |
| `llmwiki.web` | `GET /` | `serve_spa` | False | Python 擴展邊界（Flask 外殼／静態站） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1320](../../../llmwiki/web.py#L1320) |
| `llmwiki.web` | `GET /<path:path>` | `serve_spa` | False | Python 擴展邊界（Flask 外殼／静態站） | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:1320](../../../llmwiki/web.py#L1320) |

## MCP／operations

每項 `params` 之 JSON Schema（含 required、default、enum、writeOnly）完整見 baseline.json；此表非重寫 schema。19 項皆 Rust 原生核心能力，ingest 的網站解析可交明示插件。writes=False 不代表沒有 file_back／promote／lint fix 等副作用。

| 名稱 | Schema | writes／category | 分類 | 測試起點 | 狀態 | 來源 |
|---|---|---|---|---|---|---|
| `kb_search` | `{"type": "object", "properties": {"query": {"type": "string", "description": "Search query"}, "top_k": {"type": "integer", "default": 10}}, "required": ["query"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:370](../../../llmwiki/operations.py#L370) |
| `kb_search_raw` | `{"type": "object", "properties": {"query": {"type": "string", "description": "Search query"}, "top_k": {"type": "integer", "default": 10}}, "required": ["query"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:384](../../../llmwiki/operations.py#L384) |
| `kb_ask` | `{"type": "object", "properties": {"question": {"type": "string"}, "tone": {"type": "string", "default": "default"}, "file_back": {"type": "boolean", "default": false}, "deep": {"type": "boolean", "default": true}, "promote": {"type": "boolean", "default": false}, "model": {"type": "string"}, "api_key": {"type": "string", "writeOnly": true, "description": "Per-request LLM API key override. HTTP callers MUST use the X-LLM-Key header instead; the /api/ask body is rejected with 400 if any api_key-like field is present."}}, "required": ["question"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:403](../../../llmwiki/operations.py#L403) |
| `kb_get` | `{"type": "object", "properties": {"slug": {"type": "string"}, "section": {"type": "string", "description": "Section anchor from kb_get_sections (e.g. h2-緒論-bb6572)"}}, "required": ["slug"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:438](../../../llmwiki/operations.py#L438) |
| `kb_get_sections` | `{"type": "object", "properties": {"slug": {"type": "string"}}, "required": ["slug"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:456](../../../llmwiki/operations.py#L456) |
| `kb_list` | `{"type": "object", "properties": {"tag": {"type": "string"}}}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:471](../../../llmwiki/operations.py#L471) |
| `kb_backlinks` | `{"type": "object", "properties": {"slug": {"type": "string"}}, "required": ["slug"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:481](../../../llmwiki/operations.py#L481) |
| `kb_taxonomy` | `{"type": "object", "properties": {"lang": {"type": "string", "default": "zh"}}}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:492](../../../llmwiki/operations.py#L492) |
| `kb_stats` | `{"type": "object", "properties": {}}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:502](../../../llmwiki/operations.py#L502) |
| `kb_ingest` | `{"type": "object", "properties": {"source": {"type": "string", "description": "URL or local file path"}, "url": {"type": "string", "description": "Legacy alias for source"}}}` | True / write | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:509](../../../llmwiki/operations.py#L509) |
| `kb_compile` | `{"type": "object", "properties": {"full": {"type": "boolean", "default": false}}}` | True / write | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:523](../../../llmwiki/operations.py#L523) |
| `kb_lint` | `{"type": "object", "properties": {"deep": {"type": "boolean", "default": false}, "fix": {"type": "boolean", "default": false, "description": "Legacy: run auto_fix"}}}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:534](../../../llmwiki/operations.py#L534) |
| `kb_lint_fix` | `{"type": "object", "properties": {}}` | True / write | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:547](../../../llmwiki/operations.py#L547) |
| `kb_export_article` | `{"type": "object", "properties": {"slug": {"type": "string"}}, "required": ["slug"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:555](../../../llmwiki/operations.py#L555) |
| `kb_export_tag` | `{"type": "object", "properties": {"tag": {"type": "string"}}, "required": ["tag"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:566](../../../llmwiki/operations.py#L566) |
| `kb_export_graph` | `{"type": "object", "properties": {"slug": {"type": "string"}, "depth": {"type": "integer", "default": 2}}, "required": ["slug"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:577](../../../llmwiki/operations.py#L577) |
| `kb_export` | `{"type": "object", "properties": {"type": {"type": "string", "enum": ["article", "tag", "graph"]}, "slug": {"type": "string"}, "depth": {"type": "integer", "default": 2}}, "required": ["type", "slug"]}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:591](../../../llmwiki/operations.py#L591) |
| `kb_rebuild_index` | `{"type": "object", "properties": {}}` | True / write | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:606](../../../llmwiki/operations.py#L606) |
| `kb_xici` | `{"type": "object", "properties": {"lang": {"type": "string", "default": "zh"}}}` | False / read | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:614](../../../llmwiki/operations.py#L614) |

## Python 公開符號與已知私有消費者

此節各 symbol 均須按原簽名逐項驗證。`Python 擴展邊界` 的 facade 仍须將 storage/search/query/compile/jobs 轉入 Rust；非允許保留舊核心。傳輸與 callbacks 留 Python；connector parser 可留插件。新 native 實作驗證與現有測試對照見 parity.md。

| Symbol／signature | 契約摘要（原 docstring） | 分類／責任 | 測試起點 | 狀態 | 來源 |
|---|---|---|---|---|---|
| `llmwiki.agent_api.KnowledgeBase` | High-level API for agents to interact with the knowledge base. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:23](../../../llmwiki/agent_api.py#L23) |
| `llmwiki.agent_api.KnowledgeBase.__init__(self, base_dir: str \| Path \| None=None)` |  | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:26](../../../llmwiki/agent_api.py#L26) |
| `llmwiki.agent_api.KnowledgeBase.ingest(self, source: str)` | Ingest a URL or local file path. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:31](../../../llmwiki/agent_api.py#L31) |
| `llmwiki.agent_api.KnowledgeBase.compile(self, full: bool=False)` | Compile raw documents into wiki. full=True recompiles everything. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:39](../../../llmwiki/agent_api.py#L39) |
| `llmwiki.agent_api.KnowledgeBase.ask(self, question: str, deep: bool=False, file_back: bool=True, tone: str='default', promote: bool=False)` | Ask a question against the knowledge base. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:47](../../../llmwiki/agent_api.py#L47) |
| `llmwiki.agent_api.KnowledgeBase.search(self, query_text: str, top_k: int=10)` | Full-text search. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:77](../../../llmwiki/agent_api.py#L77) |
| `llmwiki.agent_api.KnowledgeBase.lint_check(self, deep_check: bool=False)` | Run health checks. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:82](../../../llmwiki/agent_api.py#L82) |
| `llmwiki.agent_api.KnowledgeBase.lint_fix(self)` | Run auto-fix on lint issues (metadata + broken links). | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:91](../../../llmwiki/agent_api.py#L91) |
| `llmwiki.agent_api.KnowledgeBase.health_report(self)` | Return the last persisted health report. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:96](../../../llmwiki/agent_api.py#L96) |
| `llmwiki.agent_api.KnowledgeBase.get_xici(self, lang: str='zh')` | Get the Xi Ci (guided introduction) for the knowledge base. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:105](../../../llmwiki/agent_api.py#L105) |
| `llmwiki.agent_api.KnowledgeBase.generate_xici(self, lang: str='zh')` | Regenerate the Xi Ci. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:110](../../../llmwiki/agent_api.py#L110) |
| `llmwiki.agent_api.KnowledgeBase.list_sources(self)` | List all ingested raw documents. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:116](../../../llmwiki/agent_api.py#L116) |
| `llmwiki.agent_api.KnowledgeBase.rebuild_index(self)` | Rebuild the wiki index. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:121](../../../llmwiki/agent_api.py#L121) |
| `llmwiki.agent_api.KnowledgeBase.get_article(self, slug: str)` | Read a specific wiki article by slug. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:126](../../../llmwiki/agent_api.py#L126) |
| `llmwiki.agent_api.KnowledgeBase.export_article(self, slug: str)` |  | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:143](../../../llmwiki/agent_api.py#L143) |
| `llmwiki.agent_api.KnowledgeBase.export_by_tag(self, tag: str)` |  | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:150](../../../llmwiki/agent_api.py#L150) |
| `llmwiki.agent_api.KnowledgeBase.export_graph(self, slug: str, depth: int=2)` |  | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:154](../../../llmwiki/agent_api.py#L154) |
| `llmwiki.agent_api.KnowledgeBase.list_articles(self)` | List all wiki articles with metadata. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:158](../../../llmwiki/agent_api.py#L158) |
| `llmwiki.agent_api.create_agent_server(base_dir: str \| Path \| None=None, port: int=5556)` | Create an HTTP API server for agent access. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/agent_api.py:175](../../../llmwiki/agent_api.py#L175) |
| `llmwiki.anchor.Anchor` | Located span in original content. | Rust 原生 | [test_anchor](../../../tests/test_anchor.py) | pending | [llmwiki/anchor.py:64](../../../llmwiki/anchor.py#L64) |
| `llmwiki.anchor.normalize_text(s: str, level: NormalizeLevel='punct_spaces')` | Normalize ``s`` for span matching. | Rust 原生 | [test_anchor](../../../tests/test_anchor.py) | pending | [llmwiki/anchor.py:78](../../../llmwiki/anchor.py#L78) |
| `llmwiki.anchor.locate_span(content: str, head: str, tail: str='', *, normalize: NormalizeLevel='punct_spaces')` | Locate the passage ``[head .. tail]`` within ``content``. | Rust 原生 | [test_anchor](../../../tests/test_anchor.py) | pending | [llmwiki/anchor.py:135](../../../llmwiki/anchor.py#L135) |
| `llmwiki.atomic.atomic_write_json(path: Path, data: dict, ensure_ascii: bool=False)` | Write JSON atomically using temp file + rename. | Rust 原生 | [test_atomic](../../../tests/test_atomic.py) | pending | [llmwiki/atomic.py:15](../../../llmwiki/atomic.py#L15) |
| `llmwiki.atomic.atomic_write_text(path: Path, content: str)` | Write UTF-8 text atomically using temp file + rename. | Rust 原生 | [test_atomic](../../../tests/test_atomic.py) | pending | [llmwiki/atomic.py:41](../../../llmwiki/atomic.py#L41) |
| `llmwiki.browser.is_opencli_available()` | Check if opencli is installed. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/browser.py:9](../../../llmwiki/browser.py#L9) |
| `llmwiki.browser.opencli_run(args: list[str], timeout: int=30)` | Run an opencli command and return output. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/browser.py:14](../../../llmwiki/browser.py#L14) |
| `llmwiki.browser.browse_url(url: str)` | Open a URL in the browser and get the page content. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/browser.py:23](../../../llmwiki/browser.py#L23) |
| `llmwiki.browser.screenshot(output_path: str \| None=None)` | Take a screenshot of the current browser page. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/browser.py:33](../../../llmwiki/browser.py#L33) |
| `llmwiki.browser.extract_text(url: str)` | Navigate to URL and extract text content. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/browser.py:41](../../../llmwiki/browser.py#L41) |
| `llmwiki.browser.search_web(query: str, site: str='hackernews', limit: int=10)` | Search a supported site via opencli. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/browser.py:51](../../../llmwiki/browser.py#L51) |
| `llmwiki.browser.fetch_article(url: str)` | Fetch a web article via opencli browser, returning title and content. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/browser.py:60](../../../llmwiki/browser.py#L60) |
| `llmwiki.cbeta.get_progress_file(base_dir: Path)` | Get path to the progress tracking file. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:54](../../../llmwiki/cbeta.py#L54) |
| `llmwiki.cbeta.load_progress(base_dir: Path)` | Load ingestion progress. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:61](../../../llmwiki/cbeta.py#L61) |
| `llmwiki.cbeta.save_progress(base_dir: Path, progress: dict)` | Save ingestion progress. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:69](../../../llmwiki/cbeta.py#L69) |
| `llmwiki.cbeta.list_categories()` | List all CBETA categories. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:77](../../../llmwiki/cbeta.py#L77) |
| `llmwiki.cbeta.list_works_in_category(category_id: str)` | Recursively list all individual works (sutras) in a category. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:86](../../../llmwiki/cbeta.py#L86) |
| `llmwiki.cbeta.fetch_sutra_xml(work_id: str)` | Fetch sutra XML from CBETA GitHub repository. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:122](../../../llmwiki/cbeta.py#L122) |
| `llmwiki.cbeta.ingest_work(work_id: str, label: str='', creator: str='', base_dir: Path \| None=None)` | Ingest a single CBETA work into the knowledge base. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:210](../../../llmwiki/cbeta.py#L210) |
| `llmwiki.cbeta.learn(category: str \| None=None, batch_size: int=5, base_dir: Path \| None=None)` | Progressive learning: ingest a batch of unread sutras. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:262](../../../llmwiki/cbeta.py#L262) |
| `llmwiki.cbeta.status(base_dir: Path \| None=None)` | Get current learning progress. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/cbeta.py:344](../../../llmwiki/cbeta.py#L344) |
| `llmwiki.chunk_cache.ChunkCache` | File-backed (cid, content_hash) → output cache. | Rust 原生 | [test_chunk_cache](../../../tests/test_chunk_cache.py) | pending | [llmwiki/chunk_cache.py:55](../../../llmwiki/chunk_cache.py#L55) |
| `llmwiki.chunk_cache.ChunkCache.__init__(self, base: Path \| str, subdir: str='.chunk_cache')` |  | Rust 原生 | [test_chunk_cache](../../../tests/test_chunk_cache.py) | pending | [llmwiki/chunk_cache.py:87](../../../llmwiki/chunk_cache.py#L87) |
| `llmwiki.chunk_cache.ChunkCache.root(self)` | On-disk cache directory (readable; subject to changes). | Rust 原生 | [test_chunk_cache](../../../tests/test_chunk_cache.py) | pending | [llmwiki/chunk_cache.py:91](../../../llmwiki/chunk_cache.py#L91) |
| `llmwiki.chunk_cache.ChunkCache.get(self, cid: str, content_hash: str)` | Return the cached output for ``(cid, content_hash)``, or | Rust 原生 | [test_chunk_cache](../../../tests/test_chunk_cache.py) | pending | [llmwiki/chunk_cache.py:95](../../../llmwiki/chunk_cache.py#L95) |
| `llmwiki.chunk_cache.ChunkCache.put(self, cid: str, content_hash: str, output: str)` | Store ``output`` under ``(cid, content_hash)``. Atomic write | Rust 原生 | [test_chunk_cache](../../../tests/test_chunk_cache.py) | pending | [llmwiki/chunk_cache.py:111](../../../llmwiki/chunk_cache.py#L111) |
| `llmwiki.chunk_cache.ChunkCache.clear(self, cid: str)` | Remove every entry stored for ``cid`` (across all historical | Rust 原生 | [test_chunk_cache](../../../tests/test_chunk_cache.py) | pending | [llmwiki/chunk_cache.py:121](../../../llmwiki/chunk_cache.py#L121) |
| `llmwiki.compile.sanitize_slug(slug: str)` | Normalize a concept slug to a safe filename stem. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:36](../../../llmwiki/compile.py#L36) |
| `llmwiki.compile.compile_new(base_dir: Path \| None=None, batch_size: int \| None=None)` | Compile unprocessed raw documents into wiki articles. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:174](../../../llmwiki/compile.py#L174) |
| `llmwiki.compile.compile_all(base_dir: Path \| None=None)` | Recompile everything - reset compiled flags and run. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:314](../../../llmwiki/compile.py#L314) |
| `llmwiki.compile.rebuild_index(base_dir: Path \| None=None)` | Rebuild the master index file from all wiki articles. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:337](../../../llmwiki/compile.py#L337) |
| `llmwiki.compile._find_uncompiled(raw_dir: Path)` | Find raw documents that haven't been compiled yet. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:397](../../../llmwiki/compile.py#L397) |
| `llmwiki.compile._list_existing_concepts(concepts_dir: Path)` | List existing concepts as 'slug (title)' for LLM disambiguation. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:428](../../../llmwiki/compile.py#L428) |
| `llmwiki.compile._parse_compile_response(response: str)` | Parse LLM response into article dicts. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:440](../../../llmwiki/compile.py#L440) |
| `llmwiki.compile._write_article(article: dict, concepts_dir: Path, headers: list[tuple[str, str]] \| None=None)` | Write or update an article file. Merges into existing articles. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:526](../../../llmwiki/compile.py#L526) |
| `llmwiki.compile._merge_into(existing_path: Path, article: dict, headers: list[tuple[str, str]] \| None=None)` | Merge new article content into an existing article (叠加进化). | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:598](../../../llmwiki/compile.py#L598) |
| `llmwiki.compile._split_sections(content: str, headers: list[tuple[str, str]] \| None=None)` | Split article into {section_key: content} dict. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:670](../../../llmwiki/compile.py#L670) |
| `llmwiki.compile._assemble_sections(sections: dict[str, str], headers: list[tuple[str, str]] \| None=None)` | Reassemble sections into a single content string. | Rust 原生 | [test_compile](../../../tests/test_compile.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/compile.py:716](../../../llmwiki/compile.py#L716) |
| `llmwiki.config.load_config(base_dir: Path \| None=None)` | Load config.yaml from the project root. | Rust 原生 | [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/config.py:10](../../../llmwiki/config.py#L10) |
| `llmwiki.config.ensure_dirs(cfg: dict)` | Create all configured directories if they don't exist. | Rust 原生 | [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/config.py:65](../../../llmwiki/config.py#L65) |
| `llmwiki.config.get_language_profile(cfg: dict \| None=None)` | Return the normalized language profile for this knowledge base. | Rust 原生 | [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/config.py:98](../../../llmwiki/config.py#L98) |
| `llmwiki.config.get_section_headers(cfg: dict \| None=None)` | Return compile-style section headers from the active language profile. | Rust 原生 | [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/config.py:111](../../../llmwiki/config.py#L111) |
| `llmwiki.ctext.fetch_text(url: str, use_browser: bool=False)` | Fetch page and extract classical text content from ctext.org. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/ctext.py:29](../../../llmwiki/ctext.py#L29) |
| `llmwiki.ctext.fetch_chapter_list(index_url: str)` | Fetch chapter list from a book's index page. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/ctext.py:67](../../../llmwiki/ctext.py#L67) |
| `llmwiki.ctext.fetch_book_list(catalog_url: str)` | Fetch list of books from a catalog page (e.g. /confucianism/zh). | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/ctext.py:89](../../../llmwiki/ctext.py#L89) |
| `llmwiki.ctext.ingest_chapter(book_name: str, chapter_name: str, chapter_url: str, base_dir: Path \| None=None, use_browser: bool=False)` | Ingest a single chapter from ctext.org. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/ctext.py:111](../../../llmwiki/ctext.py#L111) |
| `llmwiki.ctext.ingest_book(book_name: str, book_path: str, delay: float=1.5, base_dir: Path \| None=None, use_browser: bool=False)` | Ingest all chapters of a book from ctext.org. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/ctext.py:145](../../../llmwiki/ctext.py#L145) |
| `llmwiki.ctext.ingest_catalog(catalog: str, delay: float=1.5, base_dir: Path \| None=None, use_browser: bool=False)` | Ingest all books from a catalog (e.g. 'confucianism'). | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/ctext.py:177](../../../llmwiki/ctext.py#L177) |
| `llmwiki.entities.extract_entities(base_dir: Path \| None=None)` | Extract entities from wiki articles using LLM. Cache to entities.json. | Rust 原生 | [test_entities](../../../tests/test_entities.py) | pending | [llmwiki/entities.py:73](../../../llmwiki/entities.py#L73) |
| `llmwiki.entities._dedup_entities(entities: list[dict])` | Merge duplicate entities by name/name_local. | Rust 原生 | [test_entities](../../../tests/test_entities.py) | pending | [llmwiki/entities.py:167](../../../llmwiki/entities.py#L167) |
| `llmwiki.entities._save_entities(meta_dir: Path, result: dict)` | Write entities to cache file. | Rust 原生 | [test_entities](../../../tests/test_entities.py) | pending | [llmwiki/entities.py:250](../../../llmwiki/entities.py#L250) |
| `llmwiki.entities.get_entities(base_dir: Path \| None=None)` | Read cached entities. | Rust 原生 | [test_entities](../../../tests/test_entities.py) | pending | [llmwiki/entities.py:257](../../../llmwiki/entities.py#L257) |
| `llmwiki.entities._parse_entity_response(response: str)` | Parse LLM JSON response into entity dict. | Rust 原生 | [test_entities](../../../tests/test_entities.py) | pending | [llmwiki/entities.py:270](../../../llmwiki/entities.py#L270) |
| `llmwiki.export.export_article(slug: str, base_dir: Path \| None=None)` | Export a single article with full context. | Rust 原生 | [test_export](../../../tests/test_export.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/export.py:18](../../../llmwiki/export.py#L18) |
| `llmwiki.export.export_by_tag(tag: str, base_dir: Path \| None=None)` | Export all articles with a given tag. | Rust 原生 | [test_export](../../../tests/test_export.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/export.py:118](../../../llmwiki/export.py#L118) |
| `llmwiki.export.export_graph(slug: str, depth: int=2, base_dir: Path \| None=None)` | Export an article and N levels of connected articles. | Rust 原生 | [test_export](../../../tests/test_export.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/export.py:139](../../../llmwiki/export.py#L139) |
| `llmwiki.hooks.register(event: str, callback: Callable[..., Any])` | Register a callback for an event. May be called multiple times. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/hooks.py:25](../../../llmwiki/hooks.py#L25) |
| `llmwiki.hooks.emit(event: str, **kwargs: Any)` | Fire all callbacks registered for *event*. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/hooks.py:30](../../../llmwiki/hooks.py#L30) |
| `llmwiki.hooks.clear(event: str \| None=None)` | Remove all callbacks (or for a specific event). Useful for testing. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/hooks.py:43](../../../llmwiki/hooks.py#L43) |
| `llmwiki.ingest.ingest_url(url: str, base_dir: Path \| None=None)` | Fetch a web article and save as markdown with images downloaded locally. | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/ingest.py:51](../../../llmwiki/ingest.py#L51) |
| `llmwiki.ingest.ingest_file(file_path: str, base_dir: Path \| None=None)` | Copy a local file (paper PDF, markdown, etc.) into raw/. | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/ingest.py:135](../../../llmwiki/ingest.py#L135) |
| `llmwiki.ingest.ingest_directory(dir_path: str, base_dir: Path \| None=None)` | Ingest all supported files from a directory. | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/ingest.py:184](../../../llmwiki/ingest.py#L184) |
| `llmwiki.ingest.list_raw(base_dir: Path \| None=None)` | List all raw documents with their metadata. | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/ingest.py:256](../../../llmwiki/ingest.py#L256) |
| `llmwiki.lint.lint` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.lint_deep` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.auto_fix` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_structural` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_broken_links` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_orphans` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_missing_metadata` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_dirty_tags` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_stubs` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_uncategorized` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.check_duplicates` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.normalize_tags` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.fix_dirty_tags` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.clean_garbage` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.fix_uncategorized` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.fix_broken_links` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.merge_duplicates` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint._find_duplicate_candidates` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/__init__.py:34](../../../llmwiki/lint/__init__.py#L34) |
| `llmwiki.lint.checks.lint(base_dir: Path \| None=None)` | Run all lint checks on the wiki. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:56](../../../llmwiki/lint/checks.py#L56) |
| `llmwiki.lint.checks.lint_deep(base_dir: Path \| None=None)` | Use LLM to do a deep quality check on the wiki content. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:91](../../../llmwiki/lint/checks.py#L91) |
| `llmwiki.lint.checks.check_structural(cfg: dict, articles: list[dict] \| None=None)` | Check for structural issues. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:131](../../../llmwiki/lint/checks.py#L131) |
| `llmwiki.lint.checks.check_broken_links(cfg: dict, articles: list[dict] \| None=None, existing_slugs: set[str] \| None=None, aliases: dict \| None=None)` | Find broken wiki-links [[target]] that don't have corresponding articles. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:154](../../../llmwiki/lint/checks.py#L154) |
| `llmwiki.lint.checks.check_orphans(cfg: dict, articles: list[dict] \| None=None)` | Find articles that are not linked to from any other article. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:196](../../../llmwiki/lint/checks.py#L196) |
| `llmwiki.lint.checks.check_missing_metadata(cfg: dict, articles: list[dict] \| None=None)` | Find articles with missing or incomplete metadata. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:221](../../../llmwiki/lint/checks.py#L221) |
| `llmwiki.lint.checks.check_dirty_tags(cfg: dict, articles: list[dict] \| None=None)` | Find articles with malformed tags (LLM prompt leaks, sentences, etc.). | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:243](../../../llmwiki/lint/checks.py#L243) |
| `llmwiki.lint.checks.check_stubs(cfg: dict, articles: list[dict] \| None=None)` | Find garbage/empty stub articles that should be cleaned. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:281](../../../llmwiki/lint/checks.py#L281) |
| `llmwiki.lint.checks.check_uncategorized(cfg: dict, base_dir: Path \| None=None)` | Find articles that fall into 'Other' in the current taxonomy. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:327](../../../llmwiki/lint/checks.py#L327) |
| `llmwiki.lint.checks.check_duplicates(cfg: dict, articles: list[dict] \| None=None)` | Detect duplicate articles using scored heuristics. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/checks.py:340](../../../llmwiki/lint/checks.py#L340) |
| `llmwiki.lint.dedup.merge_duplicates(base_dir: Path \| None=None, max_merges: int=15)` | Merge confirmed duplicate articles using LLM. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/dedup.py:119](../../../llmwiki/lint/dedup.py#L119) |
| `llmwiki.lint.fixes.normalize_tags(base_dir: Path \| None=None)` | System-wide tag normalization: merge synonymous tags using LLM. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/fixes.py:33](../../../llmwiki/lint/fixes.py#L33) |
| `llmwiki.lint.fixes.fix_dirty_tags(base_dir: Path \| None=None)` | Clean up dirty tags by removing bad ones and regenerating via LLM. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/fixes.py:116](../../../llmwiki/lint/fixes.py#L116) |
| `llmwiki.lint.fixes.heal_urly_slugs(base_dir: Path \| None=None)` | Rename concept files whose slug is a URL fragment (issue #5). | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/fixes.py:164](../../../llmwiki/lint/fixes.py#L164) |
| `llmwiki.lint.fixes.clean_garbage(base_dir: Path \| None=None)` | Remove garbage/empty stub articles detected by check_stubs. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/fixes.py:257](../../../llmwiki/lint/fixes.py#L257) |
| `llmwiki.lint.fixes.fix_uncategorized(base_dir: Path \| None=None)` | Regenerate taxonomy to re-classify uncategorized articles. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/fixes.py:287](../../../llmwiki/lint/fixes.py#L287) |
| `llmwiki.lint.fixes.fix_broken_links(base_dir: Path \| None=None, max_stubs: int=10)` | Generate stub articles for broken wiki-link targets. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/fixes.py:303](../../../llmwiki/lint/fixes.py#L303) |
| `llmwiki.lint.fixes.auto_fix(base_dir: Path \| None=None)` | Attempt to auto-fix common lint issues using LLM. | Rust 原生 | [test_lint](../../../tests/test_lint.py) | pending | [llmwiki/lint/fixes.py:448](../../../llmwiki/lint/fixes.py#L448) |
| `llmwiki.llm.get_client(api_key: str \| None=None)` | Return an OpenAI client. | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:157](../../../llmwiki/llm.py#L157) |
| `llmwiki.llm.get_default_model()` |  | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:196](../../../llmwiki/llm.py#L196) |
| `llmwiki.llm.get_fallback_models()` | Get fallback model list from env. Comma-separated; empty = no fallback. | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:200](../../../llmwiki/llm.py#L200) |
| `llmwiki.llm.extract_json(text: str)` | Extract valid JSON from mixed thinking+content LLM output. | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:345](../../../llmwiki/llm.py#L345) |
| `llmwiki.llm.LLMMeta` | Metadata from a :func:`chat_with_meta` call (议 5-甲, v0.7.8). | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:398](../../../llmwiki/llm.py#L398) |
| `llmwiki.llm.chat_with_meta(prompt: str, system: str='', model: str \| None=None, max_tokens: int=16384, api_key: str \| None=None)` | Like :func:`chat` but also returns response metadata (v0.7.8). | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:442](../../../llmwiki/llm.py#L442) |
| `llmwiki.llm.chat(prompt: str, system: str='', model: str \| None=None, max_tokens: int=16384, api_key: str \| None=None)` | Send a prompt with automatic model fallback on failure. | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:550](../../../llmwiki/llm.py#L550) |
| `llmwiki.llm.reasoning_budget(max_tokens: int, tokens_per_char: float, safety: float=0.8)` | Safe input chunk size (chars) for a given output token budget (议 5-乙, v0.7.8). | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:576](../../../llmwiki/llm.py#L576) |
| `llmwiki.llm.strip_surrogates(s: str)` | Replace lone UTF-16 surrogates in *s* with ``?``. | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:681](../../../llmwiki/llm.py#L681) |
| `llmwiki.llm.chat_with_context(question: str, context_files: list[dict], system: str='', model: str \| None=None, max_tokens: int=16384, api_key: str \| None=None)` | Ask a question with file contents as context. | Rust 原生 | [test_llm](../../../tests/test_llm.py), [test_llm_meta](../../../tests/test_llm_meta.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/llm.py:701](../../../llmwiki/llm.py#L701) |
| `llmwiki.mcp_server.handle_tool(name: str, arguments: dict, base_dir: Path)` | Back-compat shim: synchronously dispatch a tool and return text. | Python 擴展邊界 | [test_query_promote](../../../tests/test_query_promote.py) | pending | [llmwiki/mcp_server.py:46](../../../llmwiki/mcp_server.py#L46) |
| `llmwiki.mcp_server.create_server(base_dir: Path)` |  | Python 擴展邊界 | [test_query_promote](../../../tests/test_query_promote.py) | pending | [llmwiki/mcp_server.py:73](../../../llmwiki/mcp_server.py#L73) |
| `llmwiki.mcp_server.main()` |  | Python 擴展邊界 | [test_query_promote](../../../tests/test_query_promote.py) | pending | [llmwiki/mcp_server.py:97](../../../llmwiki/mcp_server.py#L97) |
| `llmwiki.normalize.HeadRule` | One pattern→level rule for ``normalize_heads``. | Rust 原生 | [test_normalize](../../../tests/test_normalize.py) | pending | [llmwiki/normalize.py:63](../../../llmwiki/normalize.py#L63) |
| `llmwiki.normalize.normalize_paragraphs(body: str)` | Merge consecutive non-blank paragraph lines whose predecessor does | Rust 原生 | [test_normalize](../../../tests/test_normalize.py) | pending | [llmwiki/normalize.py:223](../../../llmwiki/normalize.py#L223) |
| `llmwiki.normalize.normalize_heads(body: str, rules: list[HeadRule])` | Rewrite ATX heading levels based on title-pattern rules. | Rust 原生 | [test_normalize](../../../tests/test_normalize.py) | pending | [llmwiki/normalize.py:391](../../../llmwiki/normalize.py#L391) |
| `llmwiki.operations.Operation` |  | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:32](../../../llmwiki/operations.py#L32) |
| `llmwiki.operations.register(op: Operation)` | Register an operation. Idempotent by name (last write wins). | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:44](../../../llmwiki/operations.py#L44) |
| `llmwiki.operations.get(name: str)` |  | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:50](../../../llmwiki/operations.py#L50) |
| `llmwiki.operations.all_operations()` |  | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:54](../../../llmwiki/operations.py#L54) |
| `llmwiki.operations.dispatch(name: str, base_dir: Path, arguments: dict \| None=None)` | Invoke an operation by name with lock management for writes. | Rust 原生 | [test_sections](../../../tests/test_sections.py), [test_query_promote](../../../tests/test_query_promote.py), [test_xllm_key](../../../tests/test_xllm_key.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/operations.py:73](../../../llmwiki/operations.py#L73) |
| `llmwiki.pdf.pdf_to_markdown(pdf_path: str, chunk_pages: int=0)` | Extract text from a PDF and convert to markdown documents. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/pdf.py:12](../../../llmwiki/pdf.py#L12) |
| `llmwiki.pdf.ingest_pdf(pdf_path: str, chunk_pages: int=20, base_dir: Path \| None=None)` | Ingest a PDF file into the knowledge base. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/pdf.py:66](../../../llmwiki/pdf.py#L66) |
| `llmwiki.pipeline.run_stage` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/__init__.py:39](../../../llmwiki/pipeline/__init__.py#L39) |
| `llmwiki.pipeline.StageContext` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/__init__.py:39](../../../llmwiki/pipeline/__init__.py#L39) |
| `llmwiki.pipeline.StagePartialExit` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/__init__.py:39](../../../llmwiki/pipeline/__init__.py#L39) |
| `llmwiki.pipeline.StageBusyError` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/__init__.py:39](../../../llmwiki/pipeline/__init__.py#L39) |
| `llmwiki.pipeline.StageState` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/__init__.py:39](../../../llmwiki/pipeline/__init__.py#L39) |
| `llmwiki.pipeline.rebuild_state` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/__init__.py:39](../../../llmwiki/pipeline/__init__.py#L39) |
| `llmwiki.pipeline.RESERVED_EVENTS` | Package-level public re-export; preserve import identity and runtime patches | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/__init__.py:39](../../../llmwiki/pipeline/__init__.py#L39) |
| `llmwiki.pipeline.driver.StageBusyError` | Raised by :func:`run_stage` when a live holder owns the lock. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:76](../../../llmwiki/pipeline/driver.py#L76) |
| `llmwiki.pipeline.driver.StagePartialExit` | Handler-raised signal that the run completed work but should be | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:84](../../../llmwiki/pipeline/driver.py#L84) |
| `llmwiki.pipeline.driver.StagePartialExit.__init__(self, reason: str)` |  | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:96](../../../llmwiki/pipeline/driver.py#L96) |
| `llmwiki.pipeline.driver.StageContext` | Handler-facing surface yielded by :func:`run_stage`. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:101](../../../llmwiki/pipeline/driver.py#L101) |
| `llmwiki.pipeline.driver.StageContext.__init__(self, base_dir: Path, stage: str, key: str)` |  | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:109](../../../llmwiki/pipeline/driver.py#L109) |
| `llmwiki.pipeline.driver.StageContext.base_dir(self)` |  | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:118](../../../llmwiki/pipeline/driver.py#L118) |
| `llmwiki.pipeline.driver.StageContext.stage(self)` |  | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:122](../../../llmwiki/pipeline/driver.py#L122) |
| `llmwiki.pipeline.driver.StageContext.key(self)` |  | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:126](../../../llmwiki/pipeline/driver.py#L126) |
| `llmwiki.pipeline.driver.StageContext.log(self, event: dict)` | Append a free-form event to the log. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:129](../../../llmwiki/pipeline/driver.py#L129) |
| `llmwiki.pipeline.driver.StageContext.artifact(self, path: str)` | Record an artifact path produced by this stage. Idempotent | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:149](../../../llmwiki/pipeline/driver.py#L149) |
| `llmwiki.pipeline.driver.StageContext.meta_update(self, **kwargs: Any)` | Merge keyword arguments into the run's meta. Keys | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:164](../../../llmwiki/pipeline/driver.py#L164) |
| `llmwiki.pipeline.driver.StageContext.mark_partial(self, reason: str)` | Flag this run as partial on normal exit. If called, the | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:188](../../../llmwiki/pipeline/driver.py#L188) |
| `llmwiki.pipeline.driver.run_stage(base_dir: Path \| str, stage: str, key: str, *, ttl: int=3600, meta_init: dict \| None=None)` | Enter a stage run. The **only** public entry point to | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/driver.py:203](../../../llmwiki/pipeline/driver.py#L203) |
| `llmwiki.pipeline.lock.StageLock` | Per-(stage, key) exclusive file lock. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/lock.py:131](../../../llmwiki/pipeline/lock.py#L131) |
| `llmwiki.pipeline.lock.StageLock.__init__(self, base_dir: Path \| str, stage: str, key: str)` |  | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/lock.py:134](../../../llmwiki/pipeline/lock.py#L134) |
| `llmwiki.pipeline.lock.StageLock.path(self)` |  | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/lock.py:142](../../../llmwiki/pipeline/lock.py#L142) |
| `llmwiki.pipeline.lock.StageLock.acquire(self, ttl: int=3600)` | Attempt to take the lock. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/lock.py:145](../../../llmwiki/pipeline/lock.py#L145) |
| `llmwiki.pipeline.lock.StageLock.release(self)` | Release the lock if we hold it. Idempotent: safe to call | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/lock.py:232](../../../llmwiki/pipeline/lock.py#L232) |
| `llmwiki.pipeline.lock.StageLock.force_break(self)` | Operator escape hatch: remove the pidfile regardless of | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/lock.py:249](../../../llmwiki/pipeline/lock.py#L249) |
| `llmwiki.pipeline.log.append(base_dir: Path, stage: str, key: str, event: dict)` | Append one event to the ``(stage, key)`` log. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/log.py:61](../../../llmwiki/pipeline/log.py#L61) |
| `llmwiki.pipeline.log.iter_events(base_dir: Path, stage: str, key: str)` | Yield every event in the log, oldest first. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/log.py:103](../../../llmwiki/pipeline/log.py#L103) |
| `llmwiki.pipeline.log.tail(base_dir: Path, stage: str, key: str, limit: int=50)` | Return the last ``limit`` events (most-recent last). | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/log.py:147](../../../llmwiki/pipeline/log.py#L147) |
| `llmwiki.pipeline.log.log_path(base_dir: Path, stage: str, key: str)` | Public accessor for the on-disk log path (read-only use). | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/log.py:159](../../../llmwiki/pipeline/log.py#L159) |
| `llmwiki.pipeline.state.StageState` | At-a-glance summary of a (stage, key) run history. | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/state.py:40](../../../llmwiki/pipeline/state.py#L40) |
| `llmwiki.pipeline.state.rebuild_state(base_dir: Path \| str, stage: str, key: str)` | Reconstruct :class:`StageState` from the log. Contract: | Rust 原生 | [test_pipeline](../../../tests/test_pipeline.py) | pending | [llmwiki/pipeline/state.py:79](../../../llmwiki/pipeline/state.py#L79) |
| `llmwiki.query.query(question: str, output_format: str='markdown', file_back: bool=False, base_dir: Path \| None=None, tone: str='default', return_path: bool=False, model: str \| None=None, api_key: str \| None=None)` | Ask a question against the wiki and return the answer. | Rust 原生 | [test_query_promote](../../../tests/test_query_promote.py), [test_query_prefilter](../../../tests/test_query_prefilter.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/query.py:77](../../../llmwiki/query.py#L77) |
| `llmwiki.query.query_with_search(question: str, base_dir: Path \| None=None, tone: str='default', file_back: bool=False, return_context: bool=False, promote: bool=False, model: str \| None=None, api_key: str \| None=None)` | Multi-step query: first search for relevant articles, then answer. | Rust 原生 | [test_query_promote](../../../tests/test_query_promote.py), [test_query_prefilter](../../../tests/test_query_prefilter.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/query.py:137](../../../llmwiki/query.py#L137) |
| `llmwiki.query.promote_to_concept(question: str, answer: str, consulted: list[dict], index: list[dict], base_dir: Path \| None=None)` | LLM judges whether a Q&A should become a wiki concept, and if yes, writes it. | Rust 原生 | [test_query_promote](../../../tests/test_query_promote.py), [test_query_prefilter](../../../tests/test_query_prefilter.py), [test_v064_features](../../../tests/test_v064_features.py) | pending | [llmwiki/query.py:332](../../../llmwiki/query.py#L332) |
| `llmwiki.refs.discover_plugins()` | Auto-discover ref plugins in this directory. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/refs/__init__.py:24](../../../llmwiki/refs/__init__.py#L24) |
| `llmwiki.refs.list_plugins()` | List all available ref plugins with metadata. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/refs/__init__.py:47](../../../llmwiki/refs/__init__.py#L47) |
| `llmwiki.refs.get_source_url(source: dict)` | Resolve a source dict to a permalink URL using the appropriate plugin. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/refs/__init__.py:59](../../../llmwiki/refs/__init__.py#L59) |
| `llmwiki.refs.cbeta.get_source_url(source: dict)` | Build CBETA Online permalink from source metadata. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/refs/cbeta.py:11](../../../llmwiki/refs/cbeta.py#L11) |
| `llmwiki.refs.ctext.get_source_url(source: dict)` | Build ctext.org permalink from source metadata. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/refs/ctext.py:11](../../../llmwiki/refs/ctext.py#L11) |
| `llmwiki.refs.wikisource.get_source_url(source: dict)` | Build Wikisource permalink from source metadata. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/refs/wikisource.py:11](../../../llmwiki/refs/wikisource.py#L11) |
| `llmwiki.resolve.build_aliases(concepts_dir: Path)` | Build alias map from all article metadata. | Rust 原生 | [test_resolve](../../../tests/test_resolve.py) | pending | [llmwiki/resolve.py:40](../../../llmwiki/resolve.py#L40) |
| `llmwiki.resolve.save_aliases(aliases: dict[str, str], meta_dir: Path)` | Write aliases.json to the meta directory (atomic). | Rust 原生 | [test_resolve](../../../tests/test_resolve.py) | pending | [llmwiki/resolve.py:84](../../../llmwiki/resolve.py#L84) |
| `llmwiki.resolve.load_aliases(meta_dir: Path)` | Load aliases.json from the meta directory. | Rust 原生 | [test_resolve](../../../tests/test_resolve.py) | pending | [llmwiki/resolve.py:90](../../../llmwiki/resolve.py#L90) |
| `llmwiki.resolve.resolve_link(target: str, aliases: dict[str, str])` | Resolve a wiki-link target to a canonical slug. | Rust 原生 | [test_resolve](../../../tests/test_resolve.py) | pending | [llmwiki/resolve.py:98](../../../llmwiki/resolve.py#L98) |
| `llmwiki.search.search(query: str, top_k: int=10, base_dir: Path \| None=None)` | Search the wiki using TF-IDF-like scoring. | Rust 原生 | [test_search_raw](../../../tests/test_search_raw.py) | pending | [llmwiki/search.py:47](../../../llmwiki/search.py#L47) |
| `llmwiki.search.search_raw(query: str, top_k: int=10, base_dir: Path \| None=None)` | Full-text search over the raw/ ingest directory (pre-compile sources). | Rust 原生 | [test_search_raw](../../../tests/test_search_raw.py) | pending | [llmwiki/search.py:119](../../../llmwiki/search.py#L119) |
| `llmwiki.search.search_cli(query: str, base_dir: Path \| None=None)` | CLI-friendly search output (for LLM tool use). | Rust 原生 | [test_search_raw](../../../tests/test_search_raw.py) | pending | [llmwiki/search.py:210](../../../llmwiki/search.py#L210) |
| `llmwiki.search.create_search_app(base_dir: Path \| None=None)` | Create Flask app for web UI search. | Rust 原生 | [test_search_raw](../../../tests/test_search_raw.py) | pending | [llmwiki/search.py:227](../../../llmwiki/search.py#L227) |
| `llmwiki.sections.Section` | One node in the section tree. | Rust 原生 | [test_sections](../../../tests/test_sections.py) | pending | [llmwiki/sections.py:71](../../../llmwiki/sections.py#L71) |
| `llmwiki.sections.normalize_title(title: str)` | Strip invisibles + punctuation + whitespace; preserve CJK / kana / alphanumerics. | Rust 原生 | [test_sections](../../../tests/test_sections.py) | pending | [llmwiki/sections.py:87](../../../llmwiki/sections.py#L87) |
| `llmwiki.sections.make_anchor(level: int, ancestor_titles: list[str])` | Build anchor for a heading given its level and ancestor chain (current title last). | Rust 原生 | [test_sections](../../../tests/test_sections.py) | pending | [llmwiki/sections.py:104](../../../llmwiki/sections.py#L104) |
| `llmwiki.sections.parse_sections(body: str)` | Parse a markdown body into a nested section tree. | Rust 原生 | [test_sections](../../../tests/test_sections.py) | pending | [llmwiki/sections.py:139](../../../llmwiki/sections.py#L139) |
| `llmwiki.sections.find_section(sections: list[Section], anchor: str)` | Walk the tree to find a section by exact anchor match. Returns None if absent. | Rust 原生 | [test_sections](../../../tests/test_sections.py) | pending | [llmwiki/sections.py:196](../../../llmwiki/sections.py#L196) |
| `llmwiki.sections.extract_section_text(body: str, sections: list[Section], anchor: str)` | Return body[start:end] for the named section subtree, or None if anchor not found. | Rust 原生 | [test_sections](../../../tests/test_sections.py) | pending | [llmwiki/sections.py:207](../../../llmwiki/sections.py#L207) |
| `llmwiki.split.Section` | One flat section produced by :func:`split_by_heading`. | Rust 原生 | [test_split](../../../tests/test_split.py) | pending | [llmwiki/split.py:46](../../../llmwiki/split.py#L46) |
| `llmwiki.split.split_by_heading(body: str, level: int)` | Split *body* at every ATX heading of depth *level*, return flat list. | Rust 原生 | [test_split](../../../tests/test_split.py) | pending | [llmwiki/split.py:64](../../../llmwiki/split.py#L64) |
| `llmwiki.sync.is_enabled()` | Return True iff sync env vars are set. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/sync.py:52](../../../llmwiki/sync.py#L52) |
| `llmwiki.sync.pull_ingested(source: str)` | Fetch the set of work_ids already ingested for a given source. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/sync.py:75](../../../llmwiki/sync.py#L75) |
| `llmwiki.sync.push_ingested(source: str, work_id: str, title: str='')` | Upsert a single (source, work_id) row marking it as ingested. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/sync.py:116](../../../llmwiki/sync.py#L116) |
| `llmwiki.sync.push_ingested_batch(rows: Iterable[dict])` | Bulk upsert. Each row must have at least 'source' and 'work_id'. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/sync.py:151](../../../llmwiki/sync.py#L151) |
| `llmwiki.sync.mark_compiled(source: str, work_id: str)` | Flip compiled=true / compiled_at=now for a known (source, work_id). | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/sync.py:194](../../../llmwiki/sync.py#L194) |
| `llmwiki.sync.pull_compiled(source: str)` | Fetch the set of work_ids already compiled for a given source. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/sync.py:232](../../../llmwiki/sync.py#L232) |
| `llmwiki.taxonomy.generate_taxonomy(base_dir: Path \| None=None)` | Use LLM to generate taxonomy from current articles, save to cache. | Rust 原生 | [test_languages](../../../tests/test_languages.py), [test_web_languages](../../../tests/test_web_languages.py) | pending | [llmwiki/taxonomy.py:105](../../../llmwiki/taxonomy.py#L105) |
| `llmwiki.taxonomy.build_taxonomy(base_dir: Path \| None=None, lang: str='zh')` | Read cached taxonomy and return localized tree for the web API. | Rust 原生 | [test_languages](../../../tests/test_languages.py), [test_web_languages](../../../tests/test_web_languages.py) | pending | [llmwiki/taxonomy.py:406](../../../llmwiki/taxonomy.py#L406) |
| `llmwiki.taxonomy.assign_new_articles(base_dir: Path \| None=None)` | Assign newly compiled articles to existing taxonomy categories. | Rust 原生 | [test_languages](../../../tests/test_languages.py), [test_web_languages](../../../tests/test_web_languages.py) | pending | [llmwiki/taxonomy.py:449](../../../llmwiki/taxonomy.py#L449) |
| `llmwiki.taxonomy.load_taxonomy(base_dir: Path \| None=None)` | Load cached taxonomy (raw, not localized). | Rust 原生 | [test_languages](../../../tests/test_languages.py), [test_web_languages](../../../tests/test_web_languages.py) | pending | [llmwiki/taxonomy.py:547](../../../llmwiki/taxonomy.py#L547) |
| `llmwiki.web.derive_session_token(secret: str)` | Derive a session cookie token from the API secret. | Python 擴展邊界 | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:121](../../../llmwiki/web.py#L121) |
| `llmwiki.web.require_auth(f)` | Decorator: protect a route when LLMBASE_API_SECRET is set. | Python 擴展邊界 | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:191](../../../llmwiki/web.py#L191) |
| `llmwiki.web.create_web_app(base_dir: Path \| None=None)` | Create the full web application. | Python 擴展邊界 | [test_web_articles](../../../tests/test_web_articles.py), [test_web_languages](../../../tests/test_web_languages.py), [test_sections](../../../tests/test_sections.py), [test_v064_features](../../../tests/test_v064_features.py), [test_xllm_key](../../../tests/test_xllm_key.py) | pending | [llmwiki/web.py:224](../../../llmwiki/web.py#L224) |
| `llmwiki.wikisource.fetch_page(title: str)` | Fetch a page's wikitext content via MediaWiki API. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/wikisource.py:48](../../../llmwiki/wikisource.py#L48) |
| `llmwiki.wikisource.fetch_subpages(title: str)` | Fetch subpage list for a multi-chapter work. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/wikisource.py:77](../../../llmwiki/wikisource.py#L77) |
| `llmwiki.wikisource.ingest_work(title: str, base_dir: Path \| None=None)` | Ingest a complete work from Wikisource. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/wikisource.py:94](../../../llmwiki/wikisource.py#L94) |
| `llmwiki.wikisource._ingest_page(page_title: str, work_title: str, raw_dir: Path)` | Ingest a single page. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/wikisource.py:125](../../../llmwiki/wikisource.py#L125) |
| `llmwiki.wikisource.learn(reading_list: str \| None=None, batch_size: int=5, base_dir: Path \| None=None)` | Progressive learning from Wikisource. | Python 擴展邊界 | 未見專項測試；須新增 | pending | [llmwiki/wikisource.py:160](../../../llmwiki/wikisource.py#L160) |
| `llmwiki.worker.register_learn_source(name: str, handler)` | Register a learn source handler. | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/worker.py:54](../../../llmwiki/worker.py#L54) |
| `llmwiki.worker.register_job(job_id: str, interval_hours: float, handler)` | Register a custom background job. | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/worker.py:74](../../../llmwiki/worker.py#L74) |
| `llmwiki.worker.run_worker(base_dir: Path \| None=None)` | Main worker loop — runs forever, executing scheduled tasks. | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/worker.py:86](../../../llmwiki/worker.py#L86) |
| `llmwiki.worker.start_worker_thread(base_dir: Path \| None=None)` | Start worker as a background daemon thread (only once per process). | Rust 原生 | [test_v064_features](../../../tests/test_v064_features.py), [test_languages](../../../tests/test_languages.py) | pending | [llmwiki/worker.py:337](../../../llmwiki/worker.py#L337) |
| `llmwiki.xici.generate_xici(base_dir: Path \| None=None, lang: str='zh')` | Generate Xi Ci for the given language. | Rust 原生 | [test_languages](../../../tests/test_languages.py), [test_web_languages](../../../tests/test_web_languages.py) | pending | [llmwiki/xici.py:54](../../../llmwiki/xici.py#L54) |
| `llmwiki.xici.get_xici(base_dir: Path \| None=None, lang: str='zh')` | Get cached Xi Ci, or empty if not generated yet. | Rust 原生 | [test_languages](../../../tests/test_languages.py), [test_web_languages](../../../tests/test_web_languages.py) | pending | [llmwiki/xici.py:201](../../../llmwiki/xici.py#L201) |

## 現有測試索引

僅索引已存在 test function，未聲稱已執行或完整覆蓋。具體 test class 的 nodeid 仍依 pytest collection；baseline.json 留 file/line 可查。

| 測試 | 來源 |
|---|---|
| `test_normalize_none_is_identity` | [tests/test_anchor.py:11](../../../tests/test_anchor.py#L11) |
| `test_normalize_punct_strips_cjk_and_ascii` | [tests/test_anchor.py:16](../../../tests/test_anchor.py#L16) |
| `test_normalize_punct_keeps_whitespace` | [tests/test_anchor.py:21](../../../tests/test_anchor.py#L21) |
| `test_normalize_punct_spaces_strips_all_whitespace_incl_ideographic` | [tests/test_anchor.py:26](../../../tests/test_anchor.py#L26) |
| `test_normalize_punct_spaces_combines_punct_and_spaces` | [tests/test_anchor.py:30](../../../tests/test_anchor.py#L30) |
| `test_normalize_unknown_level_raises` | [tests/test_anchor.py:34](../../../tests/test_anchor.py#L34) |
| `test_head_tail_exact` | [tests/test_anchor.py:42](../../../tests/test_anchor.py#L42) |
| `test_head_tail_exact_without_normalize` | [tests/test_anchor.py:51](../../../tests/test_anchor.py#L51) |
| `test_punct_differ_matches_through_comma` | [tests/test_anchor.py:62](../../../tests/test_anchor.py#L62) |
| `test_punct_spaces_bridges_spaces_and_punct` | [tests/test_anchor.py:69](../../../tests/test_anchor.py#L69) |
| `test_offsets_are_original_not_normalized` | [tests/test_anchor.py:76](../../../tests/test_anchor.py#L76) |
| `test_offsets_exclude_trailing_dropped_chars` | [tests/test_anchor.py:88](../../../tests/test_anchor.py#L88) |
| `test_head_only_when_tail_missing` | [tests/test_anchor.py:100](../../../tests/test_anchor.py#L100) |
| `test_empty_tail_yields_head_only` | [tests/test_anchor.py:109](../../../tests/test_anchor.py#L109) |
| `test_empty_tail_explicit_empty_string` | [tests/test_anchor.py:117](../../../tests/test_anchor.py#L117) |
| `test_tail_all_punct_treated_as_empty` | [tests/test_anchor.py:124](../../../tests/test_anchor.py#L124) |
| `test_tail_before_head_does_not_match` | [tests/test_anchor.py:132](../../../tests/test_anchor.py#L132) |
| `test_no_match_returns_none` | [tests/test_anchor.py:143](../../../tests/test_anchor.py#L143) |
| `test_empty_content_returns_none` | [tests/test_anchor.py:147](../../../tests/test_anchor.py#L147) |
| `test_empty_head_raises` | [tests/test_anchor.py:151](../../../tests/test_anchor.py#L151) |
| `test_head_all_punct_normalizes_to_empty_returns_none` | [tests/test_anchor.py:156](../../../tests/test_anchor.py#L156) |
| `test_head_longer_than_content_returns_none` | [tests/test_anchor.py:162](../../../tests/test_anchor.py#L162) |
| `test_anchor_invariant_start_lt_end` | [tests/test_anchor.py:169](../../../tests/test_anchor.py#L169) |
| `test_anchor_frozen_dataclass` | [tests/test_anchor.py:178](../../../tests/test_anchor.py#L178) |
| `test_large_content_performance` | [tests/test_anchor.py:187](../../../tests/test_anchor.py#L187) |
| `test_atomic_write_creates_file` | [tests/test_atomic.py:9](../../../tests/test_atomic.py#L9) |
| `test_atomic_write_overwrites` | [tests/test_atomic.py:18](../../../tests/test_atomic.py#L18) |
| `test_atomic_write_creates_dirs` | [tests/test_atomic.py:26](../../../tests/test_atomic.py#L26) |
| `test_atomic_write_unicode` | [tests/test_atomic.py:34](../../../tests/test_atomic.py#L34) |
| `test_atomic_write_no_temp_leftover` | [tests/test_atomic.py:43](../../../tests/test_atomic.py#L43) |
| `test_get_unknown_cid_returns_none` | [tests/test_chunk_cache.py:27](../../../tests/test_chunk_cache.py#L27) |
| `test_put_then_get_round_trips` | [tests/test_chunk_cache.py:32](../../../tests/test_chunk_cache.py#L32) |
| `test_put_preserves_utf8_content` | [tests/test_chunk_cache.py:38](../../../tests/test_chunk_cache.py#L38) |
| `test_put_accepts_empty_string` | [tests/test_chunk_cache.py:45](../../../tests/test_chunk_cache.py#L45) |
| `test_put_rejects_non_string` | [tests/test_chunk_cache.py:52](../../../tests/test_chunk_cache.py#L52) |
| `test_same_cid_different_content_hash_is_miss` | [tests/test_chunk_cache.py:61](../../../tests/test_chunk_cache.py#L61) |
| `test_different_cid_same_content_hash_isolated` | [tests/test_chunk_cache.py:72](../../../tests/test_chunk_cache.py#L72) |
| `test_overwrite_same_key` | [tests/test_chunk_cache.py:83](../../../tests/test_chunk_cache.py#L83) |
| `test_clear_removes_all_hashes_for_cid` | [tests/test_chunk_cache.py:95](../../../tests/test_chunk_cache.py#L95) |
| `test_clear_noop_on_unknown_cid` | [tests/test_chunk_cache.py:107](../../../tests/test_chunk_cache.py#L107) |
| `test_put_after_clear_works` | [tests/test_chunk_cache.py:114](../../../tests/test_chunk_cache.py#L114) |
| `test_cid_with_filesystem_unfriendly_chars` | [tests/test_chunk_cache.py:125](../../../tests/test_chunk_cache.py#L125) |
| `test_content_hash_with_special_chars` | [tests/test_chunk_cache.py:136](../../../tests/test_chunk_cache.py#L136) |
| `test_root_property_exposes_dir` | [tests/test_chunk_cache.py:142](../../../tests/test_chunk_cache.py#L142) |
| `test_custom_subdir` | [tests/test_chunk_cache.py:147](../../../tests/test_chunk_cache.py#L147) |
| `test_cid_and_content_hashes_use_full_sha256` | [tests/test_chunk_cache.py:154](../../../tests/test_chunk_cache.py#L154) |
| `test_clear_propagates_non_enoent_errors` | [tests/test_chunk_cache.py:171](../../../tests/test_chunk_cache.py#L171) |
| `test_atomic_write_no_partial_on_exception` | [tests/test_chunk_cache.py:190](../../../tests/test_chunk_cache.py#L190) |
| `test_no_tmp_leak_after_successful_put` | [tests/test_chunk_cache.py:212](../../../tests/test_chunk_cache.py#L212) |
| `test_boundary_shift_scenario` | [tests/test_chunk_cache.py:228](../../../tests/test_chunk_cache.py#L228) |
| `test_rebuild_index` | [tests/test_compile.py:12](../../../tests/test_compile.py#L12) |
| `test_rebuild_creates_backlinks` | [tests/test_compile.py:36](../../../tests/test_compile.py#L36) |
| `test_split_sections` | [tests/test_compile.py:46](../../../tests/test_compile.py#L46) |
| `test_assemble_sections` | [tests/test_compile.py:69](../../../tests/test_compile.py#L69) |
| `test_wenyan_empty_header_split_whole_body` | [tests/test_compile.py:83](../../../tests/test_compile.py#L83) |
| `test_wenyan_empty_header_assemble_bare_body` | [tests/test_compile.py:92](../../../tests/test_compile.py#L92) |
| `test_wenyan_empty_header_roundtrip` | [tests/test_compile.py:101](../../../tests/test_compile.py#L101) |
| `test_wenyan_empty_header_merge_replaces_longer_and_keeps_shorter` | [tests/test_compile.py:108](../../../tests/test_compile.py#L108) |
| `test_default_trilingual_split_assemble_regression` | [tests/test_compile.py:131](../../../tests/test_compile.py#L131) |
| `test_section_headers_patch_wins_for_empty_header_split` | [tests/test_compile.py:150](../../../tests/test_compile.py#L150) |
| `test_merge_into_adds_content` | [tests/test_compile.py:159](../../../tests/test_compile.py#L159) |
| `test_merge_into_no_duplicate_content` | [tests/test_compile.py:174](../../../tests/test_compile.py#L174) |
| `test_dedup_by_name` | [tests/test_entities.py:6](../../../tests/test_entities.py#L6) |
| `test_dedup_by_name_local` | [tests/test_entities.py:17](../../../tests/test_entities.py#L17) |
| `test_dedup_null_articles` | [tests/test_entities.py:28](../../../tests/test_entities.py#L28) |
| `test_dedup_non_dict_items` | [tests/test_entities.py:38](../../../tests/test_entities.py#L38) |
| `test_dedup_events_date_field` | [tests/test_entities.py:44](../../../tests/test_entities.py#L44) |
| `test_dedup_empty` | [tests/test_entities.py:54](../../../tests/test_entities.py#L54) |
| `test_parse_entity_response_valid` | [tests/test_entities.py:59](../../../tests/test_entities.py#L59) |
| `test_parse_entity_response_with_thinking` | [tests/test_entities.py:65](../../../tests/test_entities.py#L65) |
| `test_parse_entity_response_invalid` | [tests/test_entities.py:71](../../../tests/test_entities.py#L71) |
| `test_export_article` | [tests/test_export.py:9](../../../tests/test_export.py#L9) |
| `test_export_article_alias` | [tests/test_export.py:24](../../../tests/test_export.py#L24) |
| `test_export_article_not_found` | [tests/test_export.py:33](../../../tests/test_export.py#L33) |
| `test_export_article_content_split` | [tests/test_export.py:38](../../../tests/test_export.py#L38) |
| `test_export_article_backlinks` | [tests/test_export.py:46](../../../tests/test_export.py#L46) |
| `test_export_article_outgoing_links` | [tests/test_export.py:55](../../../tests/test_export.py#L55) |
| `test_export_article_related_by_tags` | [tests/test_export.py:63](../../../tests/test_export.py#L63) |
| `test_export_by_tag` | [tests/test_export.py:72](../../../tests/test_export.py#L72) |
| `test_export_by_tag_empty` | [tests/test_export.py:82](../../../tests/test_export.py#L82) |
| `test_export_graph` | [tests/test_export.py:87](../../../tests/test_export.py#L87) |
| `test_export_graph_depth_0` | [tests/test_export.py:98](../../../tests/test_export.py#L98) |
| `test_builtin_trilingual_profile_without_cfg` | [tests/test_languages.py:9](../../../tests/test_languages.py#L9) |
| `test_config_wenyan_profile_bare_body` | [tests/test_languages.py:26](../../../tests/test_languages.py#L26) |
| `test_import_time_section_headers_patch_wins_over_config` | [tests/test_languages.py:45](../../../tests/test_languages.py#L45) |
| `test_validation_rejects_empty_header_with_two_sections` | [tests/test_languages.py:59](../../../tests/test_languages.py#L59) |
| `test_validation_rejects_missing_default_profile` | [tests/test_languages.py:78](../../../tests/test_languages.py#L78) |
| `test_validation_rejects_view_show_unknown_code` | [tests/test_languages.py:94](../../../tests/test_languages.py#L94) |
| `test_check_broken_links_with_aliases` | [tests/test_lint.py:22](../../../tests/test_lint.py#L22) |
| `test_check_orphans` | [tests/test_lint.py:37](../../../tests/test_lint.py#L37) |
| `test_check_missing_metadata` | [tests/test_lint.py:46](../../../tests/test_lint.py#L46) |
| `test_check_stubs_detects_template` | [tests/test_lint.py:63](../../../tests/test_lint.py#L63) |
| `test_check_stubs_detects_prompt_leak` | [tests/test_lint.py:79](../../../tests/test_lint.py#L79) |
| `test_check_dirty_tags` | [tests/test_lint.py:93](../../../tests/test_lint.py#L93) |
| `test_clean_garbage` | [tests/test_lint.py:105](../../../tests/test_lint.py#L105) |
| `test_find_duplicate_candidates_cjk` | [tests/test_lint.py:119](../../../tests/test_lint.py#L119) |
| `test_find_duplicate_candidates_no_false_positive` | [tests/test_lint.py:135](../../../tests/test_lint.py#L135) |
| `test_extract_json_clean_array` | [tests/test_llm.py:6](../../../tests/test_llm.py#L6) |
| `test_extract_json_clean_object` | [tests/test_llm.py:10](../../../tests/test_llm.py#L10) |
| `test_extract_json_with_thinking` | [tests/test_llm.py:14](../../../tests/test_llm.py#L14) |
| `test_extract_json_thinking_with_brackets` | [tests/test_llm.py:19](../../../tests/test_llm.py#L19) |
| `test_extract_json_plain_text` | [tests/test_llm.py:25](../../../tests/test_llm.py#L25) |
| `test_extract_json_rightmost_wins` | [tests/test_llm.py:30](../../../tests/test_llm.py#L30) |
| `test_extract_json_markdown_fenced` | [tests/test_llm.py:37](../../../tests/test_llm.py#L37) |
| `test_extract_json_invalid_json` | [tests/test_llm.py:45](../../../tests/test_llm.py#L45) |
| `test_load_env_cwd_with_config_yaml` | [tests/test_llm.py:62](../../../tests/test_llm.py#L62) |
| `test_load_env_cwd_with_unrelated_config_yaml_is_ignored` | [tests/test_llm.py:76](../../../tests/test_llm.py#L76) |
| `test_load_env_cwd_without_config_yaml_is_ignored` | [tests/test_llm.py:95](../../../tests/test_llm.py#L95) |
| `test_load_env_explicit_override` | [tests/test_llm.py:114](../../../tests/test_llm.py#L114) |
| `test_load_env_explicit_override_missing_fails_closed` | [tests/test_llm.py:130](../../../tests/test_llm.py#L130) |
| `test_load_env_shell_export_wins` | [tests/test_llm.py:145](../../../tests/test_llm.py#L145) |
| `test_load_env_empty_file_is_still_selected` | [tests/test_llm.py:163](../../../tests/test_llm.py#L163) |
| `test_load_env_explicit_override_empty_file_ok` | [tests/test_llm.py:182](../../../tests/test_llm.py#L182) |
| `test_load_env_xdg_fallback` | [tests/test_llm.py:193](../../../tests/test_llm.py#L193) |
| `test_chat_with_meta_happy_path` | [tests/test_llm_meta.py:39](../../../tests/test_llm_meta.py#L39) |
| `test_chat_with_meta_length_cut_returns_truncated_true` | [tests/test_llm_meta.py:57](../../../tests/test_llm_meta.py#L57) |
| `test_chat_with_meta_unknown_finish_reason_not_truncated` | [tests/test_llm_meta.py:74](../../../tests/test_llm_meta.py#L74) |
| `test_chat_with_meta_reasoning_tokens_flattened_from_openai_nested` | [tests/test_llm_meta.py:87](../../../tests/test_llm_meta.py#L87) |
| `test_extract_usage_missing_details_leaves_reasoning_none` | [tests/test_llm_meta.py:109](../../../tests/test_llm_meta.py#L109) |
| `test_extract_usage_none_returns_none_filled_dict` | [tests/test_llm_meta.py:123](../../../tests/test_llm_meta.py#L123) |
| `test_extract_usage_accepts_dict_shaped_payload` | [tests/test_llm_meta.py:132](../../../tests/test_llm_meta.py#L132) |
| `test_extract_usage_dict_without_reasoning_details` | [tests/test_llm_meta.py:153](../../../tests/test_llm_meta.py#L153) |
| `test_chat_with_meta_fallback_records_fallback_model_and_attempts` | [tests/test_llm_meta.py:166](../../../tests/test_llm_meta.py#L166) |
| `test_chat_with_meta_attempts_one_on_clean_first_try` | [tests/test_llm_meta.py:196](../../../tests/test_llm_meta.py#L196) |
| `test_chat_with_meta_empty_exhausted_returns_empty_plus_last_meta` | [tests/test_llm_meta.py:213](../../../tests/test_llm_meta.py#L213) |
| `test_chat_with_meta_empty_then_length_cut_last_round_wins` | [tests/test_llm_meta.py:235](../../../tests/test_llm_meta.py#L235) |
| `test_chat_with_meta_forwards_api_key` | [tests/test_llm_meta.py:264](../../../tests/test_llm_meta.py#L264) |
| `test_chat_with_meta_does_not_leak_api_key_into_meta` | [tests/test_llm_meta.py:278](../../../tests/test_llm_meta.py#L278) |
| `test_chat_returns_bare_string_not_tuple` | [tests/test_llm_meta.py:296](../../../tests/test_llm_meta.py#L296) |
| `test_chat_empty_exhausted_returns_empty_string` | [tests/test_llm_meta.py:309](../../../tests/test_llm_meta.py#L309) |
| `test_reasoning_budget_siwen_postmortem_numbers` | [tests/test_llm_meta.py:329](../../../tests/test_llm_meta.py#L329) |
| `test_reasoning_budget_tight_safety` | [tests/test_llm_meta.py:338](../../../tests/test_llm_meta.py#L338) |
| `test_reasoning_budget_ascii_summarization_ratio` | [tests/test_llm_meta.py:344](../../../tests/test_llm_meta.py#L344) |
| `test_reasoning_budget_never_returns_zero_for_valid_input` | [tests/test_llm_meta.py:351](../../../tests/test_llm_meta.py#L351) |
| `test_reasoning_budget_rejects_nonpositive_max_tokens` | [tests/test_llm_meta.py:359](../../../tests/test_llm_meta.py#L359) |
| `test_reasoning_budget_rejects_nonpositive_ratio` | [tests/test_llm_meta.py:367](../../../tests/test_llm_meta.py#L367) |
| `test_reasoning_budget_rejects_safety_out_of_range` | [tests/test_llm_meta.py:375](../../../tests/test_llm_meta.py#L375) |
| `test_reasoning_budget_safety_one_is_valid` | [tests/test_llm_meta.py:385](../../../tests/test_llm_meta.py#L385) |
| `test_reasoning_budget_is_pure` | [tests/test_llm_meta.py:391](../../../tests/test_llm_meta.py#L391) |
| `test_reasoning_budget_rejects_inf_overflow_as_valueerror` | [tests/test_llm_meta.py:399](../../../tests/test_llm_meta.py#L399) |
| `test_reasoning_budget_rejects_nan_ratio` | [tests/test_llm_meta.py:415](../../../tests/test_llm_meta.py#L415) |
| `test_reasoning_budget_rejects_int_too_large_for_float` | [tests/test_llm_meta.py:424](../../../tests/test_llm_meta.py#L424) |
| `test_paragraphs_merges_non_terminator_line` | [tests/test_normalize.py:17](../../../tests/test_normalize.py#L17) |
| `test_paragraphs_keeps_break_after_terminator` | [tests/test_normalize.py:22](../../../tests/test_normalize.py#L22) |
| `test_paragraphs_merges_chain_across_multiple_lines` | [tests/test_normalize.py:27](../../../tests/test_normalize.py#L27) |
| `test_paragraphs_preserves_blank_line_as_boundary` | [tests/test_normalize.py:32](../../../tests/test_normalize.py#L32) |
| `test_paragraphs_terminator_followed_by_closing_wrapper` | [tests/test_normalize.py:37](../../../tests/test_normalize.py#L37) |
| `test_paragraphs_does_not_merge_into_heading` | [tests/test_normalize.py:43](../../../tests/test_normalize.py#L43) |
| `test_paragraphs_does_not_merge_into_empty_heading` | [tests/test_normalize.py:49](../../../tests/test_normalize.py#L49) |
| `test_paragraphs_does_not_merge_heading_into_body` | [tests/test_normalize.py:58](../../../tests/test_normalize.py#L58) |
| `test_paragraphs_preserves_list_items` | [tests/test_normalize.py:63](../../../tests/test_normalize.py#L63) |
| `test_paragraphs_preserves_list_item_continuation` | [tests/test_normalize.py:69](../../../tests/test_normalize.py#L69) |
| `test_paragraphs_list_ends_on_blank_line` | [tests/test_normalize.py:76](../../../tests/test_normalize.py#L76) |
| `test_paragraphs_preserves_blockquote_continuation` | [tests/test_normalize.py:82](../../../tests/test_normalize.py#L82) |
| `test_paragraphs_preserves_html_block_inner` | [tests/test_normalize.py:88](../../../tests/test_normalize.py#L88) |
| `test_paragraphs_inline_html_does_not_open_block` | [tests/test_normalize.py:95](../../../tests/test_normalize.py#L95) |
| `test_paragraphs_preserves_link_reference_definition` | [tests/test_normalize.py:102](../../../tests/test_normalize.py#L102) |
| `test_paragraphs_preserves_indented_code_block` | [tests/test_normalize.py:109](../../../tests/test_normalize.py#L109) |
| `test_paragraphs_indented_code_ends_on_blank_line` | [tests/test_normalize.py:116](../../../tests/test_normalize.py#L116) |
| `test_paragraphs_preserves_tab_indented_code` | [tests/test_normalize.py:121](../../../tests/test_normalize.py#L121) |
| `test_paragraphs_indented_code_after_heading` | [tests/test_normalize.py:127](../../../tests/test_normalize.py#L127) |
| `test_paragraphs_line_after_indented_code_does_not_merge` | [tests/test_normalize.py:134](../../../tests/test_normalize.py#L134) |
| `test_paragraphs_fence_inside_list_keeps_container` | [tests/test_normalize.py:142](../../../tests/test_normalize.py#L142) |
| `test_paragraphs_multi_paragraph_list_item` | [tests/test_normalize.py:150](../../../tests/test_normalize.py#L150) |
| `test_paragraphs_multi_paragraph_list_item_deep` | [tests/test_normalize.py:158](../../../tests/test_normalize.py#L158) |
| `test_paragraphs_preserves_blockquote` | [tests/test_normalize.py:165](../../../tests/test_normalize.py#L165) |
| `test_paragraphs_preserves_table_row` | [tests/test_normalize.py:170](../../../tests/test_normalize.py#L170) |
| `test_paragraphs_preserves_thematic_break` | [tests/test_normalize.py:175](../../../tests/test_normalize.py#L175) |
| `test_paragraphs_preserves_fenced_code_verbatim` | [tests/test_normalize.py:180](../../../tests/test_normalize.py#L180) |
| `test_paragraphs_fence_opener_with_info_string` | [tests/test_normalize.py:187](../../../tests/test_normalize.py#L187) |
| `test_paragraphs_tilde_fences` | [tests/test_normalize.py:193](../../../tests/test_normalize.py#L193) |
| `test_paragraphs_preserves_missing_trailing_newline` | [tests/test_normalize.py:198](../../../tests/test_normalize.py#L198) |
| `test_paragraphs_preserves_multiple_trailing_newlines` | [tests/test_normalize.py:203](../../../tests/test_normalize.py#L203) |
| `test_paragraphs_mixed_endings_preserve_trailing_newline` | [tests/test_normalize.py:210](../../../tests/test_normalize.py#L210) |
| `test_paragraphs_lstrips_lazy_continuation` | [tests/test_normalize.py:215](../../../tests/test_normalize.py#L215) |
| `test_paragraphs_empty_input` | [tests/test_normalize.py:222](../../../tests/test_normalize.py#L222) |
| `test_paragraphs_preserves_crlf` | [tests/test_normalize.py:226](../../../tests/test_normalize.py#L226) |
| `test_paragraphs_override_terminators` | [tests/test_normalize.py:232](../../../tests/test_normalize.py#L232) |
| `test_paragraphs_default_constants_shape` | [tests/test_normalize.py:240](../../../tests/test_normalize.py#L240) |
| `test_heads_no_rules_is_noop` | [tests/test_normalize.py:255](../../../tests/test_normalize.py#L255) |
| `test_heads_rewrites_by_pattern` | [tests/test_normalize.py:260](../../../tests/test_normalize.py#L260) |
| `test_heads_non_matching_heading_preserved` | [tests/test_normalize.py:266](../../../tests/test_normalize.py#L266) |
| `test_heads_first_match_wins` | [tests/test_normalize.py:271](../../../tests/test_normalize.py#L271) |
| `test_heads_preserves_indent_and_closer` | [tests/test_normalize.py:280](../../../tests/test_normalize.py#L280) |
| `test_heads_skips_fenced_code` | [tests/test_normalize.py:287](../../../tests/test_normalize.py#L287) |
| `test_heads_skips_html_block` | [tests/test_normalize.py:294](../../../tests/test_normalize.py#L294) |
| `test_heads_rejects_invalid_level` | [tests/test_normalize.py:302](../../../tests/test_normalize.py#L302) |
| `test_heads_preserves_missing_trailing_newline` | [tests/test_normalize.py:309](../../../tests/test_normalize.py#L309) |
| `test_heads_empty_input` | [tests/test_normalize.py:314](../../../tests/test_normalize.py#L314) |
| `test_heads_preserves_crlf` | [tests/test_normalize.py:318](../../../tests/test_normalize.py#L318) |
| `test_paragraphs_then_heads_composes_cleanly` | [tests/test_normalize.py:326](../../../tests/test_normalize.py#L326) |
| `test_happy_path_writes_ok` | [tests/test_pipeline.py:64](../../../tests/test_pipeline.py#L64) |
| `test_pending_when_no_log` | [tests/test_pipeline.py:76](../../../tests/test_pipeline.py#L76) |
| `test_reserved_event_names_rejected` | [tests/test_pipeline.py:85](../../../tests/test_pipeline.py#L85) |
| `test_ctx_log_requires_event_field` | [tests/test_pipeline.py:99](../../../tests/test_pipeline.py#L99) |
| `test_mark_partial_writes_partial` | [tests/test_pipeline.py:108](../../../tests/test_pipeline.py#L108) |
| `test_stage_partial_exit_is_swallowed` | [tests/test_pipeline.py:119](../../../tests/test_pipeline.py#L119) |
| `test_partial_reason_non_string_coerced` | [tests/test_pipeline.py:132](../../../tests/test_pipeline.py#L132) |
| `test_terminal_survives_broken_str_and_repr` | [tests/test_pipeline.py:159](../../../tests/test_pipeline.py#L159) |
| `test_partial_reason_defaults_to_empty` | [tests/test_pipeline.py:196](../../../tests/test_pipeline.py#L196) |
| `test_exception_writes_failed_and_reraises` | [tests/test_pipeline.py:209](../../../tests/test_pipeline.py#L209) |
| `test_baseexception_still_writes_terminal` | [tests/test_pipeline.py:220](../../../tests/test_pipeline.py#L220) |
| `test_failed_err_is_truncated` | [tests/test_pipeline.py:232](../../../tests/test_pipeline.py#L232) |
| `test_artifact_dedup_within_run` | [tests/test_pipeline.py:249](../../../tests/test_pipeline.py#L249) |
| `test_artifact_dedup_across_rounds` | [tests/test_pipeline.py:263](../../../tests/test_pipeline.py#L263) |
| `test_artifact_requires_non_empty_string` | [tests/test_pipeline.py:281](../../../tests/test_pipeline.py#L281) |
| `test_meta_init_populates_meta` | [tests/test_pipeline.py:290](../../../tests/test_pipeline.py#L290) |
| `test_meta_update_last_write_wins` | [tests/test_pipeline.py:298](../../../tests/test_pipeline.py#L298) |
| `test_meta_only_last_round_applies` | [tests/test_pipeline.py:307](../../../tests/test_pipeline.py#L307) |
| `test_meta_update_noop_without_kwargs` | [tests/test_pipeline.py:326](../../../tests/test_pipeline.py#L326) |
| `test_multi_round_last_terminal_wins` | [tests/test_pipeline.py:337](../../../tests/test_pipeline.py#L337) |
| `test_multi_round_success_after_failure` | [tests/test_pipeline.py:350](../../../tests/test_pipeline.py#L350) |
| `test_running_status_when_last_round_unterminated` | [tests/test_pipeline.py:366](../../../tests/test_pipeline.py#L366) |
| `test_interrupted_status_after_stale_break` | [tests/test_pipeline.py:380](../../../tests/test_pipeline.py#L380) |
| `test_busy_lock_blocks_live_holder` | [tests/test_pipeline.py:420](../../../tests/test_pipeline.py#L420) |
| `test_dead_pid_lockfile_is_broken` | [tests/test_pipeline.py:437](../../../tests/test_pipeline.py#L437) |
| `test_cross_host_lockfile_is_broken` | [tests/test_pipeline.py:450](../../../tests/test_pipeline.py#L450) |
| `test_malformed_pidfile_is_broken` | [tests/test_pipeline.py:475](../../../tests/test_pipeline.py#L475) |
| `test_pathological_pid_types_are_stale` | [tests/test_pipeline.py:488](../../../tests/test_pipeline.py#L488) |
| `test_live_pid_lock_NEVER_broken_even_past_ttl` | [tests/test_pipeline.py:507](../../../tests/test_pipeline.py#L507) |
| `test_release_is_idempotent` | [tests/test_pipeline.py:526](../../../tests/test_pipeline.py#L526) |
| `test_append_adds_timestamp` | [tests/test_pipeline.py:539](../../../tests/test_pipeline.py#L539) |
| `test_server_ts_overwrites_caller_ts` | [tests/test_pipeline.py:548](../../../tests/test_pipeline.py#L548) |
| `test_torn_line_is_skipped` | [tests/test_pipeline.py:556](../../../tests/test_pipeline.py#L556) |
| `test_torn_multibyte_line_is_skipped` | [tests/test_pipeline.py:576](../../../tests/test_pipeline.py#L576) |
| `test_binary_garbage_line_is_skipped` | [tests/test_pipeline.py:602](../../../tests/test_pipeline.py#L602) |
| `test_non_utf8_pidfile_is_stale` | [tests/test_pipeline.py:618](../../../tests/test_pipeline.py#L618) |
| `test_blank_lines_skipped` | [tests/test_pipeline.py:629](../../../tests/test_pipeline.py#L629) |
| `test_tail_limit` | [tests/test_pipeline.py:638](../../../tests/test_pipeline.py#L638) |
| `test_concurrent_append_no_interleave` | [tests/test_pipeline.py:661](../../../tests/test_pipeline.py#L661) |
| `test_key_hashed_to_full_sha256` | [tests/test_pipeline.py:682](../../../tests/test_pipeline.py#L682) |
| `test_fs_unsafe_key_contained` | [tests/test_pipeline.py:698](../../../tests/test_pipeline.py#L698) |
| `test_meta_init_non_dict_rejected` | [tests/test_pipeline.py:718](../../../tests/test_pipeline.py#L718) |
| `test_unreadable_pidfile_returns_busy_not_break` | [tests/test_pipeline.py:733](../../../tests/test_pipeline.py#L733) |
| `test_meta_init_unserializable_rejected_without_side_effects` | [tests/test_pipeline.py:768](../../../tests/test_pipeline.py#L768) |
| `test_iter_events_drops_non_object_lines` | [tests/test_pipeline.py:790](../../../tests/test_pipeline.py#L790) |
| `test_rebuild_tolerates_non_dict_meta_init` | [tests/test_pipeline.py:811](../../../tests/test_pipeline.py#L811) |
| `test_stagestate_default_construction` | [tests/test_pipeline.py:825](../../../tests/test_pipeline.py#L825) |
| `test_meta_update_cannot_inject_reserved_event` | [tests/test_pipeline.py:836](../../../tests/test_pipeline.py#L836) |
| `test_meta_update_cannot_inject_ts` | [tests/test_pipeline.py:849](../../../tests/test_pipeline.py#L849) |
| `test_meta_update_event_key_always_wins` | [tests/test_pipeline.py:855](../../../tests/test_pipeline.py#L855) |
| `test_stage_name_rejects_path_traversal` | [tests/test_pipeline.py:883](../../../tests/test_pipeline.py#L883) |
| `test_stage_name_accepts_reasonable_values` | [tests/test_pipeline.py:892](../../../tests/test_pipeline.py#L892) |
| `test_concurrent_stale_break_no_overlap` | [tests/test_pipeline.py:899](../../../tests/test_pipeline.py#L899) |
| `test_empty_pidfile_never_breaks_live_lock` | [tests/test_pipeline.py:986](../../../tests/test_pipeline.py#L986) |
| `test_interrupted_event_precedes_new_start_in_log` | [tests/test_pipeline.py:1012](../../../tests/test_pipeline.py#L1012) |
| `test_release_race_does_not_log_spurious_interrupted` | [tests/test_pipeline.py:1055](../../../tests/test_pipeline.py#L1055) |
| `test_stage_and_key_isolation` | [tests/test_pipeline.py:1105](../../../tests/test_pipeline.py#L1105) |
| `test_prefilter_returns_top_k` | [tests/test_query_prefilter.py:41](../../../tests/test_query_prefilter.py#L41) |
| `test_prefilter_degenerate_query_falls_back` | [tests/test_query_prefilter.py:50](../../../tests/test_query_prefilter.py#L50) |
| `test_prefilter_empty_index` | [tests/test_query_prefilter.py:57](../../../tests/test_query_prefilter.py#L57) |
| `test_prefilter_top_k_zero_returns_empty` | [tests/test_query_prefilter.py:61](../../../tests/test_query_prefilter.py#L61) |
| `test_prefilter_no_matches_falls_back_to_slice` | [tests/test_query_prefilter.py:65](../../../tests/test_query_prefilter.py#L65) |
| `test_query_with_search_caps_prompt_for_large_index` | [tests/test_query_prefilter.py:76](../../../tests/test_query_prefilter.py#L76) |
| `test_promote_declined` | [tests/test_query_promote.py:21](../../../tests/test_query_promote.py#L21) |
| `test_promote_new_concept` | [tests/test_query_promote.py:48](../../../tests/test_query_promote.py#L48) |
| `test_promote_merges_into_existing` | [tests/test_query_promote.py:101](../../../tests/test_query_promote.py#L101) |
| `test_promote_invalid_json` | [tests/test_query_promote.py:153](../../../tests/test_query_promote.py#L153) |
| `test_promote_non_object_json` | [tests/test_query_promote.py:180](../../../tests/test_query_promote.py#L180) |
| `test_promote_merge_into_overrides_slug` | [tests/test_query_promote.py:202](../../../tests/test_query_promote.py#L202) |
| `test_promote_traversal_slug_rejected` | [tests/test_query_promote.py:249](../../../tests/test_query_promote.py#L249) |
| `test_mcp_kb_ask_promote_takes_job_lock` | [tests/test_query_promote.py:288](../../../tests/test_query_promote.py#L288) |
| `test_promote_missing_required_fields` | [tests/test_query_promote.py:314](../../../tests/test_query_promote.py#L314) |
| `test_build_aliases_from_articles` | [tests/test_resolve.py:8](../../../tests/test_resolve.py#L8) |
| `test_resolve_link_exact` | [tests/test_resolve.py:25](../../../tests/test_resolve.py#L25) |
| `test_resolve_link_not_found` | [tests/test_resolve.py:35](../../../tests/test_resolve.py#L35) |
| `test_resolve_link_fuzzy` | [tests/test_resolve.py:44](../../../tests/test_resolve.py#L44) |
| `test_save_load_aliases` | [tests/test_resolve.py:53](../../../tests/test_resolve.py#L53) |
| `test_resolve_with_simplified_traditional` | [tests/test_resolve.py:64](../../../tests/test_resolve.py#L64) |
| `test_search_raw_finds_verbatim` | [tests/test_search_raw.py:21](../../../tests/test_search_raw.py#L21) |
| `test_search_raw_empty_when_no_raw` | [tests/test_search_raw.py:37](../../../tests/test_search_raw.py#L37) |
| `test_search_raw_scrubs_local_paths` | [tests/test_search_raw.py:47](../../../tests/test_search_raw.py#L47) |
| `test_search_raw_top_k_edges` | [tests/test_search_raw.py:60](../../../tests/test_search_raw.py#L60) |
| `test_search_raw_no_absolute_paths_in_results` | [tests/test_search_raw.py:71](../../../tests/test_search_raw.py#L71) |
| `test_search_raw_ranks_by_tf_idf` | [tests/test_search_raw.py:82](../../../tests/test_search_raw.py#L82) |
| `test_normalize_strips_invisibles_and_punct` | [tests/test_sections.py:20](../../../tests/test_sections.py#L20) |
| `test_normalize_preserves_cjk_kana_alnum` | [tests/test_sections.py:27](../../../tests/test_sections.py#L27) |
| `test_normalize_empty_title_safely` | [tests/test_sections.py:32](../../../tests/test_sections.py#L32) |
| `test_anchor_format_and_components` | [tests/test_sections.py:41](../../../tests/test_sections.py#L41) |
| `test_anchor_stable_across_whitespace_and_punct_noise` | [tests/test_sections.py:49](../../../tests/test_sections.py#L49) |
| `test_anchor_changes_when_title_word_changes` | [tests/test_sections.py:57](../../../tests/test_sections.py#L57) |
| `test_anchor_path_distinguishes_same_title_under_different_parents` | [tests/test_sections.py:64](../../../tests/test_sections.py#L64) |
| `test_anchor_independent_of_sibling_position` | [tests/test_sections.py:71](../../../tests/test_sections.py#L71) |
| `test_anchor_empty_title_falls_back` | [tests/test_sections.py:78](../../../tests/test_sections.py#L78) |
| `test_anchor_truncates_long_slug` | [tests/test_sections.py:83](../../../tests/test_sections.py#L83) |
| `test_parse_empty_body` | [tests/test_sections.py:94](../../../tests/test_sections.py#L94) |
| `test_parse_no_headings` | [tests/test_sections.py:98](../../../tests/test_sections.py#L98) |
| `test_parse_single_heading_spans_to_end` | [tests/test_sections.py:102](../../../tests/test_sections.py#L102) |
| `test_parse_nested_tree_shape` | [tests/test_sections.py:113](../../../tests/test_sections.py#L113) |
| `test_parse_skipped_levels_still_nest_correctly` | [tests/test_sections.py:133](../../../tests/test_sections.py#L133) |
| `test_parse_eight_layer_deep_nesting` | [tests/test_sections.py:141](../../../tests/test_sections.py#L141) |
| `test_parse_skips_headings_in_fenced_code` | [tests/test_sections.py:157](../../../tests/test_sections.py#L157) |
| `test_parse_skips_headings_in_tilde_fences` | [tests/test_sections.py:169](../../../tests/test_sections.py#L169) |
| `test_parse_fence_with_longer_opener_requires_matching_close` | [tests/test_sections.py:175](../../../tests/test_sections.py#L175) |
| `test_parse_preserves_csharp_in_title` | [tests/test_sections.py:184](../../../tests/test_sections.py#L184) |
| `test_parse_ignores_4_space_indented_pseudo_heading` | [tests/test_sections.py:192](../../../tests/test_sections.py#L192) |
| `test_parse_3_space_indent_still_heading` | [tests/test_sections.py:199](../../../tests/test_sections.py#L199) |
| `test_parse_collision_suffixed` | [tests/test_sections.py:206](../../../tests/test_sections.py#L206) |
| `test_parse_subtree_slice_is_self_contained` | [tests/test_sections.py:219](../../../tests/test_sections.py#L219) |
| `test_find_section_walks_tree` | [tests/test_sections.py:232](../../../tests/test_sections.py#L232) |
| `test_find_section_returns_none_when_missing` | [tests/test_sections.py:241](../../../tests/test_sections.py#L241) |
| `test_extract_section_text_returns_subtree` | [tests/test_sections.py:246](../../../tests/test_sections.py#L246) |
| `test_extract_section_text_unknown_anchor_returns_none` | [tests/test_sections.py:257](../../../tests/test_sections.py#L257) |
| `test_op_kb_get_sections_returns_tree` | [tests/test_sections.py:291](../../../tests/test_sections.py#L291) |
| `test_op_kb_get_sections_missing_article` | [tests/test_sections.py:305](../../../tests/test_sections.py#L305) |
| `test_op_kb_get_with_section_returns_subtree` | [tests/test_sections.py:310](../../../tests/test_sections.py#L310) |
| `test_op_kb_get_unknown_section_flag` | [tests/test_sections.py:324](../../../tests/test_sections.py#L324) |
| `test_op_kb_get_no_section_unchanged` | [tests/test_sections.py:331](../../../tests/test_sections.py#L331) |
| `test_http_sections_returns_tree` | [tests/test_sections.py:351](../../../tests/test_sections.py#L351) |
| `test_http_sections_404_unknown` | [tests/test_sections.py:361](../../../tests/test_sections.py#L361) |
| `test_http_sections_path_traversal_blocked` | [tests/test_sections.py:366](../../../tests/test_sections.py#L366) |
| `test_empty_body_returns_empty` | [tests/test_split.py:21](../../../tests/test_split.py#L21) |
| `test_no_headings_returns_empty` | [tests/test_split.py:25](../../../tests/test_split.py#L25) |
| `test_no_target_level_returns_empty` | [tests/test_split.py:29](../../../tests/test_split.py#L29) |
| `test_invalid_level_raises` | [tests/test_split.py:39](../../../tests/test_split.py#L39) |
| `test_single_heading_yields_one_section` | [tests/test_split.py:47](../../../tests/test_split.py#L47) |
| `test_single_heading_last_line_no_newline` | [tests/test_split.py:60](../../../tests/test_split.py#L60) |
| `test_two_same_level_sections` | [tests/test_split.py:71](../../../tests/test_split.py#L71) |
| `test_section_offsets_cover_body_contiguously` | [tests/test_split.py:82](../../../tests/test_split.py#L82) |
| `test_deeper_headings_stay_in_content` | [tests/test_split.py:95](../../../tests/test_split.py#L95) |
| `test_higher_level_ends_section` | [tests/test_split.py:106](../../../tests/test_split.py#L106) |
| `test_preface_before_first_heading_not_in_result` | [tests/test_split.py:121](../../../tests/test_split.py#L121) |
| `test_fence_protects_pseudo_heading` | [tests/test_split.py:133](../../../tests/test_split.py#L133) |
| `test_tilde_fence_also_respected` | [tests/test_split.py:142](../../../tests/test_split.py#L142) |
| `test_three_space_indent_heading_recognized` | [tests/test_split.py:151](../../../tests/test_split.py#L151) |
| `test_four_space_indent_not_heading` | [tests/test_split.py:158](../../../tests/test_split.py#L158) |
| `test_crlf_body` | [tests/test_split.py:170](../../../tests/test_split.py#L170) |
| `test_cr_only_body` | [tests/test_split.py:180](../../../tests/test_split.py#L180) |
| `test_content_leading_trailing_blank_lines_stripped` | [tests/test_split.py:195](../../../tests/test_split.py#L195) |
| `test_content_preserves_internal_blank_lines` | [tests/test_split.py:201](../../../tests/test_split.py#L201) |
| `test_splits_at_requested_level` | [tests/test_split.py:211](../../../tests/test_split.py#L211) |
| `test_taixu_style_bian_with_chapters` | [tests/test_split.py:221](../../../tests/test_split.py#L221) |
| `test_section_is_dataclass` | [tests/test_split.py:251](../../../tests/test_split.py#L251) |
| `test_taixu_fixture_split_counts` | [tests/test_split.py:275](../../../tests/test_split.py#L275) |
| `test_taixu_fixture_sanming_lun_h2_titles` | [tests/test_split.py:281](../../../tests/test_split.py#L281) |
| `test_taixu_fixture_duplicate_chapter_numbers_preserved` | [tests/test_split.py:289](../../../tests/test_split.py#L289) |
| `test_taixu_fixture_xinjing_no_h1_in_body` | [tests/test_split.py:300](../../../tests/test_split.py#L300) |
| `test_strip_surrogates_passes_clean_text` | [tests/test_v064_features.py:13](../../../tests/test_v064_features.py#L13) |
| `test_strip_surrogates_replaces_lone_low_surrogate` | [tests/test_v064_features.py:18](../../../tests/test_v064_features.py#L18) |
| `test_strip_surrogates_replaces_high_surrogate` | [tests/test_v064_features.py:28](../../../tests/test_v064_features.py#L28) |
| `test_strip_surrogates_handles_non_string` | [tests/test_v064_features.py:35](../../../tests/test_v064_features.py#L35) |
| `test_chat_with_context_sanitizes_payload` | [tests/test_v064_features.py:40](../../../tests/test_v064_features.py#L40) |
| `test_query_passes_model_through` | [tests/test_v064_features.py:64](../../../tests/test_v064_features.py#L64) |
| `test_query_with_search_passes_model_through` | [tests/test_v064_features.py:81](../../../tests/test_v064_features.py#L81) |
| `test_query_default_model_is_none_when_omitted` | [tests/test_v064_features.py:113](../../../tests/test_v064_features.py#L113) |
| `test_api_ask_shallow_passes_model` | [tests/test_v064_features.py:139](../../../tests/test_v064_features.py#L139) |
| `test_api_ask_deep_passes_model` | [tests/test_v064_features.py:153](../../../tests/test_v064_features.py#L153) |
| `test_api_ask_omitted_model_not_in_dispatch_args` | [tests/test_v064_features.py:168](../../../tests/test_v064_features.py#L168) |
| `test_api_ask_rejects_non_string_model` | [tests/test_v064_features.py:184](../../../tests/test_v064_features.py#L184) |
| `test_api_ask_rejects_oversized_model` | [tests/test_v064_features.py:190](../../../tests/test_v064_features.py#L190) |
| `test_api_ask_empty_string_model_treated_as_none` | [tests/test_v064_features.py:196](../../../tests/test_v064_features.py#L196) |
| `test_kb_ask_operation_declares_model_param` | [tests/test_v064_features.py:214](../../../tests/test_v064_features.py#L214) |
| `test_ingest_url_sanitizes_content` | [tests/test_v064_features.py:223](../../../tests/test_v064_features.py#L223) |
| `test_ingest_file_sanitizes_content` | [tests/test_v064_features.py:246](../../../tests/test_v064_features.py#L246) |
| `test_api_ask_model_override_blocked_when_api_secret_set` | [tests/test_v064_features.py:282](../../../tests/test_v064_features.py#L282) |
| `test_api_ask_model_override_allowed_when_authed` | [tests/test_v064_features.py:290](../../../tests/test_v064_features.py#L290) |
| `test_api_ask_model_override_rejects_spa_cookie` | [tests/test_v064_features.py:310](../../../tests/test_v064_features.py#L310) |
| `test_api_ask_promote_still_accepts_cookie` | [tests/test_v064_features.py:328](../../../tests/test_v064_features.py#L328) |
| `test_api_ask_model_allowlist_rejects_unlisted` | [tests/test_v064_features.py:345](../../../tests/test_v064_features.py#L345) |
| `test_api_ask_model_allowlist_accepts_listed` | [tests/test_v064_features.py:352](../../../tests/test_v064_features.py#L352) |
| `test_sanitize_slug_strips_url_punctuation` | [tests/test_v064_features.py:371](../../../tests/test_v064_features.py#L371) |
| `test_heal_urly_slugs_renames_dirty_files` | [tests/test_v064_features.py:380](../../../tests/test_v064_features.py#L380) |
| `test_http_timeout_env_overrides_default` | [tests/test_v064_features.py:420](../../../tests/test_v064_features.py#L420) |
| `test_worker_status_requires_auth_when_secret_set` | [tests/test_v064_features.py:436](../../../tests/test_v064_features.py#L436) |
| `test_worker_status_reflects_lock_state` | [tests/test_v064_features.py:451](../../../tests/test_v064_features.py#L451) |
| `test_http_timeout_env_invalid_falls_back` | [tests/test_v064_features.py:478](../../../tests/test_v064_features.py#L478) |
| `test_backward_compat_no_query` | [tests/test_web_articles.py:55](../../../tests/test_web_articles.py#L55) |
| `test_articles_pagination` | [tests/test_web_articles.py:64](../../../tests/test_web_articles.py#L64) |
| `test_next_cursor_null_on_exact_last_page` | [tests/test_web_articles.py:81](../../../tests/test_web_articles.py#L81) |
| `test_articles_tag_filter` | [tests/test_web_articles.py:90](../../../tests/test_web_articles.py#L90) |
| `test_articles_q_filter` | [tests/test_web_articles.py:99](../../../tests/test_web_articles.py#L99) |
| `test_articles_fields_selector` | [tests/test_web_articles.py:106](../../../tests/test_web_articles.py#L106) |
| `test_articles_limit_bounds` | [tests/test_web_articles.py:113](../../../tests/test_web_articles.py#L113) |
| `test_bad_limit_beats_etag` | [tests/test_web_articles.py:119](../../../tests/test_web_articles.py#L119) |
| `test_articles_lite_requires_index` | [tests/test_web_articles.py:126](../../../tests/test_web_articles.py#L126) |
| `test_articles_etag_304` | [tests/test_web_articles.py:140](../../../tests/test_web_articles.py#L140) |
| `test_articles_etag_invalidates_on_rename` | [tests/test_web_articles.py:153](../../../tests/test_web_articles.py#L153) |
| `test_articles_etag_distinct_for_query` | [tests/test_web_articles.py:170](../../../tests/test_web_articles.py#L170) |
| `test_tag_filter_no_substring_match` | [tests/test_web_articles.py:177](../../../tests/test_web_articles.py#L177) |
| `test_etag_wildcard_and_weak_strong` | [tests/test_web_articles.py:197](../../../tests/test_web_articles.py#L197) |
| `test_lite_route_not_shadowed_by_path_slug` | [tests/test_web_articles.py:212](../../../tests/test_web_articles.py#L212) |
| `test_lite_tag_filter_narrows` | [tests/test_web_articles.py:225](../../../tests/test_web_articles.py#L225) |
| `test_lite_tag_unknown_returns_empty_200` | [tests/test_web_articles.py:237](../../../tests/test_web_articles.py#L237) |
| `test_lite_no_tag_returns_full_list` | [tests/test_web_articles.py:249](../../../tests/test_web_articles.py#L249) |
| `test_lite_etag_distinct_per_tag` | [tests/test_web_articles.py:258](../../../tests/test_web_articles.py#L258) |
| `test_lite_etag_304_respects_tag` | [tests/test_web_articles.py:269](../../../tests/test_web_articles.py#L269) |
| `test_lite_cache_default_no_cache` | [tests/test_web_articles.py:281](../../../tests/test_web_articles.py#L281) |
| `test_lite_cache_max_age_env_opts_in` | [tests/test_web_articles.py:288](../../../tests/test_web_articles.py#L288) |
| `test_lite_cache_max_age_applies_to_304` | [tests/test_web_articles.py:300](../../../tests/test_web_articles.py#L300) |
| `test_lite_cache_invalid_env_falls_back` | [tests/test_web_articles.py:311](../../../tests/test_web_articles.py#L311) |
| `test_lite_cache_zero_env_means_no_cache` | [tests/test_web_articles.py:322](../../../tests/test_web_articles.py#L322) |
| `test_lite_tag_normalizes_string_tags` | [tests/test_web_articles.py:331](../../../tests/test_web_articles.py#L331) |
| `test_api_languages_factory_profile` | [tests/test_web_languages.py:17](../../../tests/test_web_languages.py#L17) |
| `test_taxonomy_lang_defaults_and_rejects_invalid` | [tests/test_web_languages.py:28](../../../tests/test_web_languages.py#L28) |
| `test_invalid_languages_config_returns_json_500` | [tests/test_web_languages.py:39](../../../tests/test_web_languages.py#L39) |
| `test_xici_accepts_profile_view_lang` | [tests/test_web_languages.py:60](../../../tests/test_web_languages.py#L60) |
| `test_xici_generate_rejects_profile_lang_without_xici_style` | [tests/test_web_languages.py:68](../../../tests/test_web_languages.py#L68) |
| `test_xici_generate_accepts_factory_xici_lang` | [tests/test_web_languages.py:99](../../../tests/test_web_languages.py#L99) |
| `test_import_time_patch_reflected_over_http` | [tests/test_web_languages.py:121](../../../tests/test_web_languages.py#L121) |
| `test_get_client_default_is_singleton` | [tests/test_xllm_key.py:28](../../../tests/test_xllm_key.py#L28) |
| `test_get_client_with_api_key_is_not_cached` | [tests/test_xllm_key.py:38](../../../tests/test_xllm_key.py#L38) |
| `test_chat_forwards_api_key_to_call_llm` | [tests/test_xllm_key.py:58](../../../tests/test_xllm_key.py#L58) |
| `test_chat_default_api_key_is_none` | [tests/test_xllm_key.py:72](../../../tests/test_xllm_key.py#L72) |
| `test_chat_with_context_forwards_api_key` | [tests/test_xllm_key.py:86](../../../tests/test_xllm_key.py#L86) |
| `test_query_passes_api_key_through` | [tests/test_xllm_key.py:104](../../../tests/test_xllm_key.py#L104) |
| `test_query_with_search_passes_api_key_to_selector_and_answer` | [tests/test_xllm_key.py:119](../../../tests/test_xllm_key.py#L119) |
| `test_kb_ask_schema_declares_api_key_writeonly` | [tests/test_xllm_key.py:151](../../../tests/test_xllm_key.py#L151) |
| `test_op_ask_forwards_api_key` | [tests/test_xllm_key.py:164](../../../tests/test_xllm_key.py#L164) |
| `test_api_ask_rejects_api_key_in_body_toplevel` | [tests/test_xllm_key.py:207](../../../tests/test_xllm_key.py#L207) |
| `test_api_ask_rejects_api_key_in_nested_body` | [tests/test_xllm_key.py:221](../../../tests/test_xllm_key.py#L221) |
| `test_api_ask_rejects_api_key_in_list_body` | [tests/test_xllm_key.py:234](../../../tests/test_xllm_key.py#L234) |
| `test_api_ask_rejects_api_key_at_arbitrary_depth` | [tests/test_xllm_key.py:245](../../../tests/test_xllm_key.py#L245) |
| `test_api_ask_non_bearer_auth_rejected_for_x_llm_key` | [tests/test_xllm_key.py:263](../../../tests/test_xllm_key.py#L263) |
| `test_api_ask_non_bearer_auth_rejected_for_model_override` | [tests/test_xllm_key.py:281](../../../tests/test_xllm_key.py#L281) |
| `test_api_ask_x_llm_key_honored_in_local_dev` | [tests/test_xllm_key.py:293](../../../tests/test_xllm_key.py#L293) |
| `test_api_ask_x_llm_key_requires_auth_when_secret_set` | [tests/test_xllm_key.py:308](../../../tests/test_xllm_key.py#L308) |
| `test_api_ask_x_llm_key_accepted_with_strong_auth` | [tests/test_xllm_key.py:320](../../../tests/test_xllm_key.py#L320) |
| `test_api_ask_cookie_auth_insufficient_for_x_llm_key` | [tests/test_xllm_key.py:337](../../../tests/test_xllm_key.py#L337) |
| `test_api_ask_deep_path_forwards_api_key` | [tests/test_xllm_key.py:353](../../../tests/test_xllm_key.py#L353) |
| `test_api_ask_empty_x_llm_key_header_treated_as_none` | [tests/test_xllm_key.py:368](../../../tests/test_xllm_key.py#L368) |
| `test_api_ask_oversized_x_llm_key_rejected` | [tests/test_xllm_key.py:382](../../../tests/test_xllm_key.py#L382) |
| `test_api_ask_response_does_not_echo_key` | [tests/test_xllm_key.py:395](../../../tests/test_xllm_key.py#L395) |
| `test_api_ask_logs_do_not_contain_key` | [tests/test_xllm_key.py:416](../../../tests/test_xllm_key.py#L416) |
| `test_chat_redacts_key_in_error` | [tests/test_xllm_key.py:436](../../../tests/test_xllm_key.py#L436) |
