"""Tests for BM25 prefilter in query_with_search.

Regression guard: at ~11k+ articles, the full index summary exceeded the
upstream LLM context window (~160k tokens for siwen's 11,625 articles).
Prefilter caps the prompt regardless of KB scale.
"""

import json
from pathlib import Path

import yaml
import pytest

from tools.query import _bm25_prefilter, query_with_search


def _make_entries(n: int, relevant_keyword: str = "kong-xing-emptiness") -> list[dict]:
    """Build n synthetic index entries with a few question-relevant ones."""
    entries = []
    # 3 relevant entries on 空性
    entries.append({"slug": "kong-xing", "title": "空性 / Emptiness",
                    "summary": f"The central concept of {relevant_keyword} in Madhyamaka.",
                    "tags": ["buddhism", relevant_keyword]})
    entries.append({"slug": "sunyata", "title": "Sunyata / 空",
                    "summary": f"Sanskrit term, equivalent to {relevant_keyword}.",
                    "tags": ["buddhism"]})
    entries.append({"slug": "nagarjuna", "title": "Nāgārjuna / 龍樹",
                    "summary": f"Philosopher who systematized {relevant_keyword}.",
                    "tags": ["philosophy", "buddhism"]})
    # n-3 filler entries
    for i in range(n - 3):
        entries.append({
            "slug": f"filler-{i}",
            "title": f"Filler topic {i}",
            "summary": f"Unrelated content about topic {i} and its history.",
            "tags": [f"topic-{i % 10}"],
        })
    return entries


def test_prefilter_returns_top_k():
    index = _make_entries(1000)
    result = _bm25_prefilter("空性 是 什麼", index, top_k=50)
    assert len(result) <= 50
    slugs = {r["slug"] for r in result}
    # At least one of the three relevant entries should rank in top 50
    assert slugs & {"kong-xing", "sunyata", "nagarjuna"}


def test_prefilter_degenerate_query_falls_back():
    index = _make_entries(100)
    # Pure punctuation → no tokens → must not crash
    result = _bm25_prefilter("???", index, top_k=10)
    assert len(result) == 10


def test_prefilter_empty_index():
    assert _bm25_prefilter("anything", [], top_k=10) == []


def test_prefilter_top_k_zero_returns_empty():
    assert _bm25_prefilter("q", _make_entries(10), top_k=0) == []


def test_prefilter_no_matches_falls_back_to_slice():
    """If BM25 scores zero everywhere, still return candidates (not empty)."""
    index = [
        {"slug": f"doc-{i}", "title": f"doc {i}", "summary": "zz zz", "tags": []}
        for i in range(20)
    ]
    # Query shares no tokens with any doc
    result = _bm25_prefilter("completely unrelated terms xyzzy", index, top_k=5)
    assert len(result) == 5


def test_query_with_search_caps_prompt_for_large_index(tmp_path, monkeypatch):
    """Large KB must not pass the full index into the LLM selector."""
    # Build a minimal KB layout
    raw = tmp_path / "raw"
    wiki = tmp_path / "wiki"
    concepts = wiki / "concepts"
    meta = wiki / "_meta"
    outputs = wiki / "outputs"
    for d in (raw, concepts, meta, outputs):
        d.mkdir(parents=True)

    cfg = {
        "llm": {"max_tokens": 1024},
        "paths": {
            "raw": str(raw), "wiki": str(wiki), "outputs": str(outputs),
            "meta": str(meta), "concepts": str(concepts),
        },
        "compile": {"batch_size": 10, "backlinks": True},
        "search": {"port": 5555},
        "query": {"prefilter_threshold": 500, "prefilter_top_k": 50},
    }
    (tmp_path / "config.yaml").write_text(yaml.dump(cfg))

    # 5000 synthetic entries
    entries = _make_entries(5000)
    (meta / "index.json").write_text(json.dumps(entries))
    # Create concept files for the relevant entries so context_files fills
    for e in entries[:3]:
        (concepts / f"{e['slug']}.md").write_text(
            f"---\ntitle: {e['title']}\n---\n\nContent about {e['slug']}."
        )

    # Record prompts handed to the LLM
    captured = {"search_prompt_len": 0}

    def fake_chat(prompt, **kw):
        captured["search_prompt_len"] = max(captured["search_prompt_len"], len(prompt))
        # Return a title we know is relevant so the selector succeeds
        return "空性 / Emptiness"

    def fake_chat_with_context(question, context_files, **kw):
        return "answer"

    monkeypatch.setattr("tools.query.chat", fake_chat)
    monkeypatch.setattr("tools.query.chat_with_context", fake_chat_with_context)
    monkeypatch.chdir(tmp_path)

    result = query_with_search("What is 空性?", base_dir=tmp_path)
    assert result == "answer"
    # Without prefilter: 5000 entries × ~60 chars ≈ 300k chars.
    # With prefilter (top_k=50): should be well under 50k chars.
    assert captured["search_prompt_len"] < 50_000, (
        f"search prompt was {captured['search_prompt_len']} chars — prefilter not engaged"
    )
