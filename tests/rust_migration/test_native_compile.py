"""Installed-wheel compile facade parity with real native reads/writes and HTTP."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import sqlite3
import threading

import frontmatter
import pytest
import yaml

from llmwiki import _native
import llmwiki.compile as compiler


@pytest.fixture(autouse=True)
def native_candidate():
    if not _native.available():
        pytest.skip("native candidate extension is not built")


@pytest.fixture
def library(tmp_path):
    cfg={"llm":{"max_tokens":400},"compile":{"batch_size":1},
         "paths":{"raw":"originals","wiki":"knowledge","concepts":"knowledge/entries","outputs":"knowledge/answers","meta":"state"},
         "languages":{"default":"bare","profiles":{"bare":{"sections":[{"code":"wy","key":"文言","header":""}]}}}}
    (tmp_path/"config.yaml").write_text(yaml.safe_dump(cfg,allow_unicode=True))
    for path in cfg["paths"].values():
        (tmp_path/path).mkdir(parents=True,exist_ok=True)
    (tmp_path/"originals/book").mkdir()
    raw=b"---\r\ntitle: Source\r\ntype: buddhist_sutra\r\nwork_id: B1\r\nsource: https://example.test/source\r\nunknown: 42\r\n---\r\n  " + "原典𠮷 café".encode()+b"\r\nTail \t\r\n"
    (tmp_path/"originals/book/index.md").write_bytes(raw)
    return tmp_path,raw


def reject_legacy(*_args,**_kwargs):
    raise AssertionError("default native compile entered legacy Python core")


def tagged(slug="concept",body="A substantive knowledge article."):
    return f"===ARTICLE===\nslug: {slug}\ntitle: Topic / 原典\nsummary: Summary\ntags: topic\n---\n{body}\n===END==="


@contextmanager
def provider(monkeypatch,text,finish="stop"):
    calls=[]
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*_):
            pass
        def do_POST(self):
            calls.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            raw=json.dumps({"choices":[{"message":{"content":text},"finish_reason":finish}],"usage":{"total_tokens":12}}).encode()
            self.send_response(200)
            self.send_header("Content-Length",str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
    server=ThreadingHTTPServer(("127.0.0.1",0),Handler)
    worker=threading.Thread(target=server.serve_forever,kwargs={"poll_interval":0.01},daemon=True)
    worker.start()
    monkeypatch.setenv("LLMBASE_BASE_URL",f"http://127.0.0.1:{server.server_port}/v1")
    monkeypatch.setenv("LLMBASE_API_KEY","fixture-compile-token")
    monkeypatch.setenv("LLMBASE_PRIMARY_RETRIES","1")
    monkeypatch.setenv("LLMBASE_FALLBACK_MODELS","")
    try:
        yield calls
    finally:
        server.shutdown();server.server_close();worker.join(timeout=2)


def test_pure_helpers_use_rust_without_config_or_files_and_keep_header_assignment(tmp_path,monkeypatch):
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(compiler,"load_config",reject_legacy)
    monkeypatch.setattr(compiler,"_legacy_parse_compile_response",reject_legacy)
    monkeypatch.setattr(compiler,"_legacy_split_sections",reject_legacy)
    monkeypatch.setattr(compiler,"_legacy_assemble_sections",reject_legacy)
    anchor=compiler.DEFAULT_SECTION_HEADERS
    monkeypatch.setattr(compiler,"SECTION_HEADERS",[("文言","")])
    assert compiler._split_sections("原典𠮷") == {"_preamble":"","文言":"原典𠮷"}
    assert compiler._assemble_sections({"文言":"文。"}) == "文。"
    monkeypatch.setattr(compiler,"SECTION_HEADERS",[("english","## English")])
    assert compiler._split_sections("## ENGLISH\nBody")["english"].strip()=="Body"
    assert compiler.DEFAULT_SECTION_HEADERS is anchor
    assert compiler._parse_compile_response(tagged())[0]["slug"]=="concept"
    assert compiler._parse_update_block("slug: x\nappend: |\n  More") == {"slug":"x","content":"More"}
    assert compiler.sanitize_slug("http://host/a?b#c")=="http-host-a-b-c"
    assert _native.last_trace()["storage"]=="none"
    assert list(tmp_path.iterdir())==[]


def test_private_read_helpers_preserve_paths_and_use_actual_native_file_reads(library,monkeypatch):
    root,_=library
    raw=root/"originals"
    concepts=root/"knowledge/entries"
    (concepts/"xue.md").write_text("---\ntitle: 學\n---\nBody.")
    monkeypatch.setattr(frontmatter,"load",reject_legacy)
    monkeypatch.setattr(compiler,"_legacy_find_uncompiled",reject_legacy)
    monkeypatch.setattr(compiler,"_legacy_list_existing_concepts",reject_legacy)
    assert compiler._find_uncompiled(raw)==[raw/"book/index.md"]
    assert compiler._list_existing_concepts(concepts)==["xue (學)"]
    assert compiler._find_uncompiled(raw/"absent")==[]
    assert not list(root.rglob("llmbase.sqlite3"))


def test_default_merge_preserves_bytes_until_strict_threshold_and_imported_shape(tmp_path,monkeypatch):
    path=tmp_path/"entry.md"
    path.write_text("---\ntitle: Original\nunknown: 42\ntags: [old]\n---\nabcde")
    original=path.read_bytes()
    monkeypatch.setattr(frontmatter,"load",reject_legacy)
    monkeypatch.setattr(compiler,"_legacy_merge_into",reject_legacy)
    assert compiler._merge_into(path,{"content":"abcdef","tags":["ignored"]},headers=[("文言","")]) is None
    assert path.read_bytes()==original
    compiler._merge_into(path,{"content":"abcdefg𠮷","tags":["new"]},headers=[("文言","")])
    merged=frontmatter.loads(path.read_text())
    assert merged.content=="abcdefg𠮷"
    assert merged["unknown"]==42
    assert merged["tags"]==["new","old"]
    assert _native.last_trace()["operation"]=="compat_compile_write"


def test_real_http_compile_preserves_raw_body_hooks_metadata_and_uses_native_storage(library,monkeypatch):
    root,raw=library
    events=[]
    import llmwiki.hooks as hooks
    import llmwiki.taxonomy as taxonomy
    monkeypatch.setattr(hooks,"emit",lambda event,**payload:events.append((event,payload)))
    assigned=[]
    monkeypatch.setattr(taxonomy,"assign_new_articles",lambda base:assigned.append(base))
    monkeypatch.setattr(frontmatter,"load",reject_legacy)
    for name in ("_legacy_compile_new","_legacy_write_article","_legacy_parse_compile_response","_legacy_rebuild_index"):
        monkeypatch.setattr(compiler,name,reject_legacy)
    monkeypatch.setattr(compiler,"SYSTEM_PROMPT","Runtime system prompt.")
    monkeypatch.setattr(compiler,"COMPILE_USER_PROMPT","{title}|{content}|{existing}|{article_format}")
    monkeypatch.setattr(compiler,"COMPILE_ARTICLE_FORMAT","Runtime bare-body format.")
    with provider(monkeypatch,tagged(body="原典之義，當據原文而論。")) as calls:
        result=compiler.compile_new(root)
    assert result==[str(root/"knowledge/entries/concept.md")]
    assert calls[0]["messages"][0]["content"]=="Runtime system prompt."
    assert "Runtime bare-body format." in calls[0]["messages"][1]["content"]
    assert "原典𠮷" in calls[0]["messages"][1]["content"]
    current=(root/"originals/book/index.md").read_bytes()
    assert current.split(b"---\n",2)[2]==raw.split(b"---\r\n",2)[2]
    metadata=frontmatter.loads(current.decode()).metadata
    assert metadata["compiled"] is True and metadata["unknown"]==42
    assert metadata["compiled_at"]
    assert json.loads((root/"state/compiled_sources.json").read_text())==["https://example.test/source"]
    with sqlite3.connect(root/"state/llmbase.sqlite3") as db:
        assert any(row[0]==raw for row in db.execute("SELECT original FROM revisions"))
    assert [event for event,_ in events]==["before_compile","compiled","index_rebuilt","after_compile_batch"]
    compiled=events[1][1]
    assert compiled["source"]=="cbeta" and compiled["work_id"]=="B1"
    assert assigned==[root]
    assert _native.last_trace()["operation"]=="compat_compile_rebuild"
    assert compiler.compile_new(root)==[]


@pytest.mark.parametrize("text,finish",[("unparseable","stop"),(tagged(),"length"),(tagged(body=""),"stop")])
def test_invalid_or_truncated_default_provider_output_never_marks_source_compiled(library,monkeypatch,text,finish):
    root,raw=library
    with provider(monkeypatch,text,finish):
        with pytest.raises(ValueError):
            compiler.compile_new(root)
    assert (root/"originals/book/index.md").read_bytes()==raw
    assert list((root/"knowledge/entries").glob("*.md"))==[]


def test_live_compile_chat_override_and_empty_header_profile_still_execute(library,monkeypatch):
    root,_=library
    calls=[]
    monkeypatch.setattr(compiler,"chat",lambda prompt,**kwargs:calls.append((prompt,kwargs)) or tagged("hooked","文言正文。"))
    monkeypatch.setattr(compiler,"SECTION_HEADERS",[("文言","")])
    import llmwiki.taxonomy as taxonomy
    monkeypatch.setattr(taxonomy,"assign_new_articles",lambda *_:None)
    result=compiler.compile_new(root)
    assert calls and result==[str(root/"knowledge/entries/hooked.md")]
    assert frontmatter.loads(Path(result[0]).read_text()).content=="文言正文。"
    assert _native.last_trace()["operation"]=="compat_compile_rebuild"
