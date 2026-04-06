"""Xi Ci (系辞) — LLM-generated guided introduction for the knowledge base.

Like a master librarian writing a guided introduction, this module
generates a living overview that ties together all articles into a
coherent intellectual framework. It adapts to the user's language
and regenerates as the knowledge base evolves.

The Xi Ci is NOT a summary — it's a meta-narrative that reveals the
structure, connections, and significance of the collected knowledge.
"""

import json
import logging
from datetime import datetime, timezone
from pathlib import Path

import frontmatter

from .config import load_config, ensure_dirs
from .llm import chat

logger = logging.getLogger("llmbase.xici")

XICI_SYSTEM_PROMPT = """You are a master librarian and intellectual guide. Your task is to write
a guided introduction (导读) for a personal knowledge base — a living preface that reveals
the deep structure and significance of the collected knowledge.

Rules:
- Write in the REQUESTED LANGUAGE and STYLE
- Do NOT list articles — weave their themes into a coherent narrative
- Reveal connections between topics that may not be obvious
- Identify the intellectual trajectory: what direction is this knowledge growing toward?
- Keep it concise: 3-5 sentences of elegant prose
- End with a question or insight that invites further exploration
- Do NOT assume any specific domain — derive everything from the actual content
- Do NOT mention "knowledge base" or "wiki" — write as if introducing a body of thought"""

LANG_STYLES = {
    "zh": "请用古典中文（文言文）风格撰写。用字简练，句式古雅。可用「者」「也」「矣」「焉」等语气词。",
    "en": "Write in elegant academic English. Formal but not stuffy. Like a well-crafted book preface.",
    "ja": "学術的な日本語で書いてください。格調高く、簡潔に。古典的な教養を感じさせる文体で。",
    "zh-en": "写两段：第一段用文言文，第二段用 English。两段各自独立，不是翻译关系，而是从不同文化视角解读同一知识体系。",
}


def generate_xici(base_dir: Path | None = None, lang: str = "zh") -> dict:
    """Generate Xi Ci for the given language.

    All languages are derived from a Chinese 文言文 base version:
    1. Generate (or load cached) 文言文 导读
    2. If lang != "zh", translate from 文言文 into target language
    This ensures all versions share the same intellectual framework.
    """
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    concepts_dir = Path(cfg["paths"]["concepts"])

    # Gather article metadata
    articles = []
    for md_file in sorted(concepts_dir.glob("*.md")):
        post = frontmatter.load(str(md_file))
        articles.append({
            "title": post.metadata.get("title", md_file.stem),
            "tags": post.metadata.get("tags", []),
            "summary": post.metadata.get("summary", ""),
        })

    if not articles:
        return {
            "text": "",
            "themes": [],
            "lang": lang,
            "generated_at": None,
            "article_count": 0,
        }

    # Collect top themes from tags
    from collections import Counter
    tag_counter = Counter()
    for a in articles:
        for t in a.get("tags", []):
            tag_counter[t] += 1
    themes = [tag for tag, _ in tag_counter.most_common(7)]

    # Step 1: Get or generate the 文言文 base
    zh_xici = get_xici(base_dir, "zh")
    zh_text = zh_xici.get("text", "")

    if not zh_text or zh_xici.get("article_count", 0) != len(articles):
        # Need to (re)generate the 文言文 base
        overview = "\n".join(
            f"- {a['title']}: {a['summary']} [tags: {', '.join(a['tags'])}]"
            for a in articles
        )
        style = LANG_STYLES["zh"]
        prompt = (
            f"Here are {len(articles)} articles in a personal knowledge base:\n\n"
            f"{overview}\n\n"
            f"Write a guided introduction (导读) for this knowledge base.\n\n"
            f"Language and style instruction:\n{style}\n\n"
            f"Remember: weave a narrative, don't list. Reveal the hidden structure."
        )
        try:
            zh_text = chat(prompt, system=XICI_SYSTEM_PROMPT, max_tokens=1024).strip()
        except Exception as e:
            logger.error(f"[xici] 文言文 generation failed: {e}")
            zh_text = ""

        # Cache the 文言文 base
        zh_result = {
            "text": zh_text,
            "themes": themes,
            "lang": "zh",
            "generated_at": datetime.now(timezone.utc).isoformat(),
            "article_count": len(articles),
        }
        _save_xici(cfg, "zh", zh_result)

    # Step 2: If target lang is zh, we're done
    if lang == "zh":
        return {
            "text": zh_text,
            "themes": themes,
            "lang": "zh",
            "generated_at": datetime.now(timezone.utc).isoformat(),
            "article_count": len(articles),
        }

    # Step 3: Translate from 文言文 into target language
    translate_instructions = {
        "en": (
            "Translate this classical Chinese (文言文) guided introduction into elegant academic English. "
            "Preserve the intellectual structure and rhetorical rhythm. "
            "Do not simplify — match the gravitas of the original."
        ),
        "ja": (
            "この文言文の導読を格調高い学術的日本語に翻訳してください。"
            "原文の知的構造と修辞的リズムを保ってください。"
        ),
        "zh-en": (
            "Output TWO paragraphs:\n"
            "1. The original 文言文 text as-is (do not modify)\n"
            "2. An English translation that preserves the intellectual structure\n\n"
            "Separate the two paragraphs with a line containing only ---"
        ),
    }

    instruction = translate_instructions.get(lang, translate_instructions["en"])
    translate_prompt = f"{instruction}\n\nOriginal 文言文:\n\n{zh_text}"

    try:
        text = chat(translate_prompt, max_tokens=1024).strip()
    except Exception as e:
        logger.error(f"[xici] Translation to {lang} failed: {e}")
        text = zh_text  # Fallback to 文言文

    result = {
        "text": text,
        "themes": themes,
        "lang": lang,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "article_count": len(articles),
    }

    # Cache to file
    _save_xici(cfg, lang, result)

    return result


def get_xici(base_dir: Path | None = None, lang: str = "zh") -> dict:
    """Get cached Xi Ci, or empty if not generated yet."""
    cfg = load_config(base_dir)
    meta_dir = Path(cfg["paths"]["meta"])
    path = meta_dir / f"xici-{lang}.json"
    if path.exists():
        return json.loads(path.read_text())
    return {
        "text": "",
        "themes": [],
        "lang": lang,
        "generated_at": None,
        "article_count": 0,
    }


def _save_xici(cfg: dict, lang: str, result: dict):
    """Cache Xi Ci to meta directory."""
    meta_dir = Path(cfg["paths"]["meta"])
    meta_dir.mkdir(parents=True, exist_ok=True)
    path = meta_dir / f"xici-{lang}.json"
    path.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
