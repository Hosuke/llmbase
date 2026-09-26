"""Native query sequence with actual loopback HTTP and executable Python hooks."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading

import frontmatter
import pytest
import yaml

from llmwiki import _native
import llmwiki.query as query
import llmwiki.compile as compiler
import llmwiki.llm as llm
from llmwiki.config import load_config


@pytest.fixture(autouse=True)
def candidate():
    if not _native.available():
        pytest.skip("requires installed native candidate")


@pytest.fixture
def library(tmp_path):
    config={"llm":{"max_tokens":300},"paths":{"raw":"raw","concepts":"knowledge/articles",
            "outputs":"knowledge/answers","meta":"state","wiki":"knowledge"}}
    (tmp_path/"config.yaml").write_text(yaml.safe_dump(config))
    for path in config["paths"].values():
        (tmp_path/path).mkdir(parents=True,exist_ok=True)
    rows=[]
    for slug,title in [("a","Alpha"),("b","Beta"),("c","Gamma")]:
        (tmp_path/f"knowledge/articles/{slug}.md").write_text(
            f"---\ntitle: {title}\nsummary: knowledge evidence\n---\n原典𠮷 knowledge evidence.")
        rows.append({"slug":slug,"title":title,"summary":"knowledge evidence"})
    (tmp_path/"state/index.json").write_text(json.dumps(rows))
    (tmp_path/"state/_index.md").write_text("old index")
    return tmp_path


def rejected(*args,**kwargs):
    raise AssertionError("native query called a Python legacy core")


@contextmanager
def provider(monkeypatch,responses):
    calls=[]
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args):
            pass
        def do_POST(self):
            calls.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            text=responses[min(len(calls)-1,len(responses)-1)]
            payload=json.dumps({"choices":[{"message":{"content":text},"finish_reason":"stop"}],
                                "usage":{"total_tokens":9}}).encode()
            self.send_response(200);self.send_header("Content-Length",str(len(payload)));self.end_headers();self.wfile.write(payload)
    server=ThreadingHTTPServer(("127.0.0.1",0),Handler)
    worker=threading.Thread(target=server.serve_forever,kwargs={"poll_interval":0.01},daemon=True);worker.start()
    monkeypatch.setenv("LLMBASE_BASE_URL",f"http://127.0.0.1:{server.server_port}/v1")
    monkeypatch.setenv("LLMBASE_API_KEY","fixture-query-secret")
    monkeypatch.setenv("LLMBASE_PRIMARY_RETRIES","1")
    monkeypatch.setenv("LLMBASE_FALLBACK_MODELS","")
    try:
        yield calls
    finally:
        server.shutdown();server.server_close();worker.join(timeout=2)


def test_default_deep_uses_rust_sequence_real_http_and_journal_filing(library,monkeypatch):
    for name in ["_legacy_query_with_search","_legacy_gather_context","_legacy_file_output"]:
        monkeypatch.setattr(query,name,rejected)
    monkeypatch.setattr(frontmatter,"load",rejected)
    with provider(monkeypatch,["Alpha\nBeta\nGamma","A grounded-looking answer [source titles]."]) as calls:
        answer=query.query_with_search("knowledge evidence",base_dir=library,file_back=True,return_context=True)
    assert len(calls)==2
    assert answer["answer"]=="A grounded-looking answer [source titles]."
    assert len(answer["consulted"])==3
    assert not Path(answer["output_path"]).is_absolute()
    saved=frontmatter.loads((library/answer["output_path"]).read_text())
    assert saved["type"]=="query_markdown"
    assert saved["query_provenance"]["library_id"]=="default"
    trace=_native.last_trace()
    assert trace["orchestrator"]=="rust" and trace["writes"] is True
    assert trace["stages"]==["index","selector","answer","file_output"]
    assert llm.last_call_trace()["core"]=="rust"
    assert "fixture-query-secret" not in (library/answer["output_path"]).read_text()


def test_live_system_tone_context_and_model_callbacks_remain_executable(library,monkeypatch):
    seen=[]
    def selector(prompt,**kwargs):
        query.SYSTEM_PROMPT="changed during selector"
        query.TONE_INSTRUCTIONS["runtime"]="runtime tone"
        seen.append(("selector",kwargs))
        return "Alpha\nBeta\nGamma"
    def answer(question,contexts,**kwargs):
        seen.append(("answer",kwargs))
        assert kwargs["system"]=="changed during selector\n\nruntime tone"
        assert len(contexts)==3
        return "callback answer"
    monkeypatch.setattr(query,"SYSTEM_PROMPT","initial")
    monkeypatch.setattr(query,"TONE_INSTRUCTIONS",{})
    monkeypatch.setattr(query,"chat",selector)
    monkeypatch.setattr(query,"chat_with_context",answer)
    result=query.query_with_search("knowledge",base_dir=library,tone="runtime",model="live-model",api_key="live-secret")
    assert result=="callback answer"
    assert all(kwargs["model"]=="live-model" and kwargs["api_key"]=="live-secret" for _,kwargs in seen)
    assert _native.last_trace()["model_boundary"]=="host_callback"
    assert list((library/"knowledge/answers").iterdir())==[]


def test_default_promotion_is_an_actual_guarded_article_and_runtime_bare_profile(library,monkeypatch):
    monkeypatch.setattr(query,"_legacy_promote_to_concept",rejected)
    monkeypatch.setattr(compiler,"SECTION_HEADERS",[("文言","")])
    decision={"promote":True,"slug":"xue","title":"學","summary":"學之義","tags":["學"],"content":"學者，覺也。原典𠮷"}
    with provider(monkeypatch,[json.dumps(decision,ensure_ascii=False)]) as calls:
        result=query.promote_to_concept("何謂學？","學者，覺也。",[],[],base_dir=library)
    assert result["promoted"] is True and result["merged"] is False
    article=frontmatter.loads(Path(result["path"]).read_text())
    assert article.content==decision["content"] and article["sources"][0]["plugin"]=="qa"
    assert "## 文言" not in calls[0]["messages"][1]["content"]
    assert _native.last_trace()["stages"]==["judge"]
    assert any(row["slug"]=="xue" for row in json.loads((library/"state/index.json").read_text()))


def test_native_context_excludes_private_and_stale_projection_text(library):
    (library/"knowledge/articles/b.md").write_text("---\ntitle: PrivateTitle\nprivate: true\n---\nSECRET knowledge evidence")
    (library/"state/_index.md").write_text("SECRET stale summary")
    contexts=query._gather_context("knowledge evidence",load_config(library))
    assert "SECRET" not in json.dumps(contexts)
    assert "PrivateTitle" not in json.dumps(contexts)
    assert _native.last_trace()["orchestrator"]=="rust"


def test_unindexed_private_edit_during_answer_blocks_fileback(library,monkeypatch):
    def answer(*args,**kwargs):
        (library/"knowledge/articles/a.md").write_text("---\ntitle: Alpha\nprivate: true\n---\nsecret")
        return "answer from earlier context"
    monkeypatch.setattr(query,"chat_with_context",answer)
    with pytest.raises(RuntimeError,match="changed"):
        query.query("knowledge evidence",base_dir=library,file_back=True)
    assert list((library/"knowledge/answers").iterdir())==[]


def test_pure_prefilter_honors_live_tokenizer_and_never_creates_files(tmp_path,monkeypatch):
    import llmwiki.search as search
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(query,"load_config",rejected)
    monkeypatch.setattr(query,"_legacy_bm25_prefilter",rejected)
    monkeypatch.setattr(search,"SEARCH_TOKENIZER",lambda text: ["shared"] if "wanted" in text else [])
    rows=[{"slug":"a","title":"other"},{"slug":"b","title":"wanted"}]
    assert query._bm25_prefilter("wanted",rows,1)==[rows[1]]
    assert _native.last_trace()["operation"]=="query_transform"
    assert list(tmp_path.iterdir())==[]


def test_explicit_none_returning_file_hook_does_not_trigger_native_write(library,monkeypatch):
    calls=[]
    monkeypatch.setattr(query,"chat_with_context",lambda *args,**kwargs:"answer")
    monkeypatch.setattr(query,"_file_output",lambda *args:calls.append(args))
    result=query.query("knowledge",base_dir=library,file_back=True,return_path=True)
    assert result=={"answer":"answer","output_path":None}
    assert len(calls)==1
    assert list((library/"knowledge/answers").iterdir())==[]
    assert _native.last_trace()["host_overrides"]==["file_output"]
