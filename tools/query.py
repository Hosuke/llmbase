"""Query module: Q&A against the wiki, with output filing."""

import json
import re
from datetime import datetime, timezone
from pathlib import Path

import frontmatter

from .config import load_config, ensure_dirs
from .llm import chat, chat_with_context


SYSTEM_PROMPT = """You are a research assistant with access to a personal knowledge base wiki.
Answer questions thoroughly based on the provided context. If the context doesn't contain
enough information, say so clearly.

When citing sources, reference the article titles. Use markdown formatting for your answers.
If asked to create visualizations, output matplotlib code blocks that can be executed."""


# Voice/tone modes: each maps to an instruction appended to the system prompt
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
        "範例：'佛者，覺也。覺諸法空，離一切相，是為正覺。"
        "《心經》云：色不異空，空不異色。此之謂也。'"
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


def query(
    question: str,
    output_format: str = "markdown",
    file_back: bool = False,
    base_dir: Path | None = None,
    tone: str = "default",
) -> str:
    """Ask a question against the wiki and return the answer."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)

    # Gather relevant context
    context_files = _gather_context(question, cfg)

    if not context_files:
        return "No articles found in the wiki. Run `llmbase compile` first to build the wiki from raw documents."

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
        max_tokens=cfg["llm"]["max_tokens"],
    )

    # File back into wiki if requested
    if file_back:
        _file_output(question, answer, output_format, cfg)

    return answer


def query_with_search(
    question: str,
    base_dir: Path | None = None,
    tone: str = "default",
) -> str:
    """Multi-step query: first search for relevant articles, then answer."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)

    # Step 1: Ask LLM to identify search terms
    meta_dir = Path(cfg["paths"]["meta"])
    index = _load_index(meta_dir)
    if not index:
        return "Wiki is empty. Run `llmbase compile` first."

    index_summary = "\n".join(
        f"- {e['title']}: {e['summary']}" for e in index
    )

    search_prompt = f"""Given this wiki index:
{index_summary}

And this question: {question}

Which articles (by title) are most relevant? List up to 10, one per line, just the titles."""

    relevant_titles = chat(search_prompt, max_tokens=1024)

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

    return chat_with_context(
        question,
        context_files,
        system=system,
        max_tokens=cfg["llm"]["max_tokens"],
    )


def _gather_context(question: str, cfg: dict) -> list[dict]:
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


def _file_output(question: str, answer: str, output_format: str, cfg: dict):
    """File a query output back into the wiki."""
    outputs_dir = Path(cfg["paths"]["outputs"])
    outputs_dir.mkdir(parents=True, exist_ok=True)

    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
    slug = re.sub(r"[^\w]+", "-", question.lower())[:50].strip("-")
    filename = f"{timestamp}-{slug}.md"

    post = frontmatter.Post(answer)
    post.metadata["title"] = question
    post.metadata["type"] = f"query_{output_format}"
    post.metadata["created"] = datetime.now(timezone.utc).isoformat()

    (outputs_dir / filename).write_text(frontmatter.dumps(post), encoding="utf-8")


def _load_index(meta_dir: Path) -> list[dict]:
    index_path = meta_dir / "index.json"
    if index_path.exists():
        return json.loads(index_path.read_text())
    return []
