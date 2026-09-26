"""Search engine: naive full-text search over the wiki with web UI and CLI."""

import json
import math
import re
from collections import Counter
from contextlib import contextmanager
from contextvars import ContextVar
from pathlib import Path
from typing import Callable

import frontmatter

from .config import load_config


SEARCH_TOKENIZER: Callable[[str], list[str]] | None = None
_PUBLIC_ONLY = ContextVar("llmwiki_search_public_only", default=False)


@contextmanager
def search_visibility(*, public_only: bool):
    """Constrain real default retrieval before tokenizer callbacks and scoring."""
    token = _PUBLIC_ONLY.set(_PUBLIC_ONLY.get() or public_only)
    try:
        yield
    finally:
        _PUBLIC_ONLY.reset(token)


def _hidden_from_public(metadata):
    return metadata.get("private") is True or metadata.get("visibility", "public") != "public"
STOPWORDS: set[str] = {
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "need", "dare", "ought",
    "used", "to", "of", "in", "for", "on", "with", "at", "by", "from",
    "as", "into", "through", "during", "before", "after", "above", "below",
    "between", "out", "off", "over", "under", "again", "further", "then",
    "once", "here", "there", "when", "where", "why", "how", "all", "both",
    "each", "few", "more", "most", "other", "some", "such", "no", "nor",
    "not", "only", "own", "same", "so", "than", "too", "very", "and",
    "but", "or", "if", "while", "that", "this", "it", "its", "they",
}
CJK_STOPWORDS: set[str] = {"的", "了", "是", "在", "也", "與", "与", "和"}
# Unsegmented scripts: CJK ideographs + Hiragana + Katakana + Hangul.
# These get decomposed into chars + bigrams (no whitespace boundaries).
_CJK_LIKE_RANGE = (
    r"\u3040-\u309f"          # Hiragana
    r"\u30a0-\u30ff"          # Katakana
    r"\u31f0-\u31ff"          # Katakana Phonetic Extensions
    r"\uff65-\uff9f"          # Halfwidth Katakana
    r"\u3400-\u4dbf"          # CJK Extension A
    r"\u4e00-\u9fff"          # CJK Unified Ideographs
    r"\uac00-\ud7af"          # Hangul Syllables
    r"\uf900-\ufaff"          # CJK Compatibility Ideographs
    r"\U00020000-\U0003ffff"  # CJK Extension B-G + supplements (Plane 2-3)
)
_WORD_RE = re.compile(r"\w+", re.UNICODE)
_CJK_LIKE_CHAR_RE = re.compile(f"[{_CJK_LIKE_RANGE}]")
_CJK_LIKE_RUN_RE = re.compile(f"[{_CJK_LIKE_RANGE}]+")


def search(query: str, top_k: int = 10, base_dir: Path | None = None) -> list[dict]:
    """Search the wiki using TF-IDF-like scoring."""
    from . import _native
    if _native.available():
        # Preserve Python slicing: None means all, negatives omit the tail, bool is an index.
        import operator
        limit = None if top_k is None else operator.index(top_k)
        return _native_search(query, limit, base_dir, "compiled")
    return _legacy_search(query, top_k=top_k, base_dir=base_dir)


def _legacy_search(query: str, top_k: int = 10, base_dir: Path | None = None) -> list[dict]:
    """Unbuilt development checkout only; installed native candidates cannot enter here."""
    cfg = load_config(base_dir)
    concepts_dir = Path(cfg["paths"]["concepts"])
    outputs_dir = Path(cfg["paths"]["outputs"])

    if not concepts_dir.exists():
        return []

    query_terms = _tokenize(query)
    if not query_terms:
        return []

    # Build document corpus
    docs = []
    for md_file in list(concepts_dir.glob("*.md")) + list(outputs_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        if _PUBLIC_ONLY.get() and _hidden_from_public(post.metadata):
            continue
        title = post.metadata.get("title", md_file.stem)
        summary = post.metadata.get("summary", "")
        tags = " ".join(post.metadata.get("tags", []))
        text = f"{title} {title} {summary} {tags} {post.content}"  # title weighted 2x
        tokens = _tokenize(text)
        docs.append({
            "path": str(md_file),
            "slug": md_file.stem,
            "title": title,
            "summary": summary,
            "tags": post.metadata.get("tags", []),
            "text": text,
            "tokens": tokens,
            "tokens_set": set(tokens),
        })

    if not docs:
        return []

    # Compute IDF (O(1) membership via tokens_set)
    doc_count = len(docs)
    idf = {}
    for term in query_terms:
        df = sum(1 for d in docs if term in d["tokens_set"])
        idf[term] = math.log((doc_count + 1) / (df + 1)) + 1

    # Score each document
    results = []
    for doc in docs:
        token_counts = Counter(doc["tokens"])
        score = 0.0
        matched_terms = []
        for term in query_terms:
            if term in token_counts:
                tf = 1 + math.log(token_counts[term])
                score += tf * idf[term]
                matched_terms.append(term)

        if score > 0:
            # Find best matching snippet
            snippet = _extract_snippet(doc["text"], query_terms)
            results.append({
                "slug": doc["slug"],
                "title": doc["title"],
                "summary": doc["summary"],
                "score": round(score, 3),
                "matched_terms": matched_terms,
                "snippet": snippet,
                "path": doc["path"],
            })

    results.sort(key=lambda x: x["score"], reverse=True)
    return results[:top_k]


def search_raw(query: str, top_k: int = 10, base_dir: Path | None = None) -> list[dict]:
    """Full-text search over the raw/ ingest directory (pre-compile sources).

    Used as a fallback when ``search()`` misses — raw holds verbatim source
    material (scraped pages, dictionaries, book chapters) that may contain
    exact wording, dates, or quotations lost during LLM compilation.

    Returns entries with ``source`` (raw/ subdirectory name), ``rel_path``
    (path relative to raw/), and ``source_url`` (populated only for http(s)
    URLs — local filesystem paths from local-file ingest are scrubbed to
    avoid leaking usernames/home dirs to MCP/HTTP clients).
    """
    from . import _native
    if _native.available():
        return _native_search(query, max(0, int(top_k)), base_dir, "raw")
    return _legacy_search_raw(query, top_k=top_k, base_dir=base_dir)


def _native_search(query: str, top_k: int | None, base_dir: Path | None, kind: str) -> list[dict]:
    from . import _native

    arguments = {"query": query, "kind": kind, "top_k": top_k,
                 "public_only": _PUBLIC_ONLY.get(),
                 "stopwords": list(STOPWORDS), "cjk_stopwords": list(CJK_STOPWORDS)}
    if SEARCH_TOKENIZER is not None and not (kind == "raw" and top_k == 0):
        # Only the explicitly registered Python tokenizer executes in Python. The text is
        # read by the native snapshot operation; scoring and corpus validation remain native.
        terms = _tokenize(query)
        if not terms:
            arguments["tokenized"] = {"snapshot": "", "query_terms": [], "documents": []}
            return _native.dispatch("compat_search", arguments, base_dir)["results"]
        snapshot = _native.dispatch("compat_search_documents", {"kind": kind, "public_only": _PUBLIC_ONLY.get()}, base_dir)
        documents = []
        for document in snapshot["documents"]:
            documents.append({"source_id": document["source_id"],
                              "content_hash": document["content_hash"],
                              "tokens": _tokenize(document["text"])})
        arguments["tokenized"] = {"snapshot": snapshot["snapshot"],
                                  "query_terms": terms, "documents": documents}
    return _native.dispatch("compat_search", arguments, base_dir)["results"]


def _legacy_search_raw(query: str, top_k: int = 10, base_dir: Path | None = None) -> list[dict]:
    """Unbuilt development checkout only; native runtime errors never fall back here."""
    cfg = load_config(base_dir)
    raw_dir = Path(cfg["paths"]["raw"])

    if not raw_dir.exists():
        return []

    # Clamp top_k to non-negative — negative slice would over-return.
    top_k = max(0, int(top_k))
    if top_k == 0:
        return []

    query_terms = _tokenize(query)
    if not query_terms:
        return []

    docs = []
    for md_file in raw_dir.rglob("*.md"):
        if _PUBLIC_ONLY.get() and ".original" in md_file.relative_to(raw_dir).parts:
            continue
        try:
            post = frontmatter.load(str(md_file))
        except Exception:
            continue
        if _PUBLIC_ONLY.get() and _hidden_from_public(post.metadata):
            continue
        title = post.metadata.get("title") or md_file.stem
        raw_source = post.metadata.get("source", "") or ""
        # Only surface http(s) URLs — local filesystem paths from local-file
        # ingest would leak usernames / absolute paths to callers.
        source_url = raw_source if isinstance(raw_source, str) and raw_source.startswith(("http://", "https://")) else ""
        rel = md_file.relative_to(raw_dir)
        # Use top-level subdir as the source identifier when metadata lacks it
        source_id = rel.parts[0] if len(rel.parts) > 1 else md_file.stem
        text = f"{title} {title} {source_url} {post.content}"
        tokens = _tokenize(text)
        if not tokens:
            continue
        docs.append({
            "rel_path": str(rel),
            "source": source_id,
            "source_url": source_url,
            "title": title,
            "text": text,
            "tokens": tokens,
            "tokens_set": set(tokens),
        })

    if not docs:
        return []

    doc_count = len(docs)
    idf = {}
    for term in query_terms:
        df = sum(1 for d in docs if term in d["tokens_set"])
        idf[term] = math.log((doc_count + 1) / (df + 1)) + 1

    results = []
    for doc in docs:
        token_counts = Counter(doc["tokens"])
        score = 0.0
        matched_terms = []
        for term in query_terms:
            if term in token_counts:
                tf = 1 + math.log(token_counts[term])
                score += tf * idf[term]
                matched_terms.append(term)

        if score > 0:
            snippet = _extract_snippet(doc["text"], query_terms)
            results.append({
                "source": doc["source"],
                "source_url": doc["source_url"],
                "title": doc["title"],
                "rel_path": doc["rel_path"],
                "score": round(score, 3),
                "matched_terms": matched_terms,
                "snippet": snippet,
            })

    results.sort(key=lambda x: x["score"], reverse=True)
    return results[:top_k]


def search_cli(query: str, base_dir: Path | None = None) -> str:
    """CLI-friendly search output (for LLM tool use)."""
    results = search(query, base_dir=base_dir)
    if not results:
        return f"No results found for: {query}"

    output = f"Search results for: {query}\n\n"
    for i, r in enumerate(results, 1):
        output += f"{i}. [{r['title']}] (score: {r['score']})\n"
        output += f"   {r['summary']}\n"
        if r.get("snippet"):
            output += f"   ...{r['snippet']}...\n"
        output += "\n"

    return output


def create_search_app(base_dir: Path | None = None):
    """Create Flask app for web UI search."""
    from flask import Flask, request, jsonify, render_template_string

    app = Flask(__name__)
    app.config["BASE_DIR"] = base_dir
    import os
    from .http_auth import can_read_private, private_response_headers, runtime_config
    actual_base = Path(base_dir).resolve() if base_dir is not None else Path.cwd()
    app.config["llmbase"] = runtime_config(actual_base, load_config(actual_base),
        api_secret=os.getenv("LLMBASE_API_SECRET", ""))
    app.after_request(private_response_headers)

    HTML_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
    <title>LLMBase Search</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
               max-width: 800px; margin: 0 auto; padding: 40px 20px; background: #1a1a2e; color: #e0e0e0; }
        h1 { margin-bottom: 20px; color: #e94560; }
        .search-box { display: flex; gap: 10px; margin-bottom: 30px; }
        input[type="text"] { flex: 1; padding: 12px 16px; font-size: 16px; border: 2px solid #16213e;
                             border-radius: 8px; background: #16213e; color: #e0e0e0; outline: none; }
        input[type="text"]:focus { border-color: #e94560; }
        button { padding: 12px 24px; font-size: 16px; background: #e94560; color: white;
                 border: none; border-radius: 8px; cursor: pointer; }
        button:hover { background: #c23152; }
        .result { padding: 16px; margin-bottom: 12px; background: #16213e;
                  border-radius: 8px; border-left: 3px solid #e94560; }
        .result h3 { color: #e94560; margin-bottom: 6px; }
        .result .score { color: #888; font-size: 0.85em; }
        .result .summary { margin-top: 6px; color: #aaa; }
        .result .snippet { margin-top: 8px; font-style: italic; color: #999; font-size: 0.9em; }
        .tags { margin-top: 6px; }
        .tag { display: inline-block; padding: 2px 8px; margin: 2px; background: #0f3460;
               border-radius: 4px; font-size: 0.8em; color: #a0d2db; }
        .stats { color: #666; margin-bottom: 20px; }
    </style>
</head>
<body>
    <h1>LLMBase Search</h1>
    <div class="search-box">
        <input type="text" id="q" placeholder="Search the knowledge base..." autofocus
               onkeypress="if(event.key==='Enter')doSearch()">
        <button onclick="doSearch()">Search</button>
    </div>
    <div id="stats" class="stats"></div>
    <div id="results"></div>
    <script>
        async function doSearch() {
            const q = document.getElementById('q').value;
            if (!q) return;
            const resp = await fetch('/api/search?q=' + encodeURIComponent(q));
            const data = await resp.json();
            const stats = document.getElementById('stats');
            const results = document.getElementById('results');
            stats.textContent = data.results.length + ' results found';
            results.replaceChildren();
            data.results.forEach((r, i) => {
                const item = document.createElement('div');
                item.className = 'result';
                const text = (tag, className, value) => {
                    const node = document.createElement(tag);
                    node.className = className;
                    node.textContent = value;
                    item.appendChild(node);
                };
                text('h3', '', (i + 1) + '. ' + r.title);
                text('span', 'score', 'Score: ' + r.score);
                if (r.summary) text('p', 'summary', r.summary);
                if (r.snippet) text('p', 'snippet', '...' + r.snippet + '...');
                results.appendChild(item);
            });
        }
    </script>
</body>
</html>"""

    @app.route("/")
    def index():
        return render_template_string(HTML_TEMPLATE)

    @app.route("/api/search")
    def api_search():
        q = request.args.get("q", "")
        top_k = int(request.args.get("top_k", 10))
        with search_visibility(public_only=not can_read_private()):
            results = search(q, top_k=top_k, base_dir=app.config["BASE_DIR"])
        return jsonify({"query": q, "results": results})

    return app


def _tokenize(text: str) -> list[str]:
    """Tokenize: Latin words (filtered by stopwords, len>1) + CJK chars + CJK bigrams.

    Downstream may override by setting module-level SEARCH_TOKENIZER to a callable.
    """
    if SEARCH_TOKENIZER is not None:
        return SEARCH_TOKENIZER(text)

    tokens: list[str] = []
    text_lower = text.lower()

    # Whitespace-segmented words (Latin, Cyrillic, Greek, accented, etc.).
    # Strip CJK-like chars so mixed tokens like "Go语言" still yield "go".
    text_for_words = _CJK_LIKE_CHAR_RE.sub(" ", text_lower)
    for w in _WORD_RE.findall(text_for_words):
        if w not in STOPWORDS and len(w) > 1:
            tokens.append(w)

    # Unsegmented runs (CJK / kana / Hangul): single chars + bigrams.
    for run in _CJK_LIKE_RUN_RE.findall(text):
        for ch in run:
            if ch not in CJK_STOPWORDS:
                tokens.append(ch)
        for i in range(len(run) - 1):
            tokens.append(run[i:i + 2])

    return tokens


def _extract_snippet(text: str, query_terms: list[str], window: int = 100) -> str:
    """Extract a snippet around the first matching term."""
    text_lower = text.lower()
    best_pos = len(text)
    for term in query_terms:
        pos = text_lower.find(term)
        if pos != -1 and pos < best_pos:
            best_pos = pos

    if best_pos == len(text):
        return text[:200]

    start = max(0, best_pos - window)
    end = min(len(text), best_pos + window)
    snippet = text[start:end].replace("\n", " ").strip()
    return snippet
