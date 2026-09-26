"""Ingest module: collect raw documents into the raw/ directory."""

import hashlib
import re
import shutil
import time
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urljoin, urlparse

import frontmatter
import requests
from requests import Response as _Response
from requests.utils import get_encoding_from_headers as _response_encoding
from bs4 import BeautifulSoup
from markdownify import markdownify as md

from .config import load_config, ensure_dirs
from .llm import strip_surrogates

_DEFAULT_REQUESTS_GET = requests.get
_PAGE_BYTES = 8 * 1024 * 1024
_IMAGE_BYTES = 4 * 1024 * 1024
_TOTAL_IMAGE_BYTES = 32 * 1024 * 1024


def _validate_url(url: str):
    """Inspect a public URL in Rust. The fetcher independently validates/pins DNS."""
    from . import _native
    _native.validate_fetch_url(url)


_DEFAULT_VALIDATE_URL = _validate_url


def _fetch_response(url: str, *, timeout: float, max_bytes: int, cancellation=None):
    """Default native transport, or an explicitly installed live Python hook."""
    from . import _native
    if requests.get is _DEFAULT_REQUESTS_GET:
        result = _native.fetch_url(url, options={"timeout_ms": max(1, int(timeout * 1000)),
            "connect_timeout_ms": max(1, min(10000, int(timeout * 1000))), "max_bytes": max_bytes},
            cancellation=cancellation)
        response = _Response()
        response.status_code = result["status"]
        response.url = result["url"]
        response.headers["content-type"] = result["content_type"]
        response.encoding = _response_encoding(response.headers)
        response._content = result["content"]
        return response

    # Arbitrary plugin code is a trusted callback boundary, not a Rust fetch.
    # Preserve its actual return value/exception; never reuse a native receipt.
    for hop in range(6):
        _validate_url(url)
        _native.python_boundary("fetch_url", "requests.get")
        response = requests.get(url, timeout=timeout, headers={"User-Agent": "LLMBase/1.0"},
                                allow_redirects=False)
        if response.status_code in (301, 302, 303, 307, 308):
            if hop == 5 or not response.headers.get("location"):
                raise ValueError("fetch redirect limit or invalid redirect")
            url = urljoin(url, response.headers["location"])
            continue
        response.raise_for_status()
        if len(response.content) > max_bytes:
            raise ValueError("fetch body size limit exceeded")
        return response
    raise ValueError("fetch redirect limit exceeded")


def ingest_url(url: str, base_dir: Path | None = None, *, cancellation=None) -> Path:
    """Fetch a web article and save as markdown with images downloaded locally."""
    from . import _native
    deadline = time.monotonic() + 120
    # The live validation helper remains observable to downstream callers. Default
    # native fetching rechecks every actual connection even if this hook changes.
    if cancellation is not None and cancellation.is_cancelled():
        raise ValueError("fetch cancelled")
    if _validate_url is not _DEFAULT_VALIDATE_URL:
        _validate_url(url)
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    raw_dir = Path(cfg["paths"]["raw"])

    # Fetch page
    resp = _fetch_response(url, timeout=30, max_bytes=_PAGE_BYTES, cancellation=cancellation)
    soup = BeautifulSoup(resp.text, "html.parser")

    # Extract title
    title = soup.title.string.strip() if soup.title and soup.title.string else urlparse(url).netloc
    slug = _slugify(title)

    # Keep conversion/network work outside the writer lease. Rust creates paths
    # only when the complete accepted article and its assets are ready to persist.
    doc_dir = raw_dir / slug
    images_dir = doc_dir / "images"
    assets = [(doc_dir / "source.original", resp.content)]
    asset_bytes = 0

    # Extract main content (try article tag, then body)
    article = soup.find("article") or soup.find("main") or soup.body
    if article is None:
        article = soup

    # Download images and rewrite URLs
    for img in article.find_all("img")[:32]:
        src = img.get("src")
        if not src:
            continue
        if cancellation is not None and cancellation.is_cancelled():
            raise ValueError("fetch cancelled")
        try:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or asset_bytes >= _TOTAL_IMAGE_BYTES:
                break
            src = urljoin(getattr(resp, "url", None) or url, src)
            img_resp = _fetch_response(src, timeout=min(15, remaining),
                max_bytes=min(_IMAGE_BYTES, _TOTAL_IMAGE_BYTES - asset_bytes), cancellation=cancellation)
            ext = _guess_ext(src, img_resp.headers.get("content-type", ""))
            img_hash = hashlib.md5(src.encode()).hexdigest()[:8]
            img_name = f"{img_hash}{ext}"
            img_path = images_dir / img_name
            assets.append((img_path, img_resp.content))
            asset_bytes += len(img_resp.content)
            img["src"] = f"images/{img_name}"
        except Exception:
            pass  # Skip failed image downloads

    # Convert to markdown (handle deeply nested HTML)
    import sys
    old_limit = sys.getrecursionlimit()
    sys.setrecursionlimit(10000)
    try:
        html_str = str(article)
        # Truncate extremely large pages
        if len(html_str) > 500000:
            html_str = html_str[:500000]
        content = md(html_str, heading_style="ATX", strip=["script", "style", "nav"])
    except RecursionError:
        # Fallback: extract text directly
        content = article.get_text(separator="\n\n", strip=True)
    finally:
        sys.setrecursionlimit(old_limit)

    # Sanitize lone surrogates from upstream HTML decode before they
    # reach disk — otherwise they sit in raw/ until a downstream LLM
    # call serialises them and crashes (see strip_surrogates docstring).
    post = frontmatter.Post(strip_surrogates(content))
    post.metadata["title"] = strip_surrogates(title)
    post.metadata["source"] = url
    post.metadata["ingested_at"] = datetime.now(timezone.utc).isoformat()
    post.metadata["type"] = "web_article"
    post.metadata["compiled"] = False
    post.metadata["original_file"] = "source.original"
    post.metadata["original_sha256"] = hashlib.sha256(resp.content).hexdigest()
    if resp.headers.get("content-type"):
        post.metadata["original_media_type"] = resp.headers["content-type"]

    doc_path = doc_dir / "index.md"
    serialized = frontmatter.dumps(post)
    if cancellation is not None and cancellation.is_cancelled():
        raise ValueError("fetch cancelled")
    def persist():
        if _native.available():
            for path, content_bytes in assets:
                relative = str(path.relative_to(raw_dir))
                if path.name == "source.original":
                    previous = _native.dispatch("compat_kb_raw_hash", {"path": relative}, base_dir, paths=cfg["paths"])["result"]
                    _native.dispatch("ingest_asset", {"path": relative, "content_bytes": list(content_bytes),
                        "expected_hash": previous["content_hash"]}, base_dir, paths=cfg["paths"])
                else:
                    _native.dispatch("compat_kb_raw_write", {"path": relative,
                        "content_bytes": list(content_bytes), "parse_markdown": False}, base_dir, paths=cfg["paths"])
            _native.dispatch("compat_kb_raw_write", {"path": str(doc_path.relative_to(raw_dir)),
                "content": serialized}, base_dir, paths=cfg["paths"])
        else:
            # Explicit unbuilt-checkout path: only a supplied Python fetch hook can
            # reach here, because the default HTTP transport requires native code.
            images_dir.mkdir(parents=True, exist_ok=True)
            for path, content_bytes in assets:
                path.write_bytes(content_bytes)
            doc_path.write_text(serialized, encoding="utf-8")
    _native.with_writer(base_dir, persist, paths=cfg["paths"])

    from .hooks import emit
    emit("ingested", source="web", url=url, title=title, path=str(doc_path))

    return doc_path


def ingest_file(file_path: str, base_dir: Path | None = None) -> Path:
    """Copy a local file (paper PDF, markdown, etc.) into raw/."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    raw_dir = Path(cfg["paths"]["raw"])

    src = Path(file_path)
    if not src.exists():
        raise FileNotFoundError(f"Source file not found: {file_path}")

    slug = _slugify(src.stem)
    doc_dir = raw_dir / slug
    doc_dir.mkdir(parents=True, exist_ok=True)

    dest = doc_dir / src.name
    from . import _native
    native_text = _native.available() and src.suffix.lower() in (".md", ".markdown", ".txt", ".py", ".json", ".csv")
    if native_text and src.resolve() == dest.resolve():
        raise shutil.SameFileError(f"{src!r} and {dest!r} are the same file")
    if not native_text:
        shutil.copy2(src, dest)

    # If it's already a markdown file, add frontmatter if missing
    if src.suffix.lower() in (".md", ".markdown"):
        # External-file parsing is an ingest adapter; accepted raw persistence is native.
        post = frontmatter.load(str(src if native_text else dest))
        if "title" not in post.metadata:
            post.metadata["title"] = src.stem
        if "ingested_at" not in post.metadata:
            post.metadata["ingested_at"] = datetime.now(timezone.utc).isoformat()
        post.metadata["type"] = "local_file"
        post.metadata["compiled"] = False
        # Sanitize before persistence — same reasoning as ingest_url.
        post.content = strip_surrogates(post.content)
        if isinstance(post.metadata.get("title"), str):
            post.metadata["title"] = strip_surrogates(post.metadata["title"])
        if native_text:
            _native.dispatch("compat_kb_raw_write", {"path": str(dest.relative_to(raw_dir)),
                "content": frontmatter.dumps(post), "permissions": src.stat().st_mode & 0o7777},
                base_dir, paths=cfg["paths"])
        else:
            dest.write_text(frontmatter.dumps(post), encoding="utf-8")
    else:
        # Create a companion metadata file
        meta = frontmatter.Post("")
        meta.metadata["title"] = src.stem
        meta.metadata["source"] = str(src.resolve())
        meta.metadata["ingested_at"] = datetime.now(timezone.utc).isoformat()
        meta.metadata["type"] = "local_file"
        meta.metadata["file"] = src.name
        meta.metadata["compiled"] = False
        meta_path = doc_dir / "index.md"
        if native_text:
            _native.dispatch("compat_kb_raw_write", {"path": str(dest.relative_to(raw_dir)),
                "content_bytes": list(src.read_bytes()), "parse_markdown": False,
                "permissions": src.stat().st_mode & 0o7777}, base_dir, paths=cfg["paths"])
            _native.dispatch("compat_kb_raw_write", {"path": str(meta_path.relative_to(raw_dir)),
                "content": frontmatter.dumps(meta)}, base_dir, paths=cfg["paths"])
        else:
            meta_path.write_text(frontmatter.dumps(meta), encoding="utf-8")

    from .hooks import emit
    emit("ingested", source="file", title=src.stem, path=str(dest))

    return dest


def ingest_directory(dir_path: str, base_dir: Path | None = None) -> list[Path]:
    """Ingest all supported files from a directory."""
    src_dir = Path(dir_path)
    if not src_dir.is_dir():
        raise NotADirectoryError(f"Not a directory: {dir_path}")

    results = []
    supported = {".md", ".markdown", ".txt", ".pdf", ".py", ".json", ".csv"}
    for f in sorted(src_dir.iterdir()):
        if f.is_file() and f.suffix.lower() in supported:
            results.append(ingest_file(str(f), base_dir))
    return results


def _safe_meta_value(v):
    """Coerce a frontmatter value to a JSON-safe type, redacting local paths."""
    if v is None:
        return v
    if isinstance(v, (int, float, bool)):
        return v
    if isinstance(v, str):
        return None if _is_local_path(v) else v
    if isinstance(v, (list, tuple)):
        return [x for x in (_safe_meta_value(i) for i in v) if x is not None]
    if isinstance(v, dict):
        return {str(k): sv for k, sv in ((str(k), _safe_meta_value(val)) for k, val in v.items()) if sv is not None}
    s = str(v)
    return None if _is_local_path(s) else s


def _sanitize_entry(entry: dict, raw_dir_str: str = "") -> dict:
    """Sanitize fields that may leak local filesystem paths.

    - ``path`` is converted to a relative slug (the part after ``raw/``)
    - Other string values that look like absolute filesystem paths are removed
    """
    sanitized = {}
    for k, v in entry.items():
        if k == "path" and isinstance(v, str):
            # Convert absolute path to relative slug for the frontend
            if raw_dir_str and raw_dir_str in v:
                sanitized[k] = v.split(raw_dir_str)[-1].lstrip("/\\")
            else:
                # Fallback: take last path component
                sanitized[k] = Path(v).name
            continue
        # Redact string values that look like absolute local paths
        if isinstance(v, str) and _is_local_path(v):
            continue
        sanitized[k] = v
    return sanitized


def _is_local_path(v: str) -> bool:
    """Check if a string looks like a local filesystem path.

    Requires at least two path components (e.g. /home/user) to avoid
    false-positives on site-relative URLs like /works/foo.
    """
    if v.startswith(("http://", "https://")):
        return False
    # Unix absolute paths with 2+ components (e.g. /home/user, not /works)
    if v.startswith("/") and v.count("/") >= 2:
        return True
    # Windows absolute paths: C:\... or UNC \\server\...
    if len(v) >= 3 and v[1] == ":" and v[2] in ("/", "\\"):
        return True
    if v.startswith("\\\\"):
        return True
    return False


def list_raw(base_dir: Path | None = None) -> list[dict]:
    """List all raw documents with their metadata."""
    from . import _native
    if _native.available():
        return _native.dispatch("compat_kb_raw_list", {}, base_dir)["result"]
    cfg = load_config(base_dir)
    raw_dir = Path(cfg["paths"]["raw"])
    if not raw_dir.exists():
        return []

    raw_dir_str = str(raw_dir) + "/"
    docs = []
    for doc_dir in sorted(raw_dir.iterdir()):
        if not doc_dir.is_dir():
            continue
        index_path = doc_dir / "index.md"
        if index_path.exists():
            post = frontmatter.load(str(index_path))
            entry = {
                "path": str(doc_dir),
                "title": post.metadata.get("title", doc_dir.name),
                "type": post.metadata.get("type", "unknown"),
                "compiled": post.metadata.get("compiled", False),
                "ingested_at": post.metadata.get("ingested_at", ""),
            }
            # Include all frontmatter fields so downstream can group/filter
            # (e.g. by work, chapter, source, canon, etc.)
            for k, v in post.metadata.items():
                if k not in entry:
                    entry[k] = _safe_meta_value(v)
            docs.append(_sanitize_entry(entry, raw_dir_str))
        else:
            # Check for any markdown file
            md_files = list(doc_dir.glob("*.md"))
            if md_files:
                post = frontmatter.load(str(md_files[0]))
                entry = {
                    "path": str(doc_dir),
                    "title": post.metadata.get("title", doc_dir.name),
                    "type": post.metadata.get("type", "unknown"),
                    "compiled": post.metadata.get("compiled", False),
                    "ingested_at": post.metadata.get("ingested_at", ""),
                }
                for k, v in post.metadata.items():
                    if k not in entry:
                        entry[k] = _safe_meta_value(v)
                docs.append(_sanitize_entry(entry, raw_dir_str))
    return docs


def _slugify(text: str) -> str:
    """Convert text to a filesystem-safe slug."""
    text = text.lower().strip()
    text = re.sub(r"[^\w\s-]", "", text)
    text = re.sub(r"[\s_]+", "-", text)
    text = re.sub(r"-+", "-", text)
    return text[:80].strip("-")


def _guess_ext(url: str, content_type: str) -> str:
    """Guess image file extension."""
    ct_map = {
        "image/png": ".png",
        "image/jpeg": ".jpg",
        "image/gif": ".gif",
        "image/webp": ".webp",
        "image/svg+xml": ".svg",
    }
    for ct, ext in ct_map.items():
        if ct in content_type:
            return ext
    # Try from URL
    path = urlparse(url).path
    for ext in (".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg"):
        if path.lower().endswith(ext):
            return ext
    return ".png"
