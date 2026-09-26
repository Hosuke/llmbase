# 前端基線、最小修正與驗證

2026-09-20：由 pristine Git `HEAD` 獨立重現前端既存 TypeScript 錯誤，再於
Rust migration worktree 作最小修正。此驗證確認型別檢查及打包可行；不等同
Rust HTTP／SPA 功能已全部相容。完整 API 與語言契約仍以
[parity.md](parity.md) 逐項驗收。

## 基線來源與隔離

- Git commit：`a7c4d9da764462e09c0769ecd8ba548a46d27e83`。
- 遷移 worktree：`/Users/lanchair/Developer/LLM-infra/Github/llmbase-rust`。
- 修正前 `git diff -- frontend` 為空。
- 以 `git archive HEAD frontend` 匯出至
  `/tmp/llmbase-frontend-pristine.ylmBzU/frontend.tar`，解於同目錄；該副本不引用
  worktree 的 `node_modules`，在其 `frontend/` 獨立執行 `npm ci`。
- 原 `llmbase` worktree、下游 Siwen、任何 production runtime 均未改動。
- Node `v22.23.1`、npm `10.9.8`；實際工具根目錄
  `/Users/lanchair/.local/lib/node-v22.23.1-darwin-arm64/bin`。
- tmux session：基線 `llmbase-frontend-pristine`，修正驗證
  `llmbase-frontend-fixed`；兩者均已正常結束，未啟動 dev/preview server。

基線操作（在隔離副本 `frontend/` 執行，工具目錄置於 `PATH` 最前）：

```sh
npm ci --no-audit --no-fund
npx tsc -p tsconfig.app.json --noEmit
npm run build
```

安裝 exit **0**；明確指向 app tsconfig 的型別檢查 exit **2**；build
（`tsc -b && vite build`）exit **2**。兩次檢查均見下列同一組九項頂層診斷，
與先前 `.rust-migration-runs/frontend-baseline.log` 一致。

| 原檔案位置 | 診斷 | 原因及最小修正 |
| --- | --- | --- |
| `src/lib/branding.ts:29:7` | TS2322：`Branding \| null` 不可指定為 `Branding` | 將合併結果賦予已具 `Branding` 型別的 local，再存快取並回傳；預設值及合併次序不變 |
| `src/pages/Explore.tsx:274:38` | TS6133：`event` 未使用 | 參數改名 `_event`，保留 D3 callback 的 datum 位置 |
| `src/pages/Graph.tsx:180:13` | TS2345：`DragBehavior<SVGCircleElement, Node, Node \| SubjectPosition>` 不符合含 `BaseType` 的 selection | `selectAll<SVGCircleElement, Node>('circle')` 指明實際 element/datum，毋須 cast 或關閉型別檢查 |
| `src/pages/Health.tsx:62:23` | TS2339：`{ fixes: string[]; fix_count: number }` 無 `message` | `api.lintFix` 回應改為同步 fixes／非同步 started 的 union，Health 原有兩條分支保留 |
| `src/pages/Search.tsx:4:1` | TS6133：`Tag` 未使用 | 刪未用 import |
| `src/pages/Trails.tsx:5:19` | TS6133：`localizeTitle` 未使用 | 刪未用 import |
| `src/pages/Wiki.tsx:9:9` | TS6133：`navigate` 未使用 | 刪未用 hook 與 import |
| `src/pages/Wiki.tsx:15:10` | TS6133：`viewMode` 未使用 | 刪 UI 未引用的 state |
| `src/pages/Wiki.tsx:15:20` | TS6133：`setViewMode` 未使用 | 同上 |

`lintFix` 的非同步分支有實作依據：
[`llmwiki/web.py`](../../llmwiki/web.py) 的 `api_lint_fix` 回傳
`{status: "started", message: ...}`；已完成工作另存 `last_fix.json`。原前端
早已處理 background message，僅 API 型別漏此形態。此處不把 `started` 當
`completed`，亦不引入新的 job 假成功語义。

## 修正前 SHA-256

下列各檔均由隔離 pristine 副本計算，並逐位元確認與修正前 migration
worktree 相同。package、lockfile、tsconfig 未改。

| 相對 `frontend/` 路徑 | SHA-256 |
| --- | --- |
| `package.json` | `ffbcd677200097522915b0e4961bbc82d4fe3a9f171419c0ae5410f5c03e3047` |
| `package-lock.json` | `32d8dd4d86eaa81a1b4b4794504288f66d70fdd6e6bb1d7628e2da12e874467d` |
| `tsconfig.app.json` | `aa9e68711a716c499b5730aa6664cab7e6a2cb65e9924252cefe2135a4724112` |
| `src/lib/branding.ts` | `592bac63b892f860facad42e42ff9e59f28f6d48545dc82eb254b909c5451c16` |
| `src/lib/api.ts` | `5b8d2b1e7ec3cca875ac801fa32a24a8f486265847c1018842d094c95545ea17` |
| `src/pages/Explore.tsx` | `1df6955923cac1a6467f7d5d20f6424d6a91a71e394bc898002b1310696c3ce4` |
| `src/pages/Graph.tsx` | `eaf1b2dd6960bdc60c5300a35cd148f6822bf8fb429f886ced255ca59bfdc3da` |
| `src/pages/Health.tsx` | `149b45983e74431a0ef1eeb78ab2f30f0165d305d5f3436c6659fbe6696a677a` |
| `src/pages/Search.tsx` | `2579529bdac17b7ced716fc4e0af4a31bee11d427c0a5db93d8791f6df2ad8ef` |
| `src/pages/Trails.tsx` | `e53b6d606b0742af5236f5e51181ebed320715153f044f24b746f030739c4ceb` |
| `src/pages/Wiki.tsx` | `84a6bf2629f62310575ce0b047c178701a7f04162620f90979e7f43a7f4fe9ce` |

留存本機證據：

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/llmbase-frontend-pristine.ylmBzU/frontend.tar` | `d8684765d66ab0517681f65e20e850d581b83fe9b55476bc6c21dacdb0cd1643` |
| `/tmp/llmbase-frontend-pristine.ylmBzU/baseline.log` | `8454e453f06095e19a7be5ee2715fa61c919aba8de43a59cc2023d8535b631e4` |
| `/tmp/llmbase-frontend-fixed.log` | `33fe2323298ea5f4dc35819d383798065c9e80d30c77d6e410ce6737692d7507` |

## 修正後結果

在 migration worktree 的 `frontend/`，同一 Node/npm 工具鏈執行：

| 檢查 | 結果 |
| --- | --- |
| `npx tsc -p tsconfig.app.json --noEmit` | exit **0** |
| `npm run build`（`tsc -b && vite build`） | exit **0**；Vite 8.0.3，2066 modules |
| `git diff --check` | exit **0** |

變更限七個前端 source files，共 12 insertions、10 deletions；未改依賴、
嚴格型別選項、語言 profile、API 路徑、UI layout 或 style。正常 build output
位於 ignored `static/dist/`，未提交生成 bundle。現有 bundle 的大型 chunk
提示仍在：主 chunk 約 541.50 kB（gzip 168.33 kB）；此非建置失敗，未藉本次
型別修正擴作拆包或重新設計。

此次改動由 TypeScript 及正式 build 驗證；未聲稱已完成原生 HTTP 的瀏覽器
端 smoke、下游 Siwen SPA 對接或全部功能 parity。
