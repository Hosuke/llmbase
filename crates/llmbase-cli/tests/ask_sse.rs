//! End-to-end tests use two actual ephemeral HTTP listeners: the native HTTP
//! transport and a deterministic provider. No external provider or credentials.
use axum::{
    Json, Router,
    extract::State,
    response::{Sse, sse::Event},
    routing::post,
};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use llmbase_cli::{
    Gateway,
    http::{AuthGrant, HttpState, Surface, router},
};
use llmbase_core::{
    LibraryConfig,
    application::{Application, Principal, Request},
    ask::AskModels,
};
use llmbase_model::{Client, ModelConfig};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::Notify,
    task::JoinHandle,
};

const TOKEN: &str = "offline-fixture-authorized-token";

fn transcription_request(id: &str) -> Value {
    json!({"version":1,"library_id":"test","transcription_id":id,
        "request_id":id,"session_id":"device-session","input_kind":"synthetic_transcription",
        "text":"學而時習之","answer_preferences":{"short_answer":true,"max_sentences":2}})
}

#[tokio::test]
async fn transcription_http_preparation_is_scoped_and_never_calls_the_provider() {
    let fixture = fixture(ProviderMode::Success, Duration::from_secs(5)).await;
    let client = reqwest::Client::new();
    let endpoint = format!("{}/api/v1/transcriptions/prepare", fixture.server.url);
    let request = transcription_request("prepare-device");
    let response = client.post(&endpoint).json(&request).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["question"]["question"], request["text"]);
    assert_eq!(value["question"]["short_answer"], true);
    assert_eq!(value["transcription"]["ocr_performed"], false);
    assert_eq!(value["transcription"]["write_policy"], "none");
    let response = client
        .post(format!("{}/api/v1/transcriptions/ask", fixture.server.url))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let mut cross = request.clone();
    cross["library_id"] = json!("other");
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(TOKEN)
            .json(&cross)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let mut image = request.clone();
    image["input_kind"] = json!("handwriting");
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(TOKEN)
            .json(&image)
            .send()
            .await
            .unwrap()
            .status(),
        501
    );
    let mut injected = request;
    injected["api_key"] = json!("must-not-reflect-this-value");
    let response = client
        .post(&endpoint)
        .bearer_auth(TOKEN)
        .json(&injected)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert!(
        !response
            .text()
            .await
            .unwrap()
            .contains("must-not-reflect-this-value")
    );
    assert_eq!(fixture.state.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn transcription_http_stream_has_real_early_deltas_citations_and_owned_cancellation() {
    let fixture = fixture(ProviderMode::Success, Duration::from_secs(5)).await;
    let client = reqwest::Client::new();
    let request = transcription_request("stream-device");
    let response = client
        .post(format!(
            "{}/api/v1/transcriptions/ask/stream",
            fixture.server.url
        ))
        .bearer_auth(TOKEN)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut events = until_delta(&mut stream, &mut buffer).await;
    assert_eq!(events[0]["transcription"]["ocr_performed"], false);
    assert!(!fixture.state.sent_done.load(Ordering::SeqCst));
    fixture.state.finish.notify_one();
    loop {
        let event = next_event(&mut stream, &mut buffer).await;
        let done = event["type"] == "done";
        events.push(event);
        if done {
            break;
        }
    }
    for (sequence, event) in events.iter().enumerate() {
        assert_eq!(event["transcription_id"], "stream-device");
        assert_eq!(event["session_id"], "device-session");
        assert_eq!(event["sequence"], sequence as u64);
    }
    let evidence = &events.last().unwrap()["response"]["used_evidence"][0]["evidence"];
    let resolved: Value = client
        .post(format!("{}/api/v1/evidence/resolve", fixture.server.url))
        .json(&json!({"evidence":evidence}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resolved["result"]["stale"], false);
    assert!(!fixture._dir.path().join("wiki/outputs").exists());
    drop(stream);
    let request = transcription_request("cancel-device");
    let response = client
        .post(format!(
            "{}/api/v1/transcriptions/ask/stream",
            fixture.server.url
        ))
        .bearer_auth(TOKEN)
        .json(&request)
        .send()
        .await
        .unwrap();
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    until_delta(&mut stream, &mut buffer).await;
    let endpoint = format!(
        "{}/api/v1/requests/cancel-device/cancel",
        fixture.server.url
    );
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth("another-user-fixture-token")
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let error = next_event(&mut stream, &mut buffer).await;
    assert_eq!(error["code"], "cancelled");
    assert_eq!(error["transcription_id"], "cancel-device");
}

#[derive(Clone, Copy)]
enum ProviderMode {
    Success,
    BadJson,
    Stall,
}

struct ProviderState {
    finish: Notify,
    dropped: AtomicBool,
    did_drop: Notify,
    sent_done: AtomicBool,
    calls: AtomicUsize,
    body: Mutex<Option<Value>>,
    mode: ProviderMode,
}

struct ProviderDrop(Arc<ProviderState>);
impl Drop for ProviderDrop {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
        self.0.did_drop.notify_one();
    }
}

async fn provider(
    State(state): State<Arc<ProviderState>>,
    Json(body): Json<Value>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    state.calls.fetch_add(1, Ordering::SeqCst);
    *state.body.lock().unwrap() = Some(body);
    let guard = ProviderDrop(state.clone());
    Sse::new(futures_util::stream::unfold(
        (state, 0, guard),
        |(state, step, guard)| async move {
            let data=match step {
            0=>json!({"model":"fixture-text","choices":[{"index":0,"delta":{"content":"Study "}}]}).to_string(),
            1=>{
                state.finish.notified().await;
                match state.mode {
                    ProviderMode::Success=>json!({"choices":[{"index":0,"delta":{"content":"the passage [E1]."}}]}).to_string(),
                    ProviderMode::BadJson=>"{broken fixture JSON".into(),
                    ProviderMode::Stall=>{std::future::pending::<()>().await;unreachable!()}
                }
            }
            2=>json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":7,"total_tokens":19}}).to_string(),
            3=>{state.sent_done.store(true,Ordering::SeqCst);"[DONE]".into()},
            _=>return None,
        };
            Some((Ok(Event::default().data(data)), (state, step + 1, guard)))
        },
    ))
}

struct Server {
    url: String,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn listen(app: Router) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Server { url, task }
}

struct Fixture {
    _dir: TempDir,
    _provider: Server,
    server: Server,
    state: Arc<ProviderState>,
}
async fn fixture(mode: ProviderMode, read_timeout: Duration) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("raw")).unwrap();
    std::fs::write(
        dir.path().join("raw/classic.md"),
        "---\ntitle: 經典\n---\n學而時習之，不亦說乎。𠮷。\n",
    )
    .unwrap();
    let app = Application::new(vec![LibraryConfig::load(dir.path(), "test").unwrap()]).unwrap();
    app.execute(
        &Principal::local("test"),
        Request {
            library_id: "test".into(),
            operation: "index".into(),
            args: json!({}),
        },
    )
    .unwrap();
    let state = Arc::new(ProviderState {
        finish: Notify::new(),
        dropped: AtomicBool::new(false),
        did_drop: Notify::new(),
        sent_done: AtomicBool::new(false),
        calls: AtomicUsize::new(0),
        body: Mutex::new(None),
        mode,
    });
    let upstream = listen(
        Router::new()
            .route("/v1/chat/completions", post(provider))
            .with_state(state.clone()),
    )
    .await;
    let mut config = ModelConfig::new(format!("{}/v1", upstream.url), "fixture-text");
    config.read_timeout = read_timeout;
    config.request_timeout = Duration::from_secs(10);
    config.retry.max_attempts = 1;
    let models = AskModels {
        generation: Client::new(config).unwrap(),
        embedding: None,
    };
    let grant = AuthGrant::new(TOKEN, Principal::local("test")).unwrap();
    let mut other = Principal::local("test");
    other.id = "second-user".into();
    let other_grant = AuthGrant::new("another-user-fixture-token", other).unwrap();
    let server = listen(router(
        HttpState::new(
            Gateway::new(app, "test".into()),
            Surface::Web,
            true,
            vec![grant, other_grant],
        )
        .with_models(Some(models)),
    ))
    .await;
    Fixture {
        _dir: dir,
        _provider: upstream,
        server,
        state,
    }
}

async fn next_event<S>(stream: &mut S, buffer: &mut Vec<u8>) -> Value
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    loop {
        if let Some(end) = buffer.windows(2).position(|bytes| bytes == b"\n\n") {
            let frame = String::from_utf8(buffer.drain(..end + 2).collect()).unwrap();
            let data = frame
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("data: ")
                        .or_else(|| line.strip_prefix("data:"))
                })
                .collect::<Vec<_>>()
                .join("\n");
            if data.is_empty() {
                continue;
            }
            return serde_json::from_str(&data).unwrap();
        }
        let chunk = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("native SSE stalled")
            .expect("native SSE closed too early")
            .unwrap();
        buffer.extend_from_slice(&chunk);
    }
}

async fn until_delta<S>(stream: &mut S, buffer: &mut Vec<u8>) -> Vec<Value>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    let mut events = Vec::new();
    loop {
        let event = next_event(stream, buffer).await;
        let delta = event["type"] == "delta";
        events.push(event);
        if delta {
            return events;
        }
    }
}

#[tokio::test]
async fn actual_delta_reaches_http_before_provider_finishes_and_citations_resolve() {
    let fixture = fixture(ProviderMode::Success, Duration::from_secs(5)).await;
    let client = reqwest::Client::new();
    let response=client.post(format!("{}/api/v1/ask/stream",fixture.server.url)).bearer_auth(TOKEN)
        .json(&json!({"question":"學而時習之","library_id":"test","request_id":"first-stream","session_id":"session-a","short_answer":true})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut events = until_delta(&mut stream, &mut buffer).await;
    assert_eq!(events.last().unwrap()["content"], "Study ");
    assert!(
        !fixture.state.sent_done.load(Ordering::SeqCst),
        "first native delta arrived only after full provider answer"
    );
    assert!(events.iter().any(|e| e["type"] == "evidence"));
    fixture.state.finish.notify_one();
    loop {
        let event = next_event(&mut stream, &mut buffer).await;
        let done = event["type"] == "done";
        events.push(event);
        if done {
            break;
        }
    }
    for (sequence, event) in events.iter().enumerate() {
        assert_eq!(event["sequence"], sequence as u64);
        assert_eq!(event["version"], 1);
        assert_eq!(event["request_id"], "first-stream");
        assert_eq!(event["session_id"], "session-a");
    }
    let response = &events.last().unwrap()["response"];
    assert_eq!(response["generation"]["content"], "Study the passage [E1].");
    assert_eq!(response["used_evidence"].as_array().unwrap().len(), 1);
    assert_eq!(response["generation"]["usage"]["total_tokens"], 19);
    let evidence = &response["used_evidence"][0]["evidence"];
    let resolved = client
        .post(format!("{}/api/v1/evidence/resolve", fixture.server.url))
        .json(&json!({"evidence":evidence}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(resolved["result"]["stale"], false);
    assert!(!fixture._dir.path().join("wiki/outputs").exists());
    let body = fixture.state.body.lock().unwrap().clone().unwrap();
    assert_eq!(body["stream"], true);
    assert!(
        body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("English is the default")
    );
}

#[tokio::test]
async fn explicit_cancel_is_owned_scoped_and_finishes_with_partial_error() {
    let fixture = fixture(ProviderMode::Stall, Duration::from_secs(5)).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/ask/stream", fixture.server.url))
        .bearer_auth(TOKEN)
        .json(&json!({"question":"學而時習之","request_id":"cancel-me"}))
        .send()
        .await
        .unwrap();
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    until_delta(&mut stream, &mut buffer).await;
    let endpoint = format!("{}/api/v1/requests/cancel-me/cancel", fixture.server.url);
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth("another-user-fixture-token")
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(TOKEN)
            .header("x-library-id", "other")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let event = next_event(&mut stream, &mut buffer).await;
    assert_eq!(event["type"], "error");
    assert_eq!(event["code"], "cancelled");
    assert_eq!(event["partial"], true);
    assert!(!fixture.state.sent_done.load(Ordering::SeqCst));
}

#[tokio::test]
async fn http_disconnect_cancels_provider_request_and_releases_lease() {
    let fixture = fixture(ProviderMode::Stall, Duration::from_secs(5)).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/ask/stream", fixture.server.url))
        .bearer_auth(TOKEN)
        .json(&json!({"question":"學而時習之","request_id":"drop-me"}))
        .send()
        .await
        .unwrap();
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    until_delta(&mut stream, &mut buffer).await;
    drop(stream);
    tokio::time::timeout(Duration::from_secs(5), fixture.state.did_drop.notified())
        .await
        .expect("provider HTTP body was not cancelled on client disconnect");
    assert!(fixture.state.dropped.load(Ordering::SeqCst));
    assert_eq!(
        client
            .post(format!(
                "{}/api/v1/requests/drop-me/cancel",
                fixture.server.url
            ))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

#[tokio::test]
async fn provider_failure_and_timeout_emit_error_without_fake_done() {
    for (mode, read_timeout, release) in [
        (ProviderMode::BadJson, Duration::from_secs(5), true),
        (ProviderMode::Stall, Duration::from_millis(150), false),
    ] {
        let fixture = fixture(mode, read_timeout).await;
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/api/v1/ask/stream", fixture.server.url))
            .bearer_auth(TOKEN)
            .json(&json!({"question":"學而時習之"}))
            .send()
            .await
            .unwrap();
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        until_delta(&mut stream, &mut buffer).await;
        if release {
            fixture.state.finish.notify_one();
        }
        let event = next_event(&mut stream, &mut buffer).await;
        assert_eq!(event["type"], "error");
        assert_eq!(event["partial"], true);
        // An upstream may eagerly send [DONE] after malformed data. The native
        // stream must reject that data and close after error, never emit done.
        let tail = tokio::time::timeout(Duration::from_secs(5), async {
            let mut tail = buffer;
            while let Some(chunk) = stream.next().await {
                tail.extend_from_slice(&chunk.unwrap());
            }
            tail
        })
        .await
        .expect("failed stream did not close");
        assert!(
            tail.is_empty(),
            "native stream emitted events after error: {}",
            String::from_utf8_lossy(&tail)
        );
        if matches!(mode, ProviderMode::Stall) {
            assert!(!fixture.state.sent_done.load(Ordering::SeqCst));
        }
    }
}

#[tokio::test]
async fn model_access_requires_capability_and_plain_ask_is_read_only() {
    let fixture = fixture(ProviderMode::Success, Duration::from_secs(5)).await;
    let client = reqwest::Client::new();
    let endpoint = format!("{}/api/v1/ask", fixture.server.url);
    let request = json!({"question":"學而時習之","answer_language":"wenyan"});
    assert_eq!(
        client
            .post(&endpoint)
            .header("cookie", "llmbase_auth=visitor")
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(fixture.state.calls.load(Ordering::SeqCst), 0);
    fixture.state.finish.notify_one();
    let response = client
        .post(&endpoint)
        .bearer_auth(TOKEN)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let result = response.json::<Value>().await.unwrap();
    assert_eq!(
        result["result"]["generation"]["content"],
        "Study the passage [E1]."
    );
    assert!(!fixture._dir.path().join("wiki/outputs").exists());
    assert!(
        fixture.state.body.lock().unwrap().as_ref().unwrap()["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("code: wenyan")
    );
}

#[tokio::test]
async fn standalone_query_streams_before_completion_and_partial_failure_exits_nonzero() {
    for mode in [ProviderMode::Success, ProviderMode::BadJson] {
        let fixture = fixture(mode, Duration::from_secs(5)).await;
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_llmbase"))
            .env_clear()
            .env("LLMBASE_BASE_URL", format!("{}/v1", fixture._provider.url))
            .env("LLMBASE_MODEL", "fixture-text")
            .arg("--base-dir")
            .arg(fixture._dir.path())
            .args(["--library-id", "test", "ask", "學而時習之", "--stream"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut events = Vec::new();
        loop {
            let line = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
                .await
                .expect("CLI withheld delta until provider completion")
                .unwrap()
                .unwrap();
            let event: Value = serde_json::from_str(&line).unwrap();
            let is_delta = event["type"] == "delta";
            events.push(event);
            if is_delta {
                break;
            }
        }
        assert!(!fixture.state.sent_done.load(Ordering::SeqCst));
        fixture.state.finish.notify_one();
        while let Some(line) = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .unwrap()
            .unwrap()
        {
            events.push(serde_json::from_str::<Value>(&line).unwrap());
        }
        let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        match mode {
            ProviderMode::Success => {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(events.last().unwrap()["type"], "done");
                assert!(output.stderr.is_empty());
            }
            ProviderMode::BadJson => {
                assert!(!output.status.success());
                assert_eq!(events.last().unwrap()["type"], "error");
                assert_eq!(events.last().unwrap()["partial"], true);
                assert!(!events.iter().any(|event| event["type"] == "done"));
                assert!(
                    serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"].is_object()
                );
            }
            ProviderMode::Stall => unreachable!(),
        }
        assert!(!fixture._dir.path().join("wiki/outputs").exists());
    }
}
