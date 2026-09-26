"""Query module: Q&A against the wiki, with output filing.

Customization contract
======================
Downstream projects can override these module-level constants:

  SYSTEM_PROMPT       – system message for Q&A responses
  TONE_INSTRUCTIONS   – dict of tone_id → instruction string;
                        downstream can add/remove/replace tones

Example::

    import llmwiki.query as q
    q.TONE_INSTRUCTIONS["formal_chinese"] = "請以正式中文回答。"
"""

import json
import logging
import re
from datetime import datetime, timezone
from pathlib import Path

import frontmatter

from .config import load_config, ensure_dirs, get_section_headers
from .llm import chat, chat_with_context, extract_json

logger = logging.getLogger("llmbase.query")


# ─── Customizable constants ──────────────────────────────────────

SYSTEM_PROMPT = """You are a research assistant with access to a personal knowledge base wiki.
Answer questions thoroughly based on the provided context. If the context doesn't contain
enough information, say so clearly.

When citing sources, reference the article titles. Use markdown formatting for your answers.
If asked to create visualizations, output matplotlib code blocks that can be executed."""


# Voice/tone modes: each maps to an instruction appended to the system prompt.
# Downstream can add custom tones or remove built-in ones.
TONE_INSTRUCTIONS = {
    "default": "",
    "caveman": (
        "IMPORTANT TONE OVERRIDE: You are a caveman. Speak in broken, primitive language. "
        "Use simple words. No fancy grammar. Short sentences. "
        "Example: 'Fire hot. Book say thing about Buddha. Brain think: empty = good. "
        "No-self mean no worry. Caveman like.' "
        "Still convey the actual knowledge accurately, but wrap it in caveman speech. "
        "Use grunts (Ugg, Hmm, Ooga) for emphasis."
    ),
    "wenyan": (
        "重要語氣覆蓋：請以文言文風格作答。全文須用古典漢語（文言文）書寫，"
        "仿先秦兩漢之文風，用字簡練，句式古雅。"
        "可用「者」「也」「矣」「焉」「乎」「哉」等語氣詞，"
        "用「蓋」「夫」「且」「然則」等發語詞及連詞。"
        "引經據典時宜用原文。切勿用白話文。"
        "範例：'學者，覺也。覺其所未知，明其所未明，是為真學。"
        "蓋天下之理，非一端可盡，故博學而篤志，切問而近思。'"
    ),
    "scholar": (
        "TONE OVERRIDE: Respond in the style of a careful academic scholar. "
        "Use precise terminology, cite sources with page references where possible, "
        "note areas of scholarly debate, and distinguish between established consensus "
        "and your own interpretation. Write in formal academic prose."
    ),
    "eli5": (
        "TONE OVERRIDE: Explain Like I'm 5. Use the simplest possible language. "
        "Use analogies to everyday things a child would know. "
        "Short paragraphs. No jargon at all. If you must use a big word, "
        "explain it right away in parentheses."
    ),
}


def _legacy_query(
    question: str,
    output_format: str = "markdown",
    file_back: bool = False,
    base_dir: Path | None = None,
    tone: str = "default",
    return_path: bool = False,
    model: str | None = None,
    api_key: str | None = None,
) -> str | dict:
    """Ask a question against the wiki and return the answer.

    By default returns the answer string. When ``return_path=True`` returns
    a dict ``{"answer": str, "output_path": str | None}`` — useful for API
    callers that want to expose the filed-back output location to clients.

    *model* overrides the default LLM for this call only (no env mutation,
    no client re-init). ``None`` falls back to ``LLMBASE_MODEL``.

    *api_key* overrides the LLM credential for this call only (v0.7.4).
    Fresh un-cached client per call. ``None`` falls back to
    ``LLMBASE_API_KEY``/``OPENAI_API_KEY``. The key never reaches
    ``_file_output`` (it takes only question/answer/format/cfg) and is
    redacted from any error logging in ``chat()``.
    """
    cfg = load_config(base_dir)
    ensure_dirs(cfg)

    # Gather relevant context
    context_files = _gather_context(question, cfg)

    if not context_files:
        msg = "No articles found in the wiki. Run `llmbase compile` first to build the wiki from raw documents."
        return {"answer": msg, "output_path": None} if return_path else msg

    # Build the prompt based on output format and tone
    format_instruction = _format_instruction(output_format)
    tone_instruction = TONE_INSTRUCTIONS.get(tone, "")

    system = SYSTEM_PROMPT + f"\n\n{format_instruction}"
    if tone_instruction:
        system += f"\n\n{tone_instruction}"

    answer = chat_with_context(
        question,
        context_files,
        system=system,
        model=model,
        max_tokens=cfg["llm"]["max_tokens"],
        api_key=api_key,
    )

    # File back into wiki if requested
    output_path = _file_output(question, answer, output_format, cfg) if file_back else None

    if return_path:
        return {"answer": answer, "output_path": output_path}
    return answer


def _legacy_query_with_search(
    question: str,
    base_dir: Path | None = None,
    tone: str = "default",
    file_back: bool = False,
    return_context: bool = False,
    promote: bool = False,
    model: str | None = None,
    api_key: str | None = None,
) -> str | dict:
    """Multi-step query: first search for relevant articles, then answer.

    If return_context=True, returns {"answer": str, "consulted": [slug, ...]}
    instead of just the answer string.

    If promote=True, also runs an LLM judge to decide whether this Q&A
    should be promoted into a new (or extended) wiki concept. The
    resulting promotion info is added as "promotion" in the return dict.
    Requires return_context=True to take effect.

    *model* overrides the default LLM for both the article-selector call
    and the answer call. Promote-judge intentionally still uses the
    default model, since its job is meta-evaluation and should be
    insulated from per-query model whims.

    *api_key* overrides the LLM credential for the selector and answer
    calls (v0.7.4). Promote-judge uses the module singleton — same
    rationale as *model*: meta-eval should not be paid for by an
    untrusted per-request key.
    """
    cfg = load_config(base_dir)
    ensure_dirs(cfg)

    # Step 1: Ask LLM to identify search terms
    meta_dir = Path(cfg["paths"]["meta"])
    index = _load_index(meta_dir)
    if not index:
        return "Wiki is empty. Run `llmbase compile` first."

    # Step 0: TF-IDF prefilter — caps prompt size regardless of KB scale.
    # Below threshold, full index fits any model; above, the LLM selector
    # itself needs a pre-filtered candidate pool or it blows the context
    # window (observed: 11k-article KB ≈ 160k tokens of summaries alone).
    # Only ``selector_index`` is narrowed; ``index`` stays full so later
    # steps (consulted bookkeeping, promote's duplicate check) still see
    # every article.
    query_cfg = cfg.get("query") if isinstance(cfg.get("query"), dict) else {}
    query_cfg = query_cfg or {}

    def _int_cfg(key: str, default: int) -> int:
        try:
            val = query_cfg.get(key, default)
            return int(val) if val is not None else default
        except (TypeError, ValueError):
            return default

    prefilter_threshold = _int_cfg("prefilter_threshold", 500)
    prefilter_top_k = _int_cfg("prefilter_top_k", 200)

    selector_index = index
    if len(selector_index) > prefilter_threshold:
        selector_index = _bm25_prefilter(question, selector_index, top_k=prefilter_top_k)

    # Sanitize before assembling the selector prompt — existing index.json
    # entries may carry lone surrogates from pre-0.6.6 ingests. Without this
    # the selector `chat()` crashes before chat_with_context() ever runs.
    from .llm import strip_surrogates
    index_summary = "\n".join(
        f"- {strip_surrogates(str(e.get('title', '')))}: "
        f"{strip_surrogates(str(e.get('summary', '')))}"
        for e in selector_index
    )

    search_prompt = f"""Given this wiki index:
{index_summary}

And this question: {strip_surrogates(question)}

Which articles (by title) are most relevant? List up to 10, one per line, just the titles."""

    relevant_titles = chat(search_prompt, model=model, max_tokens=1024, api_key=api_key)

    # Step 2: Load those articles
    concepts_dir = Path(cfg["paths"]["concepts"])
    context_files = []
    for entry in index:
        if entry["title"].lower() in relevant_titles.lower():
            article_path = concepts_dir / f"{entry['slug']}.md"
            if article_path.exists():
                content = article_path.read_text()
                context_files.append({"path": entry["title"], "content": content})

    # If LLM matching missed, fall back to keyword matching
    if len(context_files) < 3:
        context_files = _gather_context(question, cfg)

    if not context_files:
        return "Could not find relevant articles for this question."

    # Build system prompt with tone
    system = SYSTEM_PROMPT
    tone_instruction = TONE_INSTRUCTIONS.get(tone, "")
    if tone_instruction:
        system += f"\n\n{tone_instruction}"

    answer = chat_with_context(
        question,
        context_files,
        system=system,
        model=model,
        max_tokens=cfg["llm"]["max_tokens"],
        api_key=api_key,
    )

    output_path = _file_output(question, answer, "markdown", cfg) if file_back else None

    if return_context:
        # Extract slugs from consulted articles
        consulted = []
        for entry in index:
            if any(cf["path"] == entry["title"] for cf in context_files):
                consulted.append({"slug": entry["slug"], "title": entry["title"]})

        result: dict = {"answer": answer, "consulted": consulted, "output_path": output_path}

        if promote:
            try:
                promotion = promote_to_concept(
                    question=question,
                    answer=answer,
                    consulted=consulted,
                    index=index,
                    base_dir=base_dir,
                )
                result["promotion"] = promotion
            except Exception as e:
                logger.warning(f"Promotion failed: {e}")
                result["promotion"] = {"promoted": False, "reason": f"error: {e}"}

        return result

    return answer


PROMOTE_SYSTEM_PROMPT = """You evaluate whether a Q&A exchange should be promoted
into a knowledge base as a standalone concept article.

Be conservative: only promote when the Q&A is about a nameable concept that
adds genuinely new knowledge or meaningfully extends an existing article.
Reject conversational / procedural / list-of-things questions.

You must reply with a single JSON object. No preamble, no markdown fences."""


# Overridable. If None, the content-schema example shown to the promote judge
# is auto-derived from the active language profile at call time, so downstream
# projects (e.g. single-language siwen) don't also have to replace this prompt.
# Import-time compile.SECTION_HEADERS overrides still win via get_section_headers().
# Set to a string to force a custom schema.
PROMOTE_CONTENT_EXAMPLE: str | None = None
PROMOTE_TITLE_EXAMPLE: str | None = None


def _derive_promote_examples(cfg: dict | None = None) -> tuple[str, str]:
    """Build content/title schema hints from the active language profile."""
    headers = get_section_headers(cfg) or [("English", "## English")]

    def _label(lang_key: str, header: str) -> str:
        # Prefer the header's visible text (strip markdown hashes) for display;
        # fall back to the key itself.
        return header.lstrip("#").strip() or lang_key

    if PROMOTE_CONTENT_EXAMPLE is not None:
        content_example = PROMOTE_CONTENT_EXAMPLE
    else:
        parts = []
        for i, (lang_key, header) in enumerate(headers):
            label = _label(lang_key, header)
            body = (
                f"Full {label} body with [[wiki-links]]…"
                if i == 0
                else f"{label} body…"
            )
            parts.append(f"{header}\n\n{body}" if header else body)
        content_example = "\n\n".join(parts)

    if PROMOTE_TITLE_EXAMPLE is not None:
        title_example = PROMOTE_TITLE_EXAMPLE
    else:
        labels = [_label(k, h) for k, h in headers]
        title_example = " / ".join(f"{lbl} title" for lbl in labels)

    return content_example, title_example


def _promotion_prompt(
    question: str,
    answer: str,
    consulted: list[dict],
    index: list[dict],
    base_dir: Path | None = None,
) -> str:
    """LLM judges whether a Q&A should become a wiki concept, and if yes, writes it.

    Returns a dict describing the outcome:
        {"promoted": False, "reason": "..."}
        {"promoted": True, "slug": "...", "title": "...", "path": "...",
         "merged": bool, "reason": "..."}

    Relies on compile._write_article() for the actual write, which handles
    3-layer deduplication (slug / alias / CJK substring) — so even if the
    judge mistakenly re-proposes an existing concept, the writer will merge
    rather than create a duplicate.
    """
    cfg = load_config(base_dir)

    # Build a compact index summary for the judge (cap at ~80 entries)
    index_lines = []
    for entry in index[:80]:
        summary = entry.get("summary", "") or ""
        if len(summary) > 120:
            summary = summary[:120] + "…"
        index_lines.append(f"- {entry['slug']}: {entry.get('title', '')} — {summary}")
    index_summary = "\n".join(index_lines) if index_lines else "(empty wiki)"

    consulted_slugs = [c["slug"] for c in consulted]
    section_headers = get_section_headers(cfg)
    content_example, title_example = _derive_promote_examples(cfg)
    content_example_json = json.dumps(content_example)
    title_example_json = json.dumps(title_example)

    prompt = f"""A user asked a question and received an answer from the wiki.
Decide whether this Q&A should be promoted into a standalone concept article.

QUESTION:
{question}

ANSWER:
{answer}

ARTICLES ALREADY CONSULTED FOR THIS ANSWER (likely candidates for merge, not new creation):
{', '.join(consulted_slugs) if consulted_slugs else '(none)'}

CURRENT WIKI INDEX (do not create duplicates — use merge_into for existing slugs):
{index_summary}

Decide:
1. Does this Q&A center on a clearly nameable concept?
2. Is that concept already covered? If yes, which existing slug should we merge into?
3. Does it add genuinely new, substantive knowledge (not just rephrasing)?

PROMOTE when ALL are true:
- Clear nameable concept at the center
- Either a new concept OR a meaningful extension of an existing one
- Substantive content (~150+ words of the answer are about the concept)

REJECT when ANY are true:
- Procedural / conversational / meta question ("how do I…", "what can you…")
- Vague, multi-topic, or list-of-things question
- Pure rehash of articles already in the index
- No clean slug/title can be extracted

Reply with EXACTLY this JSON schema (all fields required):

{{
  "promote": true,
  "reason": "one-line explanation",
  "merge_into": "existing-slug or null",
  "slug": "new-or-existing-slug (kebab-case, ascii; use pinyin for Chinese)",
  "title": {title_example_json},
  "summary": "one-line summary (≤200 chars)",
  "tags": ["tag1", "tag2"],
  "content": {content_example_json}
}}

If rejecting, reply with:
{{"promote": false, "reason": "one-line explanation"}}"""

    return prompt


def _legacy_promote_to_concept(question, answer, consulted, index, base_dir=None):
    from .compile import _write_article, rebuild_index
    cfg = load_config(base_dir)
    section_headers = get_section_headers(cfg)
    prompt = _promotion_prompt(question, answer, consulted, index, base_dir)
    raw = chat(
        prompt,
        system=PROMOTE_SYSTEM_PROMPT,
        max_tokens=cfg["llm"]["max_tokens"],
    )

    try:
        decision = json.loads(extract_json(raw))
    except (json.JSONDecodeError, ValueError) as e:
        return {"promoted": False, "reason": f"judge returned invalid JSON: {e}"}

    # Fail closed on non-object JSON ([], "ok", null, 42, …)
    if not isinstance(decision, dict):
        return {
            "promoted": False,
            "reason": f"judge returned non-object JSON: {type(decision).__name__}",
        }

    if not decision.get("promote"):
        return {
            "promoted": False,
            "reason": decision.get("reason", "judge declined"),
        }

    # If the judge said "merge into existing X", treat X as the target slug —
    # this prevents the writer from creating a duplicate when the judge picks
    # a different new slug than the one it intends to merge into.
    merge_into = decision.get("merge_into") or None
    if isinstance(merge_into, str) and merge_into.strip().lower() not in ("", "null", "none"):
        target_slug = merge_into.strip()
    else:
        merge_into = None
        target_slug = decision.get("slug")

    # Validate required fields for a promotion
    if not target_slug or not decision.get("title") or not decision.get("content"):
        missing = [
            k for k, v in (
                ("slug", target_slug),
                ("title", decision.get("title")),
                ("content", decision.get("content")),
            ) if not v
        ]
        return {
            "promoted": False,
            "reason": f"judge missing fields: {', '.join(missing)}",
        }

    # Sanitize slug up-front (mirrors _write_article so pre_exists can't be
    # tricked into probing paths outside concepts/ via traversal characters)
    safe_slug = (
        target_slug.replace("/", "-").replace("\\", "-").replace("..", "").strip(".-_ ")
    )
    if not safe_slug:
        return {"promoted": False, "reason": "judge slug sanitized to empty"}

    # Build article dict shaped like compile._write_article expects
    article = {
        "slug": safe_slug,
        "title": decision["title"],
        "summary": decision.get("summary", ""),
        "tags": decision.get("tags", []),
        "content": decision["content"],
        "sources": [{
            "plugin": "qa",
            "url": "",
            "title": question,
            "question": question,
            "created": datetime.now(timezone.utc).isoformat(),
        }],
    }

    concepts_dir = Path(cfg["paths"]["concepts"])
    pre_exists = (concepts_dir / f"{safe_slug}.md").exists()

    article_path = _write_article(article, concepts_dir, headers=section_headers)
    if article_path is None:
        return {"promoted": False, "reason": "write_article rejected slug"}

    # Rebuild index/aliases/backlinks so the new concept is discoverable
    rebuild_index(base_dir)

    # Determine whether this was a merge or a new file. Merge happens when:
    # - the target file already existed (exact slug or alias hit), or
    # - dedup redirected the write to a different file (CJK substring)
    final_slug = article_path.stem
    merged = pre_exists or (final_slug != safe_slug)

    return {
        "promoted": True,
        "reason": decision.get("reason", ""),
        "slug": final_slug,
        "title": decision["title"],
        "path": str(article_path),
        "merged": merged,
    }


def _legacy_gather_context(question: str, cfg: dict) -> list[dict]:
    """Gather relevant wiki articles as context for a query."""
    concepts_dir = Path(cfg["paths"]["concepts"])
    meta_dir = Path(cfg["paths"]["meta"])
    outputs_dir = Path(cfg["paths"]["outputs"])

    context_files = []

    # Always include the index
    index_path = meta_dir / "_index.md"
    if index_path.exists():
        context_files.append({
            "path": "_index.md",
            "content": index_path.read_text()[:3000],
        })

    # Score articles by keyword overlap
    question_words = set(re.findall(r"\w+", question.lower()))
    scored = []

    for md_file in concepts_dir.glob("*.md"):
        content = md_file.read_text()
        post = frontmatter.load(str(md_file))

        # Simple relevance scoring
        title = post.metadata.get("title", "").lower()
        summary = post.metadata.get("summary", "").lower()
        tags = " ".join(post.metadata.get("tags", [])).lower()
        text = f"{title} {summary} {tags} {content[:500]}".lower()
        text_words = set(re.findall(r"\w+", text))

        overlap = len(question_words & text_words)
        if overlap > 0:
            scored.append((overlap, md_file, content))

    # Sort by relevance, take top articles
    scored.sort(key=lambda x: x[0], reverse=True)
    for _, md_file, content in scored[:15]:
        context_files.append({
            "path": md_file.name,
            "content": content[:4000],
        })

    # Also check outputs
    for md_file in outputs_dir.glob("*.md"):
        content = md_file.read_text()
        text_words = set(re.findall(r"\w+", content[:500].lower()))
        if len(question_words & text_words) > 1:
            context_files.append({
                "path": f"outputs/{md_file.name}",
                "content": content[:3000],
            })

    return context_files


def _format_instruction(output_format: str) -> str:
    """Get format-specific instructions."""
    instructions = {
        "markdown": "Format your answer as clean markdown.",
        "marp": """Format your answer as a Marp slide deck. Use this format:
---
marp: true
theme: default
---

# Slide Title

Content here

---

# Next Slide

More content""",
        "chart": """Include matplotlib Python code to generate relevant charts/visualizations.
Wrap the code in ```python blocks. The code should save the figure to a file path
that will be provided. Use plt.savefig() at the end.""",
    }
    return instructions.get(output_format, instructions["markdown"])


def _legacy_file_output(question: str, answer: str, output_format: str, cfg: dict) -> str:
    """File a query output back into the wiki.

    Returns a *sanitized* path of the written file, relative to the project
    base when possible — never an absolute filesystem path — to avoid
    leaking server directory structure through unauthenticated endpoints
    like ``/api/ask``. Falls back to just the filename if the configured
    outputs dir lies outside the base.
    """
    outputs_dir = Path(cfg["paths"]["outputs"])
    outputs_dir.mkdir(parents=True, exist_ok=True)

    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
    slug = re.sub(r"[^\w]+", "-", question.lower())[:50].strip("-")
    filename = f"{timestamp}-{slug}.md"

    post = frontmatter.Post(answer)
    post.metadata["title"] = question
    post.metadata["type"] = f"query_{output_format}"
    post.metadata["created"] = datetime.now(timezone.utc).isoformat()

    output_path = outputs_dir / filename
    output_path.write_text(frontmatter.dumps(post), encoding="utf-8")

    # Sanitize: return a path relative to the project base when possible;
    # otherwise fall back to the bare filename. Never return an absolute
    # filesystem path (info-disclosure on unauth /api/ask).
    base_dir = Path(cfg.get("base_dir") or ".").resolve()
    try:
        return str(output_path.resolve().relative_to(base_dir))
    except ValueError:
        return filename


def _legacy_load_index(meta_dir: Path) -> list[dict]:
    index_path = meta_dir / "index.json"
    if index_path.exists():
        return json.loads(index_path.read_text())
    return []


def _legacy_bm25_prefilter(question: str, index: list[dict], top_k: int) -> list[dict]:
    """Rank index entries by TF-IDF over (title + summary + tags) vs question.

    (Name retained for historical reasons; the scoring is TF-IDF, the same
    formula ``tools.search.search()`` uses.)

    Returns the top_k entries. Degenerate queries (no tokens after
    filtering) and empty-match runs fall back to a simple slice so the
    deep-ask path always has candidates to hand to the LLM selector.

    Reuses ``tools.search._tokenize`` to stay consistent with the main
    search path — CJK-aware (chars + bigrams) and stopword-filtered.
    """
    import math
    from collections import Counter
    from .search import _tokenize

    if top_k <= 0:
        return []

    query_terms = _tokenize(question)
    if not query_terms:
        return index[:top_k]

    docs = []
    for entry in index:
        tags = entry.get("tags") or []
        text = " ".join([
            str(entry.get("title", "")),
            str(entry.get("summary", "")),
            " ".join(str(t) for t in tags),
        ])
        tokens = _tokenize(text)
        if not tokens:
            continue
        docs.append({
            "entry": entry,
            "tokens": tokens,
            "tokens_set": set(tokens),
        })

    if not docs:
        return index[:top_k]

    doc_count = len(docs)
    idf = {}
    for term in query_terms:
        df = sum(1 for d in docs if term in d["tokens_set"])
        idf[term] = math.log((doc_count + 1) / (df + 1)) + 1

    scored = []
    for d in docs:
        counts = Counter(d["tokens"])
        score = 0.0
        for term in query_terms:
            if term in counts:
                tf = 1 + math.log(counts[term])
                score += tf * idf[term]
        if score > 0:
            scored.append((score, d["entry"]))

    scored.sort(key=lambda x: x[0], reverse=True)
    selected = [e for _, e in scored[:top_k]]

    return selected or index[:top_k]


# Default orchestration lives in Rust. These callbacks keep executable, per-call
# Python customizations explicit; credentials exist only in this closure.
from . import _native
from . import search as _query_search_module

_DEFAULT_QUERY_CHAT = chat
_DEFAULT_QUERY_TOKENIZER = _query_search_module._tokenize


def _query_config(cfg: dict) -> tuple[Path, dict]:
    base = Path(cfg.get("base_dir") or ".").resolve()
    paths = {key: str(Path(value).resolve()) for key, value in cfg["paths"].items()
             if key in {"raw", "concepts", "outputs", "meta"}}
    return base, paths


def _native_query(request: dict, cfg: dict, *, model=None, api_key=None):
    base, paths = _query_config(cfg)

    def callback(stage, payload):
        if stage == "index":
            if _load_index is not _DEFAULT_LOAD_INDEX:
                return _load_index(Path(cfg["paths"]["meta"])) or []
            if _gather_context is not _DEFAULT_GATHER_CONTEXT:
                # A caller-supplied retrieval fixture is an explicit Python
                # boundary, including its legacy selector index.
                return _legacy_load_index(Path(cfg["paths"]["meta"]))
            return None
        if stage == "gather":
            return ((_gather_context(payload["question"], cfg) or [])
                    if _gather_context is not _DEFAULT_GATHER_CONTEXT else None)
        if stage == "prefilter":
            return _bm25_prefilter(payload["question"], payload["index"], payload["top_k"])
        if stage == "selector":
            from .llm import strip_surrogates
            summary = "\n".join(
                f"- {strip_surrogates(str(row.get('title', '')))}: "
                f"{strip_surrogates(str(row.get('summary', '')))}"
                for row in payload["index"])
            prompt = (f"Given this wiki index:\n{summary}\n\n"
                      f"And this question: {strip_surrogates(payload['question'])}\n\n"
                      "Which articles (by title) are most relevant? List up to 10, one per line, just the titles.")
            return chat(prompt, model=model, max_tokens=1024, api_key=api_key)
        if stage == "answer":
            system = SYSTEM_PROMPT
            if not payload["deep"]:
                system += f"\n\n{_format_instruction(payload['output_format'])}"
            tone = TONE_INSTRUCTIONS.get(payload["tone"], "")
            if tone:
                system += f"\n\n{tone}"
            return chat_with_context(payload["question"], payload["context"],
                                     system=system, model=model,
                                     max_tokens=cfg["llm"]["max_tokens"], api_key=api_key)
        if stage == "file_output":
            return ({"override": True, "value": _file_output(payload["question"], payload["answer"],
                                 payload["output_format"], cfg)}
                    if _file_output is not _DEFAULT_FILE_OUTPUT else None)
        if stage == "promotion_override":
            if promote_to_concept is not _DEFAULT_PROMOTE:
                try:
                    return {"override": True, "value": promote_to_concept(**payload, base_dir=base)}
                except Exception as error:
                    from .llm import _redact_key
                    message = _redact_key(str(error), api_key)
                    logger.warning("Promotion callback failed: %s", message)
                    return {"promoted": False, "reason": f"error: {message}"}
            return None
        if stage == "judge":
            prompt = _promotion_prompt(payload["question"], payload["answer"],
                                       payload["consulted"], payload["index"], base)
            truncated = False
            if chat is _DEFAULT_QUERY_CHAT:
                from . import llm
                raw, meta = llm.chat_with_meta(prompt, system=PROMOTE_SYSTEM_PROMPT,
                                              max_tokens=cfg["llm"]["max_tokens"])
                truncated = meta.truncated
            else:
                raw = chat(prompt, system=PROMOTE_SYSTEM_PROMPT,
                           max_tokens=cfg["llm"]["max_tokens"])
            try:
                decision = json.loads(extract_json(raw))
            except (json.JSONDecodeError, ValueError) as error:
                return {"error": f"judge returned invalid JSON: {error}"}
            return {"decision": decision, "headers": get_section_headers(cfg),
                    "truncated": truncated, "created": datetime.now(timezone.utc).isoformat()}
        raise _native.NativeCapabilityError(f"unsupported native query callback: {stage}")

    from .llm import strip_surrogates
    def clean(value):
        if isinstance(value, str):
            return strip_surrogates(value)
        if isinstance(value, list):
            return [clean(item) for item in value]
        if isinstance(value, dict):
            return {key: clean(item) for key, item in value.items()}
        return value
    return _native.run_query(clean(request), callback, base, paths=paths)


def query(question: str, output_format: str = "markdown", file_back: bool = False,
          base_dir: Path | None = None, tone: str = "default", return_path: bool = False,
          model: str | None = None, api_key: str | None = None) -> str | dict:
    if not _native.available():
        return _legacy_query(question, output_format, file_back, base_dir, tone,
                             return_path, model, api_key)
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    return _native_query({"mode": "basic", "question": question,
                          "output_format": output_format, "file_back": file_back,
                          "tone": tone, "return_path": return_path}, cfg,
                         model=model, api_key=api_key)


def query_with_search(question: str, base_dir: Path | None = None, tone: str = "default",
                      file_back: bool = False, return_context: bool = False,
                      promote: bool = False, model: str | None = None,
                      api_key: str | None = None) -> str | dict:
    if not _native.available():
        return _legacy_query_with_search(question, base_dir, tone, file_back,
                                         return_context, promote, model, api_key)
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    settings = cfg.get("query") if isinstance(cfg.get("query"), dict) else {}
    def integer(key, default):
        try:
            value = settings.get(key, default)
            return int(value) if value is not None else default
        except (TypeError, ValueError):
            return default
    return _native_query({"mode": "deep", "question": question, "tone": tone,
                          "file_back": file_back, "return_context": return_context,
                          "promote": promote, "prefilter_threshold": integer("prefilter_threshold", 500),
                          "prefilter_top_k": integer("prefilter_top_k", 200)}, cfg,
                         model=model, api_key=api_key)


def promote_to_concept(question: str, answer: str, consulted: list[dict], index: list[dict],
                       base_dir: Path | None = None) -> dict:
    if not _native.available():
        return _legacy_promote_to_concept(question, answer, consulted, index, base_dir)
    cfg = load_config(base_dir)
    return _native_query({"mode": "promote", "question": question, "answer": answer,
                          "consulted": consulted, "index": index}, cfg)


def _gather_context(question: str, cfg: dict) -> list[dict]:
    if not _native.available():
        return _legacy_gather_context(question, cfg)
    return _native_query({"mode": "gather", "question": question}, cfg)


def _file_output(question: str, answer: str, output_format: str, cfg: dict) -> str:
    if not _native.available():
        return _legacy_file_output(question, answer, output_format, cfg)
    return _native_query({"mode": "file_output", "question": question,
                          "answer": answer, "output_format": output_format}, cfg)


def _load_index(meta_dir: Path) -> list[dict]:
    if not _native.available():
        return _legacy_load_index(meta_dir)
    # Pure legacy helper has only a metadata root. Infer its sibling article root;
    # complete query calls pass the effective configuration directly to Rust.
    meta_dir = Path(meta_dir).resolve()
    cfg = {"base_dir": meta_dir.parent.parent,
           "paths": {"meta": meta_dir, "concepts": meta_dir.parent / "concepts",
                     "outputs": meta_dir.parent / "outputs", "raw": meta_dir.parent.parent / "raw"}}
    return _native_query({"mode": "load_index"}, cfg)


def _bm25_prefilter(question: str, index: list[dict], top_k: int) -> list[dict]:
    if not _native.available():
        return _legacy_bm25_prefilter(question, index, top_k)
    args = {"question": question, "index": index, "top_k": top_k,
            "stopwords": list(_query_search_module.STOPWORDS),
            "cjk_stopwords": list(_query_search_module.CJK_STOPWORDS)}
    if (_query_search_module.SEARCH_TOKENIZER is not None or
            _query_search_module._tokenize is not _DEFAULT_QUERY_TOKENIZER):
        tokenizer = _query_search_module._tokenize
        args["tokenized"] = {"query": tokenizer(question), "documents": [
            tokenizer(" ".join([str(row.get("title", "")), str(row.get("summary", "")),
                                 " ".join(str(tag) for tag in (row.get("tags") or []))]))
            for row in index]}
    return _native.query_transform("prefilter", args)


_DEFAULT_GATHER_CONTEXT = _gather_context
_DEFAULT_LOAD_INDEX = _load_index
_DEFAULT_FILE_OUTPUT = _file_output
_DEFAULT_PROMOTE = promote_to_concept

query.__doc__ = _legacy_query.__doc__
query_with_search.__doc__ = _legacy_query_with_search.__doc__
promote_to_concept.__doc__ = _promotion_prompt.__doc__
