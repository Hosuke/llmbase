"""Explicit HTML/PDF parser processes for native LLMBase.

No storage, model, network, or job orchestration runs here. Input comes from stdin;
the Rust host validates and journals returned documents. Stdout is protocol only.
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from urllib.parse import urljoin, urlsplit

MAX_INPUT = 16 * 1024 * 1024


def _document(path: str, metadata: dict, body: str) -> dict:
    if not body.strip():
        raise ValueError("parser produced no text")
    # A JSON flow mapping is valid YAML; Unicode and numeric metadata survive.
    content = "---\n" + json.dumps(metadata, ensure_ascii=False, indent=2) + "\n---\n" + body
    return {"path": path, "content": content}


def parse_html(data: dict) -> list[dict]:
    from bs4 import BeautifulSoup
    from markdownify import markdownify

    html, url = data.get("html"), data.get("url")
    if not isinstance(html, str) or not isinstance(url, str):
        raise ValueError("html and url are required")
    parsed = urlsplit(url)
    if parsed.scheme not in ("http", "https") or not parsed.hostname or parsed.username or parsed.password:
        raise ValueError("source URL must be HTTP(S) without credentials")
    soup = BeautifulSoup(html, "html.parser")
    title = soup.title.get_text(" ", strip=True) if soup.title else (parsed.hostname or "Web source")
    for element in soup(["script", "style", "noscript", "template", "iframe"]):
        element.decompose()
    content = soup.find("article") or soup.find("main") or soup.body or soup
    for element in content.find_all(["a", "img"]):
        attr = "href" if element.name == "a" else "src"
        destination = urljoin(url, element.get(attr, ""))
        if urlsplit(destination).scheme in ("http", "https"):
            element[attr] = destination
        else:
            element.attrs.pop(attr, None)
    body = markdownify(str(content), heading_style="ATX").strip()
    slug = "web-" + hashlib.sha256(url.encode()).hexdigest()[:20]
    return [_document(f"{slug}/index.md", {
        "title": title, "source": url, "type": "url", "compiled": False,
        "source_content_hash": hashlib.sha256(html.encode()).hexdigest(),
        "original_file": "source.original", "original_sha256": hashlib.sha256(html.encode()).hexdigest(),
        "original_media_type": "text/html",
        "parser": "python_html_process_v1",
    }, body)]


def parse_pdf(data: dict) -> list[dict]:
    import pymupdf

    values, filename, chunk_pages = data.get("content_bytes"), data.get("filename"), data.get("chunk_pages", 20)
    if not isinstance(values, list) or any(type(v) is not int or not 0 <= v <= 255 for v in values):
        raise ValueError("PDF bytes are required")
    if not isinstance(filename, str) or len(filename) > 255 or re.search(r"[/\\\x00]", filename):
        raise ValueError("invalid PDF filename")
    if type(chunk_pages) is not int or not 0 <= chunk_pages <= 1000:
        raise ValueError("chunk_pages must be0..1000")
    source = bytes(values)
    digest = hashlib.sha256(source).hexdigest()
    with pymupdf.open(stream=source, filetype="pdf") as pdf:
        if pdf.needs_pass or not 0 < len(pdf) <= 10000:
            raise ValueError("PDF requires a password or exceeds page bounds")
        size = chunk_pages or len(pdf)
        if (len(pdf) + size - 1) // size > 64:
            raise ValueError("PDF needs more than64 output chunks; increase chunk_pages")
        documents = []
        for start in range(0, len(pdf), size):
            end = min(start + size, len(pdf))
            body = "\n\n".join(pdf[index].get_text() for index in range(start, end))
            documents.append(_document(f"pdf-{digest[:20]}/part-{start+1}.md", {
                "title": f"{filename} (pages {start+1}-{end})", "type": "pdf", "compiled": False,
                "file": filename, "page_start": start + 1, "page_end": end,
                "source_content_hash": digest, "parser": "python_pdf_process_v1",
                "original_file": "source.original", "original_sha256": digest,
                "original_media_type": "application/pdf",
            }, body))
    return documents


def main() -> int:
    try:
        raw = sys.stdin.buffer.read(MAX_INPUT + 1)
        if len(raw) > MAX_INPUT:
            raise ValueError("input bound exceeded")
        envelope = json.loads(raw)
        if envelope.get("protocol_version") != 1 or not isinstance(envelope.get("input"), dict):
            raise ValueError("protocol v1 input required")
        parsers = {"html": parse_html, "pdf": parse_pdf}
        if len(sys.argv) != 2 or sys.argv[1] not in parsers:
            raise ValueError("choose html or pdf")
        documents = parsers[sys.argv[1]](envelope["input"])
        original = (envelope["input"]["html"].encode() if sys.argv[1] == "html"
                    else bytes(envelope["input"]["content_bytes"]))
        parent = documents[0]["path"].rsplit("/", 1)[0]
        assets = [{"path": f"{parent}/source.original", "content_bytes": list(original)}]
        print(json.dumps({"protocol_version": 1, "documents": documents, "assets": assets}, ensure_ascii=False))
        return 0
    except Exception:
        # Provider/source content and filesystem paths never enter error output.
        print("Parser failed: check format, installed parser dependency, and input bounds.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
