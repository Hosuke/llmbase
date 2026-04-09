"""Wikisource plugin — ingest classical texts from zh.wikisource.org.

Uses MediaWiki API to fetch full text. No scraping needed.
All content is public domain.
"""

import re
import time
import json
from datetime import datetime, timezone
from pathlib import Path

import requests
import frontmatter

from .config import load_config, ensure_dirs

API_URL = "https://zh.wikisource.org/w/api.php"
HEADERS = {"User-Agent": "LLMBase/1.0 (https://github.com/Hosuke/llmbase)"}

# Pre-defined reading lists for auto-learning
READING_LISTS = {
    "confucianism": [
        "論語", "孟子", "大學", "中庸", "荀子",
        "禮記", "孝經", "爾雅",
    ],
    "daoism": [
        "道德經", "莊子", "列子", "關尹子", "文子",
        "抱朴子", "太上感應篇",
    ],
    "mohism": ["墨子"],
    "legalism": ["韓非子", "商君書"],
    "military": ["孫子兵法", "吳子", "六韜", "三略", "司馬法"],
    "history": [
        "春秋左氏傳", "春秋公羊傳", "春秋穀梁傳",
        "國語", "戰國策", "史記",
    ],
    "poetry": ["詩經", "楚辭"],
    "divination": ["周易"],
    "zhuzi": [
        "管子", "晏子春秋", "公孫龍子",
        "呂氏春秋", "淮南子", "鬼谷子",
        "尹文子", "鶡冠子",
    ],
}


def fetch_page(title: str) -> dict:
    """Fetch a page's wikitext content via MediaWiki API."""
    params = {
        "action": "parse",
        "page": title,
        "prop": "wikitext|categories",
        "format": "json",
    }
    resp = requests.get(API_URL, params=params, headers=HEADERS, timeout=20)
    resp.raise_for_status()
    data = resp.json()

    if "error" in data:
        return {"title": title, "content": "", "error": data["error"].get("info", "")}

    parse = data.get("parse", {})
    wikitext = parse.get("wikitext", {}).get("*", "")
    categories = [c["*"] for c in parse.get("categories", [])]

    # Clean wikitext to markdown
    content = _wikitext_to_markdown(wikitext)

    return {
        "title": parse.get("title", title),
        "content": content,
        "categories": categories,
    }


def fetch_subpages(title: str) -> list[str]:
    """Fetch subpage list for a multi-chapter work."""
    params = {
        "action": "query",
        "list": "allpages",
        "apprefix": title + "/",
        "apnamespace": 0,
        "aplimit": 500,
        "format": "json",
    }
    resp = requests.get(API_URL, params=params, headers=HEADERS, timeout=15)
    resp.raise_for_status()
    data = resp.json()
    pages = data.get("query", {}).get("allpages", [])
    return [p["title"] for p in pages]


def ingest_work(
    title: str,
    base_dir: Path | None = None,
) -> list[Path]:
    """Ingest a complete work from Wikisource."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    raw_dir = Path(cfg["paths"]["raw"])

    results = []

    # Check for subpages (multi-chapter works)
    subpages = fetch_subpages(title)
    time.sleep(0.5)

    if subpages:
        # Multi-chapter: ingest each subpage
        for sp in subpages:
            path = _ingest_page(sp, title, raw_dir)
            if path:
                results.append(path)
            time.sleep(1)
    else:
        # Single page work
        path = _ingest_page(title, title, raw_dir)
        if path:
            results.append(path)

    return results


def _ingest_page(page_title: str, work_title: str, raw_dir: Path) -> Path | None:
    """Ingest a single page."""
    slug = re.sub(r"[^\w]+", "-", page_title).strip("-")
    doc_dir = raw_dir / f"wikisource-{slug}"

    # Skip if already ingested
    if (doc_dir / "index.md").exists():
        return None

    page = fetch_page(page_title)
    if not page["content"] or len(page["content"]) < 20:
        return None

    doc_dir.mkdir(parents=True, exist_ok=True)

    # Determine chapter name
    if "/" in page_title:
        chapter = page_title.split("/")[-1]
    else:
        chapter = "全文"

    post = frontmatter.Post(page["content"])
    post.metadata["title"] = f"{work_title} · {chapter}" if chapter != "全文" else work_title
    post.metadata["source"] = f"https://zh.wikisource.org/wiki/{page_title}"
    post.metadata["ingested_at"] = datetime.now(timezone.utc).isoformat()
    post.metadata["type"] = "wikisource"
    post.metadata["work"] = work_title
    post.metadata["chapter"] = chapter
    post.metadata["compiled"] = False

    doc_path = doc_dir / "index.md"
    doc_path.write_text(frontmatter.dumps(post), encoding="utf-8")
    return doc_path


def learn(
    reading_list: str | None = None,
    batch_size: int = 5,
    base_dir: Path | None = None,
) -> list[str]:
    """Progressive learning from Wikisource.

    Each call picks the next batch of works not yet ingested.

    Local progress is validated against on-disk raw doc presence so a
    volume wipe cannot leave stale entries in the skip-set. After each
    successful ingest an ``ingested`` hook is emitted — downstream can
    register callbacks (e.g. remote sync) via ``tools.hooks.register``.
    """
    from .hooks import emit

    base = Path(base_dir) if base_dir else Path.cwd()
    cfg = load_config(base)
    meta_dir = Path(cfg["paths"]["meta"])
    raw_dir = Path(cfg["paths"]["raw"])
    meta_dir.mkdir(parents=True, exist_ok=True)

    # Load progress
    progress_path = meta_dir / "wikisource_progress.json"
    if progress_path.exists():
        progress = json.loads(progress_path.read_text())
    else:
        progress = {"ingested_works": []}

    local_ingested = set(progress["ingested_works"])

    def _raw_exists(title: str) -> bool:
        """True iff any raw file for the work exists on disk.

        Wikisource works can be single-page or multi-chapter:
        - single-page: ``raw/wikisource-<slug(title)>/index.md``
        - multi-chapter: ``raw/wikisource-<slug(title)>-<chapter>/index.md``

        Check both shapes; prefix match must end at a dash to avoid
        ``論`` matching ``論語``-prefixed directories.
        """
        slug = re.sub(r"[^\w]+", "-", title).strip("-")
        if not slug:
            return False
        if (raw_dir / f"wikisource-{slug}" / "index.md").exists():
            return True
        if not raw_dir.exists():
            return False
        prefix = f"wikisource-{slug}-"
        for entry in raw_dir.iterdir():
            if entry.is_dir() and entry.name.startswith(prefix):
                if (entry / "index.md").exists():
                    return True
        return False

    # Drop stale local entries (file gone but progress claims ingested)
    stale_local = {t for t in local_ingested if not _raw_exists(t)}
    if stale_local:
        local_ingested -= stale_local
        progress["ingested_works"] = sorted(local_ingested)
        progress_path.write_text(
            json.dumps(progress, indent=2, ensure_ascii=False)
        )

    ingested = local_ingested

    # Build work list
    if reading_list and reading_list in READING_LISTS:
        works = READING_LISTS[reading_list]
    else:
        # All works from all lists
        works = []
        for lst in READING_LISTS.values():
            works.extend(lst)
        works = list(dict.fromkeys(works))  # Deduplicate preserving order

    # Filter out already ingested
    pending = [w for w in works if w not in ingested]

    if not pending:
        return []

    batch = pending[:batch_size]
    results = []

    for title in batch:
        try:
            paths = ingest_work(title, base)
            if paths:
                results.append(title)
                ingested.add(title)
                emit("ingested", source="wikisource", work_id=title, title=title)
        except Exception:
            pass
        time.sleep(1)

    # Save progress
    progress["ingested_works"] = sorted(ingested)
    progress["last_run"] = datetime.now(timezone.utc).isoformat()
    progress_path.write_text(json.dumps(progress, indent=2, ensure_ascii=False))

    return results


def _wikitext_to_markdown(wikitext: str) -> str:
    """Convert MediaWiki wikitext to clean markdown."""
    text = wikitext

    # Remove templates like {{header|...}}
    text = re.sub(r"\{\{[Hh]eader[^}]*\}\}", "", text)
    text = re.sub(r"\{\{[^}]{0,200}\}\}", "", text)

    # Convert wiki links [[target|label]] → label
    text = re.sub(r"\[\[(?:[^|\]]*\|)?([^\]]*)\]\]", r"\1", text)

    # Convert headers
    text = re.sub(r"^={5}\s*(.+?)\s*={5}", r"##### \1", text, flags=re.MULTILINE)
    text = re.sub(r"^={4}\s*(.+?)\s*={4}", r"#### \1", text, flags=re.MULTILINE)
    text = re.sub(r"^={3}\s*(.+?)\s*={3}", r"### \1", text, flags=re.MULTILINE)
    text = re.sub(r"^={2}\s*(.+?)\s*={2}", r"## \1", text, flags=re.MULTILINE)

    # Remove HTML tags
    text = re.sub(r"<ref[^>]*>.*?</ref>", "", text, flags=re.DOTALL)
    text = re.sub(r"<br\s*/?>", "\n", text)
    text = re.sub(r"<[^>]+>", "", text)

    # Bold/italic
    text = re.sub(r"'''(.+?)'''", r"**\1**", text)
    text = re.sub(r"''(.+?)''", r"*\1*", text)

    # Clean up
    text = re.sub(r"\n{3,}", "\n\n", text)
    text = text.strip()

    return text
