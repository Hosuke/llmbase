# 固定多語 lexical 基線 v1

2026-09-20（Asia/Shanghai）建立；原始測量時間為 `2026-09-19T16:58:52Z`。先鎖 fixture/relevance，再實跑舊演算法；未據新 Rust 排名改題。此為小型工程回歸集，不推稱代表真文庫全體品質。

## 語料、分組與封存

`tests/rust_migration/fixtures/retrieval/corpus.json` 有 30 主題，英、中、日各 10；每題各具 raw 原文與 compiled 短條，機械生為 60 個 Markdown 文件。含文言无空格、繁簡、metadata aliases、題名／章名、原句、非 BMP `𠮷`、組合字 `é`，以及 bank、Mercury、空之歧義。無私密資料或遠端文庫副本。

人工定 36 題之二元 relevance：6 題 `tuning`，30 題 `held_out`。不曾向被測模型詢判相關性。`bank` 有兩篇相關文獻，其餘各一篇。預期引文另列，不送生成模型。

封存於任何評分前，SHA-256 如下：

| 檔案 | SHA-256 |
| --- | --- |
| `corpus.json` | `ee720478670f57133ef30d858c9e88ac872b85e34b4c5531c5e959661b0ea6e2` |
| `queries.json` | `9c5978f3abe73be4057c89db4b8428fc5795e8b762041370a44182a6c8ae4f06` |
| `manifest.json` | `cf322ba77a72530864c52768f5deb49e3dc2ddc3ade2b40bc6071c1e77dd6f3e` |

評測器先驗 `manifest.json` 所列 corpus/query hashes，拒不符；不會自動重寫 manifest。後續禁止為提高新版分數改 relevance。確證測試本身錯誤時須另版並存原因，保留本版原始結果。

## 真正舊路徑

評測器 `scripts/rust-migration/evaluate_retrieval.py` 明載原倉 `/Users/lanchair/Developer/LLM-infra/Github/llmbase`，核實 `llmwiki.search.__file__`，驗 `SEARCH_TOKENIZER is None`，並逐檔核未改之來源 hash：

| 舊模塊 | SHA-256 |
| --- | --- |
| `llmwiki/search.py` | `b51e41ed30e92e3a9dadc23f26be80b535da603b5beb822a4f22d9390ad814db` |
| `llmwiki/config.py` | `5fb59b498dd049f80e4da476a94053c6bb850d807f2c55587cf0a7c6431359ce` |

raw scope 真呼 `search_raw(query, top_k=10, base_dir=temporary_fixture)`；compiled scope 真呼 `search(...)`。沒有手寫一套「等價舊 scorer」，沒有 mock 搜尋，沒有模型呼叫。全部 input file hashes 於測前、測後一致。

## 指標與結果

以 document 為評分單位。Recall@5 = 前五文獻中相關數／所有人工相關數；MRR@10 = 前十文獻首個相關位置之倒數，未命中為 0。每 query 各一首次測量、三次 warm 重跑；warm 排序須悉同首次。

| 舊 backend | split | 題數 | Recall@5 | MRR@10 | 首輪 p50/p95 ms | warm p50/p95 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `search_raw` | held_out | 30 | 1.000 | 1.000 | 16.765 / 24.845 | 16.186 / 32.139 |
| `search` | held_out | 30 | 1.000 | 1.000 | 10.466 / 15.743 | 10.200 / 17.053 |
| `search_raw` | tuning | 6 | 1.000 | 1.000 | 14.507 / 16.469 | 14.927 / 17.558 |
| `search` | tuning | 6 | 1.000 | 1.000 | 12.366 / 31.472 | 11.846 / 31.779 |

必需 cases 無失。此全集甚小，分數满不等於各種字形映射與 alias 均獨立正確；部分題可憑共享字或同詞命中。別有精確 alias、字形及 scope 單元／端到端測試方足證相關能力。後續若欲增難度，另建新集，不改本基線。

當次 process peak RSS 為 28,311,552 bytes；import/setup 15.874 ms。舊 lexical 無持久索引，`index_bytes=0` 只謂沒有落盤索引，並非記憶體零成本。無 embedding、model calls 0、模型耗時 0。未 flush OS page cache，故所列為 first-pass/warm，**真正 cold-cache latency 未驗**。RSS 含 Python runtime，不能當純搜尋記憶體。無 Rust 同機同接口測量前，不作速度倍率聲稱。

舊搜尋不回 evidence object，**舊版 citation resolution rate 無可比值**。預期 quote 確實存在於固定來源／條文，可驗資料；這不等於舊版能回查結構化引文，更不證答案推論成立。

原始結果：`docs/rust-migration/results/retrieval-legacy-v1.json`。含全部 72 個 `(scope, query)` 之 ranked ids、原始 hits、分數、逐次 latency、expected quote 核查、執行環境、input hashes 及來源模塊 hashes。結果檔若已存在，評測器拒覆寫。

## 重跑與 native 比較

舊基線實跑命令（exit 0）：

```bash
/Users/lanchair/Developer/LLM-infra/Github/llmbase/.venv/bin/python -B scripts/rust-migration/evaluate_retrieval.py --backend legacy --legacy-root /Users/lanchair/Developer/LLM-infra/Github/llmbase --output docs/rust-migration/results/retrieval-legacy-v1.json
```

首次開發試跑因 macOS `/var` 與 `/private/var` 路徑 canonicalization 未統一而 exit 1，無完整結果檔；將 temporary base resolve 後重跑即成。未改 corpus、relevance 或舊演算法。

native adapter 已實跑（exit 0），同一封存 fixture 未改：

```bash
python -B scripts/rust-migration/evaluate_retrieval.py --backend native --native-bin /absolute/path/to/llmbase --compare docs/rust-migration/results/retrieval-legacy-v1.json --output docs/rust-migration/results/retrieval-native-v1.json
```

先調 `llmbase --base-dir FIXTURE --library-id retrieval-fixture-v1 --json op index --args '{}'`，再 `op list` 建 source id 映射；搜尋用 `op search --args '{"query":"…","kind":"raw|compiled","top_k":10}'`。stdout 須只含 application JSON。多 passage 以 source id／映射後 document id 去重，保首見位置；只要十個 passage，不以過取掩蓋 chunk 排名成本。

native 每個 hit 另驗 `library_id/source_id/content_hash/offset_unit=utf8_byte/start/end/exact_quote`，byte slice 必等 exact quote；再用 `op evidence` 真回查該 revision。不同的 evidence 各真調一次，重複者按完整物件快取驗證結果。此耗時不混入 search latency。

native 搜尋必 `mode="lexical"`，不能把 mock embeddings／hybrid 成績混入 baseline。`--compare` 驗相同 fixture hashes，held_out raw、compiled 的 Recall@5 與 MRR@10 各不得低於本表，且必需題需全召回；不符則輸出完整結果並 exit 2。native latency 包括每次新 CLI process 的啟動與 SQLite open，legacy 為 process 內呼叫；報告須明此不同，不能以此算 Rust 純核心加速倍數。

原始 native 收據為 `docs/rust-migration/results/retrieval-native-v1.json`，時間
`2026-09-19T17:07:20Z`，binary SHA-256
`8ba824a068df1c5bf37bc949ea2036338fc505679eb4f6dacb7828e1ef537c98`。

| Native lexical | split | 題數 | Recall@5 | MRR@10 | 首輪 p50/p95 ms | warm p50/p95 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| raw | held_out | 30 | 1.000 | 1.000 | 6.789 / 7.653 | 6.876 / 7.601 |
| compiled | held_out | 30 | 1.000 | 1.000 | 6.788 / 7.494 | 6.869 / 7.564 |
| raw | tuning | 6 | 1.000 | 1.000 | 6.570 / 7.189 | 6.634 / 7.153 |
| compiled | tuning | 6 | 1.000 | 1.000 | 6.556 / 6.910 | 6.844 / 7.329 |

`comparison_failures=[]`；新增 60 sources／60 passages，建索引 36.257 ms，
索引 339,968 bytes；60 份唯一 evidence 真回查悉成。測前後 source bytes 一致。
父評測 process peak RSS 58,736,640 bytes，child peak RSS 11,747,328 bytes，
不可直接等同 legacy in-process RSS 或 Rust 純演算法成本。此結果屬新 native FTS
操作，與 Python facade 的 `compat_search` 之舊 TF-IDF dialect 分列。

真 embedding／hybrid 語義品質及真正 cold-cache 測量仍未驗；本小集滿分亦不證其。

## Performance protocol v2

The original v1 result files and the fixed corpus/relevance remain unchanged. The v2
evaluator adds measurements that v1 did not distinguish: repeated fresh index
creation, CLI startup, no-op indexing, one-source incremental work, and the peak
RSS of each native child. Its JSON includes the evaluator hash, binary hash and
size, reported binary version, host, operating system, and caller-supplied build
label. It rejects an artifact that changes during the run and creates output files
exclusively, so an existing receipt cannot be overwritten.

The timing terms have deliberately narrow meanings:

| Measurement | What runs | What it does not establish |
| --- | --- | --- |
| Fresh index | Index 60 freshly materialized Markdown files in a new empty library per repetition | A cold operating-system page cache; materialization has just touched the source files |
| Startup | Launch the native binary with `--version` | Database opening or a persistent-server request cost |
| First-pass query | First execution of each fixed query against the indexed library | Cold disk or cold process caches across the whole benchmark |
| Warm query | Repeat the same query and verify identical rankings | A reused process; every query still starts a CLI process |
| Warm no-op index | Index the existing unchanged library once per repetition | A constant-time operation independent of library size |
| Incremental add/update/delete | Add, edit, then delete one synthetic raw source in a separate fixture copy | A large-library scaling result or a full production ingestion workload |

Every incremental phase checks the actual index report and confirms that the 60
original sources retain their content hashes and revisions. It verifies that the
new source becomes searchable and that deletion removes it from search. The
benchmark never edits the frozen corpus or the sources used for quality scoring.
Per-phase receipts include source byte counts, passages written, total index size,
SQLite/WAL/SHM sizes, and index growth after each operation. Deleted revisions may
remain stored for evidence history, so deletion need not shrink the database.

Each native command is measured from process launch through exit; JSON decoding,
fixture materialization, and post-operation validation are outside those child
timings. Query `first_pass_ms`/`warm_ms` additionally include the small Python
adapter overhead, preserving their original end-to-end meaning. CPU user/system
time, page faults, and peak RSS come from that child's `wait4` result, rather than
the largest child encountered earlier in the evaluator. Python documents this
per-child return contract in [`os.wait4`](https://docs.python.org/3.12/library/os.html#os.wait4).
RSS is normalized to bytes: the current macOS `man 2 getrusage` reports bytes;
Linux reports KiB in [`getrusage(2)`](https://www.man7.org/linux/man-pages/man2/getrusage.2.html).
The top-level Python and cumulative-child high-water marks remain available with
their broader scope; they are not the native search working set.

Each phase reports sample count, minimum, p50, nearest-rank p95, and maximum. A
20-repetition run supplies 20 fresh-index and incremental samples; the fixed 72
scope/query pairs each have five warm samples in the command below. These are
descriptive observations on a small fixture, without CPU pinning, filesystem
cache eviction, or statistical confidence intervals. Results from different
machines, interfaces, and build profiles do not support a Rust/Python speedup
ratio.

```bash
python -B scripts/rust-migration/evaluate_retrieval.py \
  --backend native --native-bin /absolute/path/to/latest/llmbase \
  --artifact-label 'remote release build; record its build receipt' \
  --compare docs/rust-migration/results/retrieval-legacy-v1.json \
  --repeats 5 --performance-repeats 20 \
  --output docs/rust-migration/results/retrieval-native-v2.json
```

This protocol measures lexical retrieval and exact evidence resolution. Model
requests and provider-request elapsed time are exactly zero because these
operations do not invoke a model. Actual provider service latency and semantic
quality are explicitly unevaluated; a zero request budget is not a measured
provider latency. `model_vs_local` separately records total quality-run wall time
and summed native child time, including indexing and evidence validation. Its
`local_search_wall_ms` and `local_search_native_child_wall_ms` isolate the fixed
queries and their warm repetitions from those setup and citation checks.

A short local end-to-end protocol self-check passed with one warm repetition and
one incremental cycle: all 72 scope/query pairs matched the frozen quality gate,
60 unique citations resolved, and all index reports, unrelated source revisions,
synthetic-source search/deletion, and scoped RSS checks passed. The subprocess
timeout guard also passed a separate short probe. The self-check log is
`.rust-migration-runs/retrieval-selfcheck-v2.log` (exit 0); its temporary result is
explicitly labelled `local-debug-harness-selfcheck-not-release-acceptance`. This
checks the harness; the latest remote release matrix remains the acceptance
measurement to record below.
