#!/usr/bin/env python3
"""Finite offline native lifecycle proof; Python is a test harness, never a core dependency.

On Linux --chroot copies only the release ELF and its shared libraries into an
empty runtime root. The HTTP fixture and this controller stay outside that root.
No real provider, credential, production document, or semantic-quality claim.

After the first CLI lexical search/evidence resolution, the same chroot prefix,
binary, base and fixture also prove the release executable's real transports:
the Agent HTTP surface (`serve`) with authenticated reads, a probe write/read/
delete lifecycle, scope rejections and one gated SSE ask stream, and the MCP
stdio protocol (`mcp`) with initialization, discovery, tool calls, a protocol
error and clean EOF exit. All child servers are terminated inside the proof.
"""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import uuid


REQUESTS = []
# One-shot provider gate, armed only around the HTTP SSE probe below. When
# armed, the fixture pauses after its first streamed delta until the controller
# has received that forwarded delta, proving forwarding before completion.
DELTA_GATE = threading.Event()
GATE_ARMED = threading.Event()
GATE_ENTERED = threading.Event()
GATE_EXPIRED = threading.Event()
GATE_RELEASED = threading.Event()
GATE_TIMEOUT = 30


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["content-length"])))
        REQUESTS.append({"path": self.path, "stream": request.get("stream", False)})
        if self.path.endswith("/embeddings"):
            data = []
            for index, value in enumerate(request["input"]):
                vector = [1.0, 0.0, 0.0] if "orchard" in value.lower() else [0.0, 1.0, 0.0]
                data.append({"index": index, "embedding": vector})
            body = {"model": "fixture-embedding", "data": data, "usage": {"prompt_tokens": 8, "total_tokens": 8}}
        elif request.get("stream"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            for index, piece in enumerate(["Orchards ", "grow fruit [E1]."]):
                gated = index == 0 and GATE_ARMED.is_set()
                if gated:
                    GATE_ENTERED.set()
                frame = {"model": "fixture-generation", "choices": [{"index": 0, "delta": {"content": piece}, "finish_reason": None}]}
                self.wfile.write(f"data: {json.dumps(frame)}\n\n".encode())
                self.wfile.flush()
                if gated:
                    if not DELTA_GATE.wait(GATE_TIMEOUT):
                        GATE_EXPIRED.set()
                        return  # Expired proof gates must never finish successfully.
                    GATE_RELEASED.set()
            final = {"model": "fixture-generation", "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 10, "completion_tokens": 8, "total_tokens": 18}}
            self.wfile.write(f"data: {json.dumps(final)}\n\ndata: [DONE]\n\n".encode())
            self.wfile.flush()
            return
        else:
            promote = "Evaluate whether" in request["messages"][0]["content"]
            article = {"slug": "orchard-answer" if promote else "orchard-knowledge", "title": "Orchard answer" if promote else "Orchard knowledge", "summary": "Cultivation from source evidence", "tags": ["botany"], "aliases": [], "content": "Orchards grow fruit. 學而時習之。"}
            content = {"promote": True, "reason": "Reusable fixture knowledge", "article": article} if promote else {"articles": [article]}
            body = {"model": "fixture-generation", "choices": [{"message": {"content": json.dumps(content, ensure_ascii=False)}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 10, "completion_tokens": 8, "total_tokens": 18}}
        encoded = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)


class LineReader:
    """Bounded select-based line reader for child stdout (protocol-only JSON)."""

    def __init__(self, fd):
        self.fd = fd
        self.buffer = b""

    def read_line(self, timeout):
        deadline = time.monotonic() + timeout
        while b"\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("child stdout line timeout")
            ready, _, _ = select.select([self.fd], [], [], remaining)
            if not ready:
                raise TimeoutError("child stdout line timeout")
            chunk = os.read(self.fd, 65536)
            if not chunk:
                raise EOFError("child stdout closed early")
            self.buffer += chunk
        line, self.buffer = self.buffer.split(b"\n", 1)
        return line

    def drain(self):
        while True:
            chunk = os.read(self.fd, 65536)
            if not chunk:
                break
            self.buffer += chunk
        return self.buffer


def read_line_timeout(fd, timeout):
    deadline = time.monotonic() + timeout
    data = b""
    while not data.endswith(b"\n"):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("timed out waiting for child stderr banner")
        ready, _, _ = select.select([fd], [], [], remaining)
        if not ready:
            raise TimeoutError("timed out waiting for child stderr banner")
        chunk = os.read(fd, 4096)
        if not chunk:
            break
        data += chunk
    return data.decode(errors="replace")


def http_json(port, path, payload, headers=None, timeout=20):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    connection.request("POST", path, body=json.dumps(payload, ensure_ascii=False).encode(),
                       headers={"Content-Type": "application/json", **(headers or {})})
    response = connection.getresponse()
    raw = response.read()
    status = response.status
    connection.close()
    return status, json.loads(raw) if raw else {}


def next_sse_frame(response, buffer, deadline=None):
    while b"\n\n" not in buffer:
        assert deadline is None or time.monotonic() < deadline, "SSE absolute deadline exceeded"
        chunk = response.read1(65536)
        if not chunk:
            return None, buffer
        buffer += chunk
    frame, buffer = buffer.split(b"\n\n", 1)
    name = None
    data_lines = []
    for line in frame.decode(errors="replace").splitlines():
        if line.startswith("event:"):
            name = line[len("event:"):].strip()
        elif line.startswith("data:"):
            data_lines.append(line[len("data:"):].lstrip())
    if not data_lines:
        return ("comment", None), buffer
    return (name or "message", json.loads("\n".join(data_lines))), buffer


def stop_process(process, sig=signal.SIGINT, wait=10):
    signals = []
    if process.poll() is None:
        process.send_signal(sig)
        signals.append(sig.name)
        try:
            process.wait(timeout=wait)
        except subprocess.TimeoutExpired:
            process.terminate()
            signals.append("SIGTERM")
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                signals.append("SIGKILL")
                process.wait(timeout=5)
    return {"exit": process.returncode, "signals": signals,
            "forced": "SIGTERM" in signals or "SIGKILL" in signals}


def http_transport(prefix, base, other, env, run, library, cli_evidence):
    """Real release-binary HTTP startup: authenticated reads, a probe write /
    search / resolve / delete lifecycle, scope rejections and one gated SSE ask."""
    result = {"command": prefix + ["--base-dir", base, "--library-id", "standalone",
                                   "--library", f"other={other}", "serve", "--host", "127.0.0.1",
                                   "--port", "0", "--public-read"],
              "stdout_log": "http-stdout.log", "stderr_log": "http-stderr.log",
              "owner_credential": "synthetic LLMBASE_API_SECRET in child env only; value redacted"}
    steps = []
    secret = f"b9-synthetic-owner-{uuid.uuid4().hex[:16]}"  # child env only; never logged
    child_env = dict(env, LLMBASE_API_SECRET=secret)
    stdout_log = open(run / "http-stdout.log", "wb")
    stderr_log = open(run / "http-stderr.log", "wb")
    drainer = None
    process = subprocess.Popen(result["command"], env=child_env, stdout=stdout_log, stderr=subprocess.PIPE)
    result["steps"] = steps
    owner = {"Authorization": f"Bearer {secret}"}
    try:
        banner = read_line_timeout(process.stderr.fileno(), 20)
        match = re.search(r"listening on 127\.0\.0\.1:(\d+)", banner)
        assert match, f"no listening banner in {banner!r}"
        port = int(match.group(1))
        result["port"] = port
        stderr_log.write(banner.encode())

        def drain_stderr():
            while True:
                chunk = os.read(process.stderr.fileno(), 65536)
                if not chunk:
                    break
                stderr_log.write(chunk)
                stderr_log.flush()

        drainer = threading.Thread(target=drain_stderr, daemon=True)
        drainer.start()

        def call(operation, args, headers, expect=200, note="", library_id="standalone"):
            started = time.monotonic()
            payload = {"library_id": library_id, "operation": operation, "args": args}
            status, value = http_json(port, "/api/v1/op",
                                      payload, headers)
            steps.append({"request": f"POST /api/v1/op {operation} {note}".strip(), "status": status,
                          "payload": payload, "response": value,
                          "wall_ms": round((time.monotonic() - started) * 1000, 3)})
            assert status == expect, f"{operation} {note}: expected {expect}, got {status}: {json.dumps(value, ensure_ascii=False)[:400]}"
            if expect == 200:
                assert value["schema_version"] == 1 and isinstance(value["request_id"], str)
            return value

        # Owner authenticated read: same lexical evidence as the CLI lifecycle.
        envelope = call("search", {"query": "orchard", "kind": "raw"}, owner, note="owner read")
        http_evidence = envelope["result"]["results"][0]["evidence"]
        assert http_evidence == cli_evidence, "HTTP/CLI evidence identity mismatch"
        resolved = call("evidence", {"evidence": http_evidence}, owner, note="owner evidence resolve")
        assert resolved["result"]["stale"] is False
        # Rejections: visitor cookie grants no authority; owner is default-library scoped.
        probe_path = "transport-probe.md"
        probe_token = "b9probe" + uuid.uuid4().hex[:16]
        probe_content = f"---\ntitle: Transport probe\n---\nB9 transport probe {probe_token}.\n"
        call("source_write", {"kind": "raw", "path": probe_path, "content": probe_content},
             {"Cookie": "llmbase_auth=visitor"}, expect=403, note="visitor-cookie write rejected")
        assert not (library / "raw" / probe_path).exists(), "rejected visitor write created a file"
        call("list", {}, owner, expect=403, note="owner targeting configured other library rejected", library_id="other")
        # Owner mutation and resulting retrieval, then deletion by evidence identity.
        call("source_write", {"kind": "raw", "path": probe_path, "content": probe_content}, owner, note="owner probe write")
        assert (library / "raw" / probe_path).read_text() == probe_content
        call("index", {}, owner, note="owner index")
        found = call("search", {"query": probe_token, "kind": "raw"}, owner, note="owner probe search")
        probe_evidence = found["result"]["results"][0]["evidence"]
        resolved = call("evidence", {"evidence": probe_evidence}, owner, note="owner probe resolve")
        assert resolved["result"]["stale"] is False
        call("source_delete", {"source_id": probe_evidence["source_id"], "expected_hash": probe_evidence["content_hash"]},
             owner, note="owner probe delete")
        gone = call("search", {"query": probe_token, "kind": "raw"}, owner, note="owner probe re-search")
        assert not gone["result"]["results"], "deleted probe still searchable"
        # One real SSE ask: a delta must arrive while the provider fixture is
        # still gated before completion, then evidence/done after release.
        request_id = "b9sse" + uuid.uuid4().hex[:12]
        events = []
        saw_delta = False
        for event in (DELTA_GATE, GATE_ENTERED, GATE_EXPIRED, GATE_RELEASED):
            event.clear()
        GATE_ARMED.set()
        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
        started = time.monotonic()
        deadline = started + 45
        payload = {"question": "What grows in an orchard?", "library_id": "standalone",
                   "request_id": request_id, "short_answer": True, "top_k": 1}
        try:
            connection.request("POST", "/api/v1/ask/stream",
                               body=json.dumps(payload).encode(),
                               headers={"Content-Type": "application/json", "Accept": "text/event-stream", **owner})
            response = connection.getresponse()
            assert response.status == 200, f"SSE status {response.status}"
            buffer = b""
            while True:
                assert time.monotonic() < deadline, "SSE absolute deadline exceeded"
                frame, buffer = next_sse_frame(response, buffer, deadline)
                if frame is None:
                    break
                name, data = frame
                if data is None:
                    continue
                events.append({"event": name, "sequence": data["sequence"],
                               "type": data["type"], "data": data})
                assert data["version"] == 1 and data["request_id"] == request_id
                assert name != "error", f"SSE error: {data}"
                if name == "delta" and not saw_delta:
                    assert GATE_ENTERED.is_set() and not GATE_EXPIRED.is_set() and not GATE_RELEASED.is_set(), "delta did not arrive while provider gate was held"
                    saw_delta = True
                    steps.append({"request": "POST /api/v1/ask/stream", "status": 200,
                                  "payload": payload, "gate_held_at_first_delta": True,
                                  "wall_ms": round((time.monotonic() - started) * 1000, 3),
                                  "note": "first delta forwarded to controller before provider gate release"})
                    DELTA_GATE.set()
                if name == "done":
                    break
        finally:
            DELTA_GATE.set()
            GATE_ARMED.clear()
            connection.close()
        assert saw_delta, "no delta forwarded before provider completion"
        assert not GATE_EXPIRED.is_set(), "provider gate expired"
        assert GATE_RELEASED.is_set(), "provider never resumed after the forwarded delta"
        assert {"evidence", "usage", "done"} <= {event["event"] for event in events}
        assert events and events[-1]["event"] == "done", f"no done event: {events}"
        sequences = [event["sequence"] for event in events]
        assert sequences == sorted(sequences) and len(set(sequences)) == len(sequences), "non-monotonic sequences"
        result.update({"steps": steps, "sse_events": events, "sse_request_id": request_id})
        return result
    finally:
        shutdown = stop_process(process, signal.SIGINT)
        result["exit"] = shutdown["exit"]
        result["shutdown"] = shutdown
        if drainer is not None:
            drainer.join(timeout=5)
            assert not drainer.is_alive(), "HTTP stderr drainer did not terminate"
        else:
            stderr_log.write(process.stderr.read())
        process.stderr.close()
        stderr_log.close()
        stdout_log.close()
        (run / "http-transport.json").write_text(json.dumps(result, ensure_ascii=False, indent=2))
        assert shutdown["exit"] == 0 and not shutdown["forced"], f"HTTP shutdown was not clean: {shutdown}"


def mcp_transport(prefix, base, env, run, original):
    """Real release-binary MCP stdio: initialization, discovery, tool calls, a
    protocol error and clean EOF exit; stdout stays protocol-only JSON."""
    command = prefix + ["--base-dir", base, "--library-id", "standalone", "mcp"]
    result = {"command": command, "stderr_log": "mcp-stderr.log", "stdout": "protocol-only JSON lines"}
    steps = []
    result["steps"] = steps
    stderr_log = open(run / "mcp-stderr.log", "wb")
    process = subprocess.Popen(command, env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=stderr_log)
    reader = LineReader(process.stdout.fileno())
    succeeded = False
    try:
        def exchange(message, note):
            process.stdin.write(json.dumps(message, ensure_ascii=False).encode() + b"\n")
            process.stdin.flush()
            started = time.monotonic()
            value = json.loads(reader.read_line(20))  # stdout must be valid protocol JSON only
            steps.append({"request": note, "id": value.get("id"),
                          "payload": message, "response": value,
                          "wall_ms": round((time.monotonic() - started) * 1000, 3)})
            if "id" in message:
                assert value.get("id") == message["id"], f"id mismatch on {note}"
            assert value.get("jsonrpc") == "2.0"
            return value

        init = exchange({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                         "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                                    "clientInfo": {"name": "b9-transport-probe", "version": "1"}}}, "initialize")
        assert init["result"]["serverInfo"]["name"] == "llmbase"
        assert init["result"]["protocolVersion"] == "2025-03-26"
        assert isinstance(init["result"]["capabilities"]["tools"], dict)
        process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        process.stdin.flush()
        listed = exchange({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}, "tools/list")
        tools = listed["result"]["tools"]
        names = {tool["name"] for tool in tools}
        assert {"kb_search_raw", "resolve_evidence"} <= names
        kb_search = next(tool for tool in tools if tool["name"] == "kb_search")
        assert kb_search["inputSchema"]["required"] == ["query"]
        index = exchange({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                          "params": {"name": "kb_rebuild_index", "arguments": {}}}, "tools/call kb_rebuild_index")
        assert index["result"].get("isError") is not True
        rebuilt = json.loads(index["result"]["content"][0]["text"])
        assert rebuilt["article_count"] == 1, rebuilt
        searched = exchange({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                             "params": {"name": "kb_search_raw", "arguments": {"query": "學而時習之"}}},
                            "tools/call kb_search_raw")
        assert searched["result"].get("isError") is not True
        search_result = json.loads(searched["result"]["content"][0]["text"])
        evidence = search_result["results"][0]["evidence"]
        assert evidence["library_id"] == "standalone"
        assert evidence["offset_unit"] == "utf8_byte"
        raw = original.encode()
        assert raw[evidence["start"]:evidence["end"]] == evidence["exact_quote"].encode()
        resolved = exchange({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                             "params": {"name": "resolve_evidence", "arguments": {"evidence": evidence}}},
                            "tools/call resolve_evidence")
        assert resolved["result"].get("isError") is not True
        resolution = json.loads(resolved["result"]["content"][0]["text"])
        assert resolution["stale"] is False and resolution["evidence"] == evidence
        unknown = exchange({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
                            "params": {"name": "unknown_tool", "arguments": {}}}, "tools/call unknown_tool")
        assert isinstance(unknown.get("error"), dict), "unknown tool must be a protocol error"
        succeeded = True
    finally:
        signals = []
        try:
            process.stdin.close()
        except (BrokenPipeError, OSError):
            pass
        try:
            exit_code = process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
            signals.append("SIGTERM")
            try:
                exit_code = process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                signals.append("SIGKILL")
                exit_code = process.wait(timeout=5)
        stderr_log.close()
        if not succeeded:
            process.stdout.close()
        result["exit"] = exit_code
        result["shutdown"] = {"clean_eof": not signals, "signals": signals}
        (run / "mcp-transport.json").write_text(json.dumps(result, ensure_ascii=False, indent=2))
    result["exit"] = exit_code
    if succeeded:
        assert exit_code == 0, f"MCP exit {exit_code} after stdin EOF"
        assert not signals, "MCP needed forced cleanup after stdin EOF"
        remainder = reader.drain()
        assert not remainder.strip(), f"non-protocol stdout bytes after last response: {remainder!r}"
    process.stdout.close()
    result["steps"] = steps
    return result


def transport_proof(prefix, base, chroot, env, run, runtime, library, cli_evidence, original):
    other = "/other" if chroot else str(runtime / "other")
    (runtime / "other").mkdir()
    return {"binary": "same pinned release binary and chroot prefix as the CLI lifecycle",
            "http": http_transport(prefix, base, other, env, run, library, cli_evidence),
            "mcp": mcp_transport(prefix, base, env, run, original)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--run-root", type=Path, required=True)
    parser.add_argument("--chroot", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    run = args.run_root.absolute()
    if run.exists():
        raise SystemExit("run-root must be a new isolated directory")
    run.mkdir(parents=True)
    runtime = run / "runtime"
    library = runtime / "library"
    library.mkdir(parents=True)
    prefix = [str(binary)]
    base = str(library)
    if args.chroot:
        if os.geteuid() != 0 or not shutil.which("chroot"):
            raise SystemExit("--chroot requires the isolated Linux root runner")
        (runtime / "bin").mkdir()
        shutil.copy2(binary, runtime / "bin/llmbase")
        dependencies = subprocess.check_output(["ldd", str(binary)], text=True)
        (run / "ldd.txt").write_text(dependencies)
        for name in sorted(set(re.findall(r"(/[^\s()]+)", dependencies))):
            source = Path(name)
            destination = runtime / source.relative_to("/")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source.resolve(), destination)
        prefix = [shutil.which("chroot"), str(runtime), "/bin/llmbase"]
        base = "/library"
        assert not any(path.name.startswith(("python", "pypy")) for path in runtime.rglob("*"))
    server = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    endpoint = f"http://127.0.0.1:{server.server_port}/v1"
    env = {"PATH": "/no-interpreters", "LLMBASE_BASE_URL": endpoint,
           "LLMBASE_MODEL": "fixture-generation", "LLMBASE_EMBEDDING_BASE_URL": endpoint,
           "LLMBASE_EMBEDDING_MODEL": "fixture-embedding", "LLMBASE_PRIMARY_RETRIES": "1"}
    steps = []

    def invoke(*command, stream=False):
        started = time.monotonic()
        result = subprocess.run(prefix + ["--base-dir", base, "--library-id", "standalone", "--json", *command],
                                env=env, capture_output=True, text=True, timeout=45)
        steps.append({"command": list(command[:2]), "exit": result.returncode,
                      "argv": prefix + ["--base-dir", base, "--library-id", "standalone", "--json", *command],
                      "wall_ms": round((time.monotonic() - started) * 1000, 3)})
        if result.returncode:
            raise RuntimeError(f"{command[:2]}: {result.stderr or result.stdout}")
        value = [json.loads(line) for line in result.stdout.splitlines()] if stream else json.loads(result.stdout)
        steps[-1]["result"] = value
        return value

    def op(name, **payload):
        return invoke("op", name, "--args", json.dumps(payload, ensure_ascii=False))

    try:
        original = "---\ntitle: Orchard source\nunknown: {retained: 𠮷}\nsource_url: https://example.invalid/orchard\n---\nAn orchard grows fruit. 學而時習之。\n"
        op("source_write", kind="raw", path="orchard.md", content=original)
        report = invoke("compile", "source", "orchard")
        assert report["status"] == "succeeded"
        assert not (library / "wiki/concepts/orchard-knowledge.md").exists()
        draft = invoke("compile", "draft", report["draft_ids"][0])
        invoke("compile", "apply", draft["id"])
        assert (library / "raw/orchard.md").read_bytes() == original.encode()
        assert invoke("index")["passages_written"] == 0
        queued = op("job_enqueue", operation="index", args={})
        finished = op("job_run_one", worker_id="standalone-fixture")
        assert finished["id"] == queued["id"] and finished["status"] == "succeeded"
        assert op("job_get", job_id=queued["id"])["attempts"] == 1
        lexical = op("search", query="orchard", kind="raw")
        evidence = lexical["results"][0]["evidence"]
        assert not op("evidence", evidence=evidence)["stale"]
        transports = transport_proof(prefix, base, args.chroot, env, run, runtime, library, evidence, original)
        vectors = op("vector_build", dimension=3, revision="fixture-v1")
        assert vectors["embedded"] > 0
        assert op("vector_build", dimension=3, revision="fixture-v1")["reused_active"]
        assert op("hybrid_search", query="orchard", kind="raw")["mode"] == "hybrid"
        events = invoke("ask", "What grows in an orchard?", "--top-k", "1", "--stream", stream=True)
        assert any(event.get("type") == "delta" for event in events)
        assert events[-1]["type"] == "done"
        answer = invoke("ask", "What grows in an orchard?", "--top-k", "1")
        assert answer["used_evidence"]
        saved = op("answer_save", request={"library_id": "standalone", "question": "What grows in an orchard?", "title": "Orchard answer", "visibility": "public"}, response=answer)
        promotion = op("answer_promote", answer_id=saved["id"])
        assert promotion["promote"] and promotion["draft_id"]
        invoke("compile", "apply", promotion["draft_id"])
        exported = op("export_article", slug="orchard-knowledge")
        assert exported["slug"] == "orchard-knowledge"
        projection = op("rebuild_metadata")
        assert projection["article_count"] == 2
        op("source_write", kind="raw", path="orchard.md", content=original + "Accepted new source revision.\n", expected_hash=evidence["content_hash"])
        assert op("evidence", evidence=evidence)["stale"]
        current = op("get", source_id=evidence["source_id"])
        op("source_delete", source_id=current["source_id"], expected_hash=current["content_hash"])
        assert not op("search", query="orchard", kind="raw")["results"]
        assert op("evidence", evidence=evidence)["deleted"]
        snapshot = "/snapshot" if args.chroot else str(runtime / "snapshot")
        restored = "/restored" if args.chroot else str(runtime / "restored")
        invoke("migration", "backup", snapshot)
        invoke("migration", "verify", snapshot)
        invoke("migration", "restore", snapshot, restored)
        invoke("migration", "verify", restored)
        invoke("migration", "rollback", snapshot)
        receipt = {"passed": True, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                   "isolated_runtime_without_python": args.chroot, "steps": steps,
                   "transports": transports,
                   "provider_fixture_requests": REQUESTS, "semantic_quality": "not evaluated",
                   "provider": "offline deterministic HTTP fixture; no credentials",
                   "runtime_files": sorted(str(path.relative_to(runtime)) for path in runtime.rglob("*") if path.is_file())}
        (run / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2))
        print(json.dumps({"passed": True, "steps": len(steps), "receipt": str(run / "receipt.json")}, indent=2))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


if __name__ == "__main__":
    main()
