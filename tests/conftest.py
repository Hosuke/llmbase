"""Shared test fixtures for llmbase tests."""

import json
import shutil
from pathlib import Path

import frontmatter
import pytest


@pytest.fixture
def tmp_kb(tmp_path):
    """Create a temporary knowledge base with sample articles."""
    # Directories
    raw_dir = tmp_path / "raw"
    wiki_dir = tmp_path / "wiki"
    concepts_dir = wiki_dir / "concepts"
    meta_dir = wiki_dir / "_meta"
    outputs_dir = wiki_dir / "outputs"

    for d in [raw_dir, concepts_dir, meta_dir, outputs_dir]:
        d.mkdir(parents=True)

    # Config
    config = {
        "llm": {"max_tokens": 8192},
        "paths": {
            "raw": str(raw_dir),
            "wiki": str(wiki_dir),
            "outputs": str(outputs_dir),
            "meta": str(meta_dir),
            "concepts": str(concepts_dir),
        },
        "compile": {"batch_size": 10, "backlinks": True},
        "search": {"port": 5555},
        "lint": {"web_search": False},
        "health": {"auto_fix_broken_links": True, "max_stubs_per_run": 10},
        "entities": {"enabled": False},
    }

    import yaml
    (tmp_path / "config.yaml").write_text(yaml.dump(config))

    # Sample articles
    _write_article(concepts_dir, "kong", {
        "title": "Emptiness / 空",
        "summary": "The concept of emptiness in Buddhist philosophy",
        "tags": ["buddhism", "philosophy"],
    }, "## English\n\nEmptiness ([[sunyata]]) is central to [[Nagarjuna]]'s philosophy.\n\n## 中文\n\n空是佛教哲学的核心概念。\n\n## 日本語\n\n空は仏教哲学の中心概念です。")

    _write_article(concepts_dir, "si-di", {
        "title": "Four Noble Truths / 四諦",
        "summary": "The four truths taught by the Buddha",
        "tags": ["buddhism", "doctrine"],
    }, "## English\n\nThe [[Four Noble Truths]] are the foundation. See also [[kong|Emptiness]].\n\n## 中文\n\n四諦是佛教的根本教義。参见[[空]]。")

    _write_article(concepts_dir, "ren", {
        "title": "Benevolence / 仁",
        "summary": "Central Confucian virtue",
        "tags": ["confucianism", "ethics"],
    }, "## English\n\nRen is Confucianism's cardinal virtue.\n\n## 中文\n\n仁是儒家核心德性。")

    return tmp_path


def _write_article(concepts_dir, slug, metadata, content):
    post = frontmatter.Post(content)
    post.metadata.update(metadata)
    post.metadata["created"] = "2026-04-01T00:00:00+00:00"
    post.metadata["updated"] = "2026-04-01T00:00:00+00:00"
    (concepts_dir / f"{slug}.md").write_text(frontmatter.dumps(post), encoding="utf-8")
