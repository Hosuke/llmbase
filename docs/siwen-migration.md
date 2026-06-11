# siwen 迁移说明：语言契约（languages profile）落地之后

> 本文是上游 llmbase「语言契约收敛」完成后，给 siwen 下一轮迁移的输入。
> 本轮**未改动 siwen 仓库**；以下全部是"新契约能让 siwen 删掉什么、必须注意什么"。

## 1. 上游新能力（本轮落地）

- `config.yaml` 新增 `languages:` 块（`default` + `profiles`），是语言结构的唯一真相源。
- 解析器 `llmwiki/config.py`：`get_language_profile(cfg)` / `get_section_headers(cfg)`，
  优先级：**import-time patch（`compile.SECTION_HEADERS` 赋值）> config `languages:` > 内建三语**。
  斯文现行 patch 路径零破坏，且 patch 生效时整条链路（compile/export/web/前端契约）跟着 patch 走。
- **单节空 header 一等公民**：`[("文言", "")]` 在 `_split_sections` / `_assemble_sections` /
  `_merge_into` 中按"整段裸正文 = 该节"处理（顺带修了空 header 正则匹配空行的潜在 bug）。
- 新公开端点 `GET /api/languages`：返回归一化 profile
  （`sections / views / codes / default_lang / api_default_lang / single_section / bare_body`）。
- `web.py` 的 `/api/taxonomy`、`/api/xici`、`/api/xici/generate` 的 `?lang=` 校验从契约派生；
  非法值返回 400 + 合法列表。**校验只对 config 显式定义的 profile 生效**：patch 路径
  （`_patched` profile）保持 legacy 行为——任意 lang 放行、默认 `zh`、worker 仍按
  legacy 四语循环。所以 siwen 现行部署（前端硬传 `lang=zh`）升级上游后零破坏。
- 上游前端 `lang.tsx` 不再硬编码语言枚举与 `## English/中文/日本語` 字面量，
  全部从 `/api/languages` 派生；`bare_body: true` 时**不做任何 section 切割，直接渲染裸正文**，
  `localizeTitle` 恒等返回，语言切换钮自动隐藏（`options.length <= 1`）。

## 2. siwen 用 config 表达文言单语（推荐写法）

```yaml
languages:
  default: wenyan
  profiles:
    wenyan:
      sections:
        - {code: zh, key: 文言, header: "", label: 文言, icon: 文}
```

**注意 `code: zh` 这个选择**：`code` 是自由字段。siwen 现有 patch 把 `xici.LANG_STYLES`
设为 `{"zh": ...}`，前端 fork 的 Dashboard 也硬传 `lang: 'zh'`。把 profile code 定为 `zh`
可让 `?lang=zh` 继续合法、`LANG_STYLES["zh"]` 继续命中，迁移期间无需同步改这些位置。
若想语义更干净用 `code: wenyan`，则必须同步改 LANG_STYLES 的 key 和前端传参。

## 3. 可以删掉的 patch（patches.py）

| patch | 现状 | 迁移后 |
|---|---|---|
| `_llm_compile.SECTION_HEADERS = [("文言", "")]`（patches.py:37） | 必需 | **可删**，由 config `languages:` 表达。过渡期保留也无害（patch 优先级更高，结果相同） |

仅此一条语言结构 patch 可删。其余 patch（各 SYSTEM_PROMPT、`COMPILE_ARTICLE_FORMAT`、
LANG_STYLES、entity formatter/批量抽取、`<think>` 剥离、CJK-slug lint 覆盖）**本轮契约
不覆盖，必须保留**——`languages:` 只管结构（split/merge/导出/校验/前端渲染），不管 LLM
prompt 文案。特别注意：若只加 config 而不保留 `COMPILE_ARTICLE_FORMAT`/`SYSTEM_PROMPT`
的 patch，compile 的 LLM 仍会按三语模板写作——结构契约不替代 prompt 契约。

## 4. 可以退役的前端 fork（frontend/src/lib/lang.tsx）

上游 `lang.tsx` 现在在 `bare_body` profile 下的行为 == siwen fork 手写的行为：
`extractLangContent` 恒等、`localizeTitle` 恒等、单 option 隐藏切换钮。

退役步骤（下一轮）：
1. 删除 siwen fork 的 `frontend/src/lib/lang.tsx`，改用上游版本。
2. **检查 import**：上游已删除静态 `LANG_OPTIONS` 导出（改为 `useLang().options`）。
   siwen fork 保留了 `LANG_OPTIONS` 导出以保形貌——若 siwen pages 有 import 它的，需改为
   `useLang().options`。
3. siwen 前端各处硬传的 `lang: 'zh'`：若 profile code 取 `zh`（见 §2）则不用动；
   否则改为 `useLang().lang` 或省略（后端会落到 `api_default_lang`）。

## 5. 候补退役（需验证，下一轮决定）

- `siwen/compile.py:_merge_into_siwen()`（自写的"不分节、按长度合并"）：上游 `_merge_into`
  现已对 `[("文言","")]` 走整段裸正文路径，单节按 1.2× 长度规则保留较长版本——语义接近。
  退役前需对照 siwen 自定逻辑逐条核（如是否有上游没有的合并规则）。
- siwen `web.py` 若有自己的 lang 处理，可直接复用上游契约校验。

## 6. 迁移注意事项 / 坑

1. **`?lang=` 现在会 400**：上游开始校验 lang。siwen 前端任何传非 profile code 的地方
   （如硬编码 `'en'`/`'ja'` 的残留）迁移后会显式失败——这是好事，但要先扫一遍。
2. **worker 的 xici 循环**改为按 `profile["codes"]` 生成。wenyan profile 只生成一个
   lang 的 xici（siwen 本来就关 worker，影响为零）。
3. **patch 优先级**：只要 patches.py 还在赋值 `SECTION_HEADERS`，config 的 `languages:`
   就被忽略（patch 胜出是契约）。删 patch 那一步要和加 config 同一个 commit。
4. **export key**：上游 `_EXPORT_KEY_MAP` 不变；文言 profile 下 export key 是 section
   `key`（"文言"）原样——siwen 若有消费 export JSON 的地方注意 key 形状。
5. **taxonomy/xici 内部仍只识别 zh/en/ja/zh-en**：`build_taxonomy` 的 label fallback 链
   与 `generate_xici` 的翻译指令是既有 customization 常量管辖（`TAXONOMY_LABEL_KEYS`、
   `TAXONOMY_GENERATOR`、`LANG_STYLES`），不随 `languages:` profile 走。自定义 code 的
   下游必须按既有契约覆盖它们（siwen 现行 patch 已覆盖，保留即可）。
5. 圣约不破：以上全部仅是"删 patch / 删 fork / 加 config"，llmwiki 之源仍未嘗一字之改。
