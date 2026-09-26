# HTTP ingestion and reference descriptors

`POST /api/ingest` 在 Web／Agent 兩 factory 均委派 `Gateway::legacy(kb_ingest)`，
保持 `{status:ok,...result}`。來源限有效 HTTP URL 或已在本 library 內之路徑；
解析、scope、原件保存與接受寫入均由 core 操作負責。

Web `POST /api/upload` 保 multipart `file` 欄位。Markdown／可接受 UTF-8 文字
交 `ingest_text`，回 `{status:ok,path,filename}`；PDF 交明確配置的 `pdf` process
adapter，`chunk_pages=20`，只以真 accepted documents 計 `chunks`。缺 parser
即明確 capability error。空檔名／缺 file 保舊 400 message；拒目錄檔名、
多 file 欄位、無效 UTF-8，以及未獲 write/library capability 的請求。

每檔解碼後上限 32 MiB；multipart envelope 額外容許 64 KiB。PDF process
protocol 另有 **16 MiB serialized JSON input** 上限，故二進位數字陣列
有效上限較低；超限精確回 `adapter input exceeds 16 MiB`，不啟 parser、不寫
source。16 MiB 回應上限仍由 operator adapter 配置約束。不可將可上傳
32 MiB 誤作每種 parser 都能接受 32 MiB。

`GET /api/refs/plugins` 回原三個 builtin reference provider 的 exact
`{id,name:{en,zh,ja}}` metadata：CBETA、Wikisource、ctext。此乃獨立 reference
descriptor 契約，不列 process ingestion adapters，也不聲稱三者具全文 search
能力。動態 Python plugin discovery 仍由 facade host 負責。

[`http_ingest_contracts.rs`](../../crates/llmbase-cli/tests/http_ingest_contracts.rs)
5 組測試覆 >1 MiB multipart、CRLF／CJK／frontmatter bytes、重複入庫、scope／
檔名／UTF-8 guard、Web／Agent ingest envelope、真 process protocol 與 original
asset，以及 serialized PDF 超限無副作用。2026-09-20
`.rust-migration-runs/http-ingest-tests-v2.log`：5／5 passed；同輪 HTTP model6、
read3、write1、static3、transports8、repair13 全過。
