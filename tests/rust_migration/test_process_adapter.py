"""Actual isolated parser process, with no persistence delegated to Python."""
import hashlib
import json
import subprocess
import sys

import pymupdf


def invoke(kind, data):
    return subprocess.run([sys.executable, "-m", "llmwiki.process_adapter", kind],
        input=json.dumps({"protocol_version": 1, "library_id": "fixture", "input": data}),
        text=True, capture_output=True, timeout=10)


def test_html_process_returns_real_text_and_exact_original_without_running_scripts():
    original = '<html><title>學問𠮷</title><script>SECRET-EVAL</script><article><h1>學問</h1><p>學而時習之。</p><a href="/chapter">原典</a></article></html>'
    process = invoke("html", {"html": original, "url": "https://example.com/book"})
    assert process.returncode == 0, process.stderr
    result = json.loads(process.stdout)
    assert "學而時習之。" in result["documents"][0]["content"]
    assert "https://example.com/chapter" in result["documents"][0]["content"]
    assert "SECRET-EVAL" not in result["documents"][0]["content"]
    assert bytes(result["assets"][0]["content_bytes"]) == original.encode()
    assert hashlib.sha256(original.encode()).hexdigest() in result["documents"][0]["content"]


def test_pdf_process_extracts_chunks_but_preserves_one_exact_original():
    with pymupdf.open() as pdf:
        for text in ["Page one evidence.", "Page two evidence."]:
            page = pdf.new_page()
            page.insert_text((72, 72), text)
        original = pdf.tobytes()
    process = invoke("pdf", {"filename": "paper.pdf", "content_bytes": list(original), "chunk_pages": 1})
    assert process.returncode == 0, process.stderr
    result = json.loads(process.stdout)
    assert len(result["documents"]) == 2
    assert "Page one evidence." in result["documents"][0]["content"]
    assert "Page two evidence." in result["documents"][1]["content"]
    assert len(result["assets"]) == 1
    assert bytes(result["assets"][0]["content_bytes"]) == original


def test_empty_or_invalid_parser_input_is_nonzero_without_source_echo():
    process = invoke("html", {"html": "<script>DO-NOT-ECHO</script>", "url": "https://example.com"})
    assert process.returncode != 0
    assert process.stdout == ""
    assert "DO-NOT-ECHO" not in process.stderr
