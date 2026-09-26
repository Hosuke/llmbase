# 固定多語檢索集 v1

人工合成 30 個文獻主題，各有 raw 與 compiled 文，合 60 檔。英、中、日各 10 主題；無模型生成／embedding 參與 relevance 判定。文言無空格、繁簡、非 BMP、組合字、題名、metadata alias、章名、原句、同詞異義皆具。

`queries.json` 有 6 題 tuning、30 題 held_out。先寫文獻與人工二元相關性，再於任何評分前寫 `manifest.json` 鎖 SHA-256。評測器拒 hash 變動，且不得自動更新 manifest。

raw/compiled 各評相同 36 題，分開報告；不混作跨庫檢索。`bank` 題有兩個相關文獻，因此 Recall@5 計召回比例，非有一命中便計 1。MRR@10 只取首個相關文獻。多 passage 命中以 document id 去重，保首現次序；仍只向 backend 要 10 個 hits，不偷偷過取補滿 10 文獻。

正文與章名有意有少量交集；raw 保較完整上下文，compiled 為短條。`aliases` 只在 metadata，舊搜尋未必收；缺失照錄，不能補進 body 以求 baseline 好看。森林條目為干擾文獻，無 held-out 專題。此規模只作工程回歸，不能推稱代表全庫真實檢索品質。

執行 `scripts/rust-migration/evaluate_retrieval.py`，先驗 manifest，再於 temporary library 機械生成 Markdown。每篇都帶唯一來源 URL、chapter、alias、未知欄及 deterministic timestamps。評測不改 fixture、不需真實憑證、不召模型。

`expected quote` 只供驗引文回查；來源字串存在與答案推論成立是兩事。本 lexical 評測不判生成答案正確，mock/真實 embedding 品質亦不計於此。
