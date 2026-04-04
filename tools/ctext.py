"""ctext.org integration — ingest classical Chinese texts from the Chinese Text Project."""

import re
import time
from datetime import datetime, timezone
from pathlib import Path

import frontmatter
import requests
from bs4 import BeautifulSoup

from .config import load_config, ensure_dirs

BASE_URL = "https://ctext.org"
HEADERS = {"User-Agent": "LLMBase/1.0 (research)"}

# Pre-defined book catalogs
CATALOGS = {
    "confucianism": "/confucianism/zh",
    "daoism": "/daoism/zh",
    "mohism": "/mohism/zh",
    "legalism": "/legalism/zh",
    "military": "/art-of-war/zh",
    "histories": "/histories/zh",
    "medicine": "/medicine/zh",
}


def fetch_text(url: str, use_browser: bool = False) -> str:
    """Fetch page and extract classical text content from ctext.org.

    Falls back to opencli browser if HTTP fetch fails or use_browser=True.
    """
    if not use_browser:
        try:
            return _fetch_text_http(url)
        except Exception:
            # Fall back to browser if HTTP fails (anti-scraping, etc.)
            pass

    return _fetch_text_browser(url)


def _fetch_text_http(url: str) -> str:
    """Fetch via direct HTTP request."""
    resp = requests.get(url, headers=HEADERS, timeout=30)
    resp.raise_for_status()
    soup = BeautifulSoup(resp.text, "html.parser")

    blocks = soup.select("td.ctext")
    if blocks:
        parts = [td.get_text(strip=True) for td in blocks if td.get_text(strip=True)]
        return "\n\n".join(parts)

    main = soup.find("div", id="content3") or soup.find("div", class_="container") or soup.body
    return main.get_text(separator="\n\n", strip=True) if main else ""


def _fetch_text_browser(url: str) -> str:
    """Fetch via opencli browser (handles JS-rendered pages and anti-scraping)."""
    from .browser import is_opencli_available, extract_text
    if not is_opencli_available():
        raise RuntimeError("opencli not installed. Install: npm install -g @jackwener/opencli")
    return extract_text(url)


def fetch_chapter_list(index_url: str) -> list[tuple[str, str]]:
    """Fetch chapter list from a book's index page."""
    resp = requests.get(index_url, headers=HEADERS, timeout=30)
    resp.raise_for_status()
    soup = BeautifulSoup(resp.text, "html.parser")

    chapters = []
    content = soup.find("div", id="content3") or soup.find("div", id="content2") or soup.body
    if not content:
        return chapters

    for a in content.find_all("a", href=True):
        href = a["href"]
        title = a.get_text(strip=True)
        if title and href.endswith("/zh") and not href.startswith("http"):
            if not href.startswith("/"):
                href = "/" + href
            chapters.append((title, href))

    return chapters


def fetch_book_list(catalog_url: str) -> list[tuple[str, str]]:
    """Fetch list of books from a catalog page (e.g. /confucianism/zh)."""
    resp = requests.get(catalog_url, headers=HEADERS, timeout=30)
    resp.raise_for_status()
    soup = BeautifulSoup(resp.text, "html.parser")

    books = []
    content = soup.find("div", id="content3") or soup.find("div", id="content2") or soup.body
    if not content:
        return books

    for a in content.find_all("a", href=True):
        href = a["href"]
        title = a.get_text(strip=True)
        if title and href.endswith("/zh") and not href.startswith("http"):
            if not href.startswith("/"):
                href = "/" + href
            books.append((title, href))

    return books


def ingest_chapter(
    book_name: str,
    chapter_name: str,
    chapter_url: str,
    base_dir: Path | None = None,
    use_browser: bool = False,
) -> Path | None:
    """Ingest a single chapter from ctext.org."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    raw_dir = Path(cfg["paths"]["raw"])

    content = fetch_text(chapter_url, use_browser=use_browser)
    if not content or len(content) < 10:
        return None

    slug = re.sub(r"[^\w]+", "-", f"{book_name}-{chapter_name}").strip("-")
    doc_dir = raw_dir / slug
    doc_dir.mkdir(parents=True, exist_ok=True)

    post = frontmatter.Post(content)
    post.metadata["title"] = f"{book_name} · {chapter_name}"
    post.metadata["source"] = chapter_url
    post.metadata["ingested_at"] = datetime.now(timezone.utc).isoformat()
    post.metadata["type"] = "classical_text"
    post.metadata["book"] = book_name
    post.metadata["chapter"] = chapter_name
    post.metadata["compiled"] = False

    doc_path = doc_dir / "index.md"
    doc_path.write_text(frontmatter.dumps(post), encoding="utf-8")
    return doc_path


def ingest_book(
    book_name: str,
    book_path: str,
    delay: float = 1.5,
    base_dir: Path | None = None,
    use_browser: bool = False,
) -> list[Path]:
    """Ingest all chapters of a book from ctext.org."""
    index_url = BASE_URL + book_path
    chapters = fetch_chapter_list(index_url)
    time.sleep(delay)

    results = []
    if not chapters:
        path = ingest_chapter(book_name, "全文", index_url, base_dir, use_browser)
        if path:
            results.append(path)
        return results

    for ch_name, ch_path in chapters:
        ch_url = BASE_URL + ch_path
        try:
            path = ingest_chapter(book_name, ch_name, ch_url, base_dir, use_browser)
            if path:
                results.append(path)
        except Exception:
            pass
        time.sleep(delay)

    return results


def ingest_catalog(
    catalog: str,
    delay: float = 1.5,
    base_dir: Path | None = None,
    use_browser: bool = False,
) -> dict[str, list[Path]]:
    """Ingest all books from a catalog (e.g. 'confucianism')."""
    catalog_path = CATALOGS.get(catalog)
    if not catalog_path:
        raise ValueError(f"Unknown catalog: {catalog}. Available: {list(CATALOGS.keys())}")

    catalog_url = BASE_URL + catalog_path
    books = fetch_book_list(catalog_url)
    time.sleep(delay)

    results = {}
    for book_name, book_path in books:
        paths = ingest_book(book_name, book_path, delay, base_dir, use_browser)
        results[book_name] = paths

    return results
