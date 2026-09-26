use std::{sync::Arc, time::Duration};

use llmbase_model::{
    CancellationToken, ChatRequest, Client, EmbeddingRequest, ModelConfig, ModelError, SecretKey,
    StreamEvent,
};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Notify, mpsc},
    task::JoinHandle,
};

const SUCCESS: &str = r#"{"model":"fixture-generation","choices":[{"index":0,"message":{"content":"答𠮷"},"finish_reason":"stop"}],"usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6,"completion_tokens_details":{"reasoning_tokens":1}}}"#;

#[tokio::test]
async fn scoped_model_and_key_override_preserve_endpoint_and_isolate_operator_fallbacks() {
    let mut fixture = Fixture::start(vec![
        Reply::json(200, SUCCESS),
        Reply::json(503, "busy"),
        Reply::json(200, SUCCESS),
    ])
    .await;
    let mut config = ModelConfig::new(&fixture.url, "operator-model");
    config.api_key = Some(SecretKey::new("operator-secret"));
    config.fallback_models = vec!["operator-fallback".into()];
    config.retry.max_attempts = 1;
    let original = Client::new(config).unwrap();
    let scoped = original
        .scoped_overrides(
            Some("scoped-model".into()),
            Some(SecretKey::new("scoped-secret")),
        )
        .unwrap();
    assert_eq!(scoped.endpoint_identity(), original.endpoint_identity());
    assert_eq!(original.model(), "operator-model");
    assert_eq!(scoped.model(), "scoped-model");
    assert!(!format!("{scoped:?}").contains("scoped-secret"));
    scoped
        .complete(ChatRequest::new("first"), CancellationToken::new())
        .await
        .unwrap();
    let first = fixture.requests.recv().await.unwrap();
    assert_eq!(first.authorization.as_deref(), Some("Bearer scoped-secret"));
    assert_eq!(first.body["model"], "scoped-model");
    assert!(
        scoped
            .complete(ChatRequest::new("failure"), CancellationToken::new())
            .await
            .is_err()
    );
    let failed = fixture.requests.recv().await.unwrap();
    assert_eq!(failed.body["model"], "scoped-model");
    original
        .complete(ChatRequest::new("original"), CancellationToken::new())
        .await
        .unwrap();
    let last = fixture.requests.recv().await.unwrap();
    assert_eq!(
        last.authorization.as_deref(),
        Some("Bearer operator-secret")
    );
    assert_eq!(last.body["model"], "operator-model");
}

struct Reply {
    status: u16,
    content_type: &'static str,
    headers: String,
    chunks: Vec<Vec<u8>>,
    delay: Duration,
    after_first_chunk: Option<Arc<Notify>>,
}

impl Reply {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "application/json",
            headers: String::new(),
            chunks: vec![body.as_bytes().to_vec()],
            delay: Duration::ZERO,
            after_first_chunk: None,
        }
    }

    fn sse(body: &str) -> Self {
        Self {
            content_type: "text/event-stream; charset=utf-8",
            ..Self::json(200, body)
        }
    }
}

#[derive(Debug)]
struct Received {
    path: String,
    authorization: Option<String>,
    body: Value,
}

struct Fixture {
    url: String,
    requests: mpsc::UnboundedReceiver<Received>,
    task: JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    async fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let (sender, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let header_end;
                loop {
                    let mut buf = [0; 1024];
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                    assert!(request.len() <= 65536, "fixture request too large");
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = end + 4;
                        break;
                    }
                }
                let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                while request.len() < header_end + content_length {
                    let mut buf = [0; 1024];
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                }
                let authorization = headers.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_owned())
                });
                sender
                    .send(Received {
                        path: headers
                            .lines()
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .nth(1)
                            .unwrap()
                            .to_owned(),
                        authorization,
                        body: serde_json::from_slice(
                            &request[header_end..header_end + content_length],
                        )
                        .unwrap(),
                    })
                    .unwrap();
                tokio::time::sleep(reply.delay).await;
                let size: usize = reply.chunks.iter().map(Vec::len).sum();
                let head = format!(
                    "HTTP/1.1 {} Fixture\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
                    reply.status, reply.content_type, size, reply.headers
                );
                if socket.write_all(head.as_bytes()).await.is_err() {
                    continue;
                }
                for (index, chunk) in reply.chunks.iter().enumerate() {
                    if socket.write_all(chunk).await.is_err() {
                        break;
                    }
                    if index == 0
                        && let Some(gate) = &reply.after_first_chunk
                    {
                        gate.notified().await;
                    }
                    tokio::task::yield_now().await;
                }
            }
        });
        Self {
            url,
            requests,
            task,
        }
    }

    fn config(&self) -> ModelConfig {
        let mut config = ModelConfig::new(&self.url, "fixture-requested");
        config.request_timeout = Duration::from_secs(2);
        config.retry.initial_backoff = Duration::from_millis(1);
        config.retry.max_backoff = Duration::from_millis(5);
        config
    }

    fn client(&self) -> Client {
        Client::new(self.config()).unwrap()
    }

    async fn request(&mut self) -> Received {
        tokio::time::timeout(Duration::from_secs(1), self.requests.recv())
            .await
            .unwrap()
            .unwrap()
    }
}

fn token() -> CancellationToken {
    CancellationToken::new()
}

#[tokio::test]
async fn complete_sends_real_http_and_preserves_metadata() {
    let mut fixture = Fixture::start(vec![Reply::json(200, SUCCESS)]).await;
    let mut config = fixture.config();
    config.api_key = Some(SecretKey::new("fixture-only-secret"));
    let client = Client::new(config).unwrap();
    let response = client
        .complete(ChatRequest::new("問").with_system("system"), token())
        .await
        .unwrap();
    assert_eq!(response.content, "答𠮷");
    assert_eq!(response.model, "fixture-generation");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert!(!response.truncated);
    assert_eq!(response.usage.reasoning_tokens, Some(1));
    assert_eq!(response.usage.total_tokens, Some(6));
    assert_eq!(response.attempts, 1);
    let request = fixture.request().await;
    assert_eq!(request.path, "/v1/chat/completions");
    assert_eq!(
        request.authorization.as_deref(),
        Some("Bearer fixture-only-secret")
    );
    assert_eq!(request.body["messages"][0]["role"], "system");
    assert_eq!(request.body["stream"], false);
    assert!(!format!("{client:?}").contains("fixture-only-secret"));
}

#[tokio::test]
async fn non_2xx_errors_do_not_echo_provider_body_or_credentials() {
    let fixture = Fixture::start(vec![Reply::json(
        401,
        r#"{"error":{"message":"Incorrect API key fixture-only-secret"}}"#,
    )])
    .await;
    let mut config = fixture.config();
    config.api_key = Some(SecretKey::new("fixture-only-secret"));
    let error = Client::new(config)
        .unwrap()
        .complete(ChatRequest::new("question"), token())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ModelError::Http {
            status: 401,
            attempts: 1
        }
    );
    assert!(!format!("{error} {error:?}").contains("fixture-only-secret"));
}

#[tokio::test]
async fn rate_limit_retries_and_reports_actual_attempts() {
    let mut first = Reply::json(429, "not inspected");
    first.headers = "Retry-After: 0\r\n".to_owned();
    let fixture = Fixture::start(vec![first, Reply::json(200, SUCCESS)]).await;
    let response = fixture
        .client()
        .complete(ChatRequest::new("q"), token())
        .await
        .unwrap();
    assert_eq!(response.attempts, 2);
}

#[tokio::test]
async fn retry_exhaustion_does_not_claim_success() {
    let fixture = Fixture::start((0..3).map(|_| Reply::json(503, "unavailable")).collect()).await;
    assert_eq!(
        fixture
            .client()
            .complete(ChatRequest::new("q"), token())
            .await
            .unwrap_err(),
        ModelError::Http {
            status: 503,
            attempts: 3
        }
    );
}

#[tokio::test]
async fn explicit_fallback_is_used_and_empty_metadata_is_preserved() {
    let empty = r#"{"choices":[{"message":{"content":""},"finish_reason":"length"}]}"#;
    let mut fixture =
        Fixture::start(vec![Reply::json(200, empty), Reply::json(200, SUCCESS)]).await;
    let mut config = fixture.config();
    config.retry.max_attempts = 1;
    config.fallback_models = vec!["fixture-fallback".to_owned()];
    let response = Client::new(config)
        .unwrap()
        .complete(ChatRequest::new("q"), token())
        .await
        .unwrap();
    assert_eq!(response.attempts, 2);
    fixture.request().await;
    assert_eq!(fixture.request().await.body["model"], "fixture-fallback");

    let fixture = Fixture::start((0..3).map(|_| Reply::json(200, empty)).collect()).await;
    let response = fixture
        .client()
        .complete(ChatRequest::new("q"), token())
        .await
        .unwrap();
    assert_eq!(response.content, "");
    assert_eq!(response.attempts, 3);
    assert!(response.truncated);
    assert_eq!(response.usage.total_tokens, None);
}

#[tokio::test]
async fn malformed_json_is_an_error_without_echo() {
    let fixture = Fixture::start(vec![Reply::json(200, "secret broken JSON")]).await;
    let error = fixture
        .client()
        .complete(ChatRequest::new("q"), token())
        .await
        .unwrap_err();
    assert!(matches!(error, ModelError::Protocol(_)));
    assert!(!format!("{error:?}").contains("secret"));
}

#[tokio::test]
async fn deadline_covers_server_wait_and_retry_backoff() {
    let mut reply = Reply::json(200, SUCCESS);
    reply.delay = Duration::from_millis(200);
    let fixture = Fixture::start(vec![reply]).await;
    let mut config = fixture.config();
    config.request_timeout = Duration::from_millis(30);
    assert_eq!(
        Client::new(config)
            .unwrap()
            .complete(ChatRequest::new("q"), token())
            .await
            .unwrap_err(),
        ModelError::Timeout
    );

    let mut reply = Reply::json(429, "rate limit");
    reply.headers = "Retry-After: 100\r\n".to_owned();
    let fixture = Fixture::start(vec![reply]).await;
    let mut config = fixture.config();
    config.request_timeout = Duration::from_millis(30);
    config.retry.max_backoff = Duration::from_secs(100);
    assert_eq!(
        Client::new(config)
            .unwrap()
            .complete(ChatRequest::new("q"), token())
            .await
            .unwrap_err(),
        ModelError::Timeout
    );
}

#[tokio::test]
async fn cancellation_interrupts_inflight_generation() {
    let mut reply = Reply::json(200, SUCCESS);
    reply.delay = Duration::from_secs(1);
    let mut fixture = Fixture::start(vec![reply]).await;
    let client = fixture.client();
    let cancel = token();
    let request_cancel = cancel.clone();
    let task =
        tokio::spawn(async move { client.complete(ChatRequest::new("q"), request_cancel).await });
    fixture.request().await;
    cancel.cancel();
    assert_eq!(task.await.unwrap().unwrap_err(), ModelError::Cancelled);
}

#[tokio::test]
async fn truncation_and_legacy_reasoning_fallback_are_explicit() {
    let fixture = Fixture::start(vec![Reply::json(200, r#"{"choices":[{"message":{"content":"","reasoning_content":"prefix"},"finish_reason":"length"}]}"#)]).await;
    let response = fixture
        .client()
        .complete(ChatRequest::new("q"), token())
        .await
        .unwrap();
    assert_eq!(response.content, "prefix");
    assert!(response.truncated);
    assert_eq!(response.finish_reason.as_deref(), Some("length"));
}

#[tokio::test]
async fn embeddings_are_validated_and_restored_to_input_order() {
    let mut fixture = Fixture::start(vec![Reply::json(200, r#"{"model":"fixture-vector","data":[{"index":1,"embedding":[0,1]},{"index":0,"embedding":[1,0]}],"usage":{"prompt_tokens":8,"total_tokens":8}}"#)]).await;
    let response = fixture
        .client()
        .embed(
            EmbeddingRequest::new(vec!["first".to_owned(), "second".to_owned()]),
            token(),
        )
        .await
        .unwrap();
    assert_eq!(response.embeddings, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    assert_eq!(response.dimensions, 2);
    assert_eq!(response.usage.prompt_tokens, Some(8));
    let request = fixture.request().await;
    assert_eq!(request.path, "/v1/embeddings");
    assert_eq!(request.body["encoding_format"], "float");
}

#[tokio::test]
async fn invalid_embedding_dimensions_and_duplicate_indices_fail() {
    for body in [
        r#"{"data":[{"index":0,"embedding":[1,0]},{"index":0,"embedding":[0,1]}]}"#,
        r#"{"data":[{"index":0,"embedding":[1,0]},{"index":1,"embedding":[0]}]}"#,
    ] {
        let fixture = Fixture::start(vec![Reply::json(200, body)]).await;
        assert!(matches!(
            fixture
                .client()
                .embed(EmbeddingRequest::new(vec!["a".into(), "b".into()]), token())
                .await
                .unwrap_err(),
            ModelError::Protocol(_)
        ));
    }
}

#[tokio::test]
async fn bytewise_sse_preserves_unicode_usage_and_finish_reason() {
    let wire = ": ping\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"文𠮷\"},\"finish_reason\":null}]}\r\n\r\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3,\"total_tokens\":5}}\n\ndata: [DONE]\n\n";
    let mut reply = Reply::sse(wire);
    reply.chunks = wire.bytes().map(|b| vec![b]).collect();
    let mut fixture = Fixture::start(vec![reply]).await;
    let mut stream = fixture
        .client()
        .stream(ChatRequest::new("q"), token())
        .unwrap();
    assert_eq!(
        stream.recv().await.unwrap().unwrap(),
        StreamEvent::Delta {
            content: "文𠮷".to_owned()
        }
    );
    assert!(matches!(
        stream.recv().await.unwrap().unwrap(),
        StreamEvent::Usage { .. }
    ));
    let StreamEvent::Done { response } = stream.recv().await.unwrap().unwrap() else {
        panic!("missing done")
    };
    assert!(response.truncated);
    assert_eq!(response.content, "文𠮷");
    assert_eq!(response.usage.total_tokens, Some(5));
    assert!(stream.recv().await.is_none());
    let request = fixture.request().await;
    assert_eq!(request.body["stream"], true);
    assert_eq!(request.body["stream_options"]["include_usage"], true);
}

fn gated_stream() -> (Reply, Arc<Notify>) {
    let gate = Arc::new(Notify::new());
    let mut reply = Reply::sse("");
    reply.chunks = vec![b"data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n".to_vec(),
        b"data: {\"choices\":[{\"delta\":{\"content\":\"second\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec()];
    reply.after_first_chunk = Some(gate.clone());
    (reply, gate)
}

#[tokio::test]
async fn first_delta_is_delivered_before_provider_can_finish() {
    let (reply, gate) = gated_stream();
    let fixture = Fixture::start(vec![reply]).await;
    let mut stream = fixture
        .client()
        .stream(ChatRequest::new("q"), token())
        .unwrap();
    let first = tokio::time::timeout(Duration::from_millis(500), stream.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        first,
        StreamEvent::Delta {
            content: "first".into()
        }
    );
    gate.notify_one();
    assert_eq!(
        stream.recv().await.unwrap().unwrap(),
        StreamEvent::Delta {
            content: "second".into()
        }
    );
    assert!(matches!(
        stream.recv().await.unwrap().unwrap(),
        StreamEvent::Done { .. }
    ));
}

#[tokio::test]
async fn stream_cancellation_is_terminal_and_does_not_cancel_parent() {
    let (reply, _gate) = gated_stream();
    let fixture = Fixture::start(vec![reply]).await;
    let parent = token();
    let mut stream = fixture
        .client()
        .stream(ChatRequest::new("q"), parent.clone())
        .unwrap();
    assert!(matches!(
        stream.recv().await.unwrap().unwrap(),
        StreamEvent::Delta { .. }
    ));
    stream.cancel();
    assert_eq!(
        stream.recv().await.unwrap().unwrap_err(),
        ModelError::Cancelled
    );
    assert!(stream.recv().await.is_none());
    assert!(!parent.is_cancelled());
}

#[tokio::test]
async fn stream_timeout_and_midstream_errors_never_produce_done_or_retry() {
    let (reply, _gate) = gated_stream();
    let fixture = Fixture::start(vec![reply]).await;
    let mut config = fixture.config();
    config.request_timeout = Duration::from_millis(50);
    let mut stream = Client::new(config)
        .unwrap()
        .stream(ChatRequest::new("q"), token())
        .unwrap();
    assert!(matches!(
        stream.recv().await.unwrap().unwrap(),
        StreamEvent::Delta { .. }
    ));
    assert_eq!(
        stream.recv().await.unwrap().unwrap_err(),
        ModelError::Timeout
    );
    assert!(stream.recv().await.is_none());

    for suffix in [
        "data: broken JSON\n\n",
        "data: {\"error\":{\"message\":\"secret\"}}\n\n",
        "",
    ] {
        let body =
            format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"first\"}}}}]}}\n\n{suffix}");
        let fixture = Fixture::start(vec![Reply::sse(&body)]).await;
        let mut stream = fixture
            .client()
            .stream(ChatRequest::new("q"), token())
            .unwrap();
        assert!(matches!(
            stream.recv().await.unwrap().unwrap(),
            StreamEvent::Delta { .. }
        ));
        let error = stream.recv().await.unwrap().unwrap_err();
        assert!(matches!(
            error,
            ModelError::Protocol(_) | ModelError::StreamInterrupted
        ));
        assert!(!format!("{error:?}").contains("secret"));
        assert!(stream.recv().await.is_none());
    }
}

#[tokio::test]
async fn provider_without_streaming_is_not_faked() {
    let fixture = Fixture::start(vec![Reply::json(200, SUCCESS)]).await;
    let mut stream = fixture
        .client()
        .stream(ChatRequest::new("q"), token())
        .unwrap();
    assert_eq!(
        stream.recv().await.unwrap().unwrap_err(),
        ModelError::StreamingUnsupported
    );
    let mut config = fixture.config();
    config.supports_streaming = false;
    assert!(matches!(
        Client::new(config)
            .unwrap()
            .stream(ChatRequest::new("q"), token()),
        Err(ModelError::StreamingUnsupported)
    ));
}

#[tokio::test]
async fn response_and_event_limits_are_enforced() {
    let fixture = Fixture::start(vec![Reply::json(200, SUCCESS)]).await;
    let mut config = fixture.config();
    config.max_response_bytes = 20;
    assert_eq!(
        Client::new(config)
            .unwrap()
            .complete(ChatRequest::new("q"), token())
            .await
            .unwrap_err(),
        ModelError::ResponseTooLarge
    );

    let fixture =
        Fixture::start(vec![Reply::sse("data: 123456789012345678901234567890\n\n")]).await;
    let mut config = fixture.config();
    config.max_event_bytes = 16;
    let mut stream = Client::new(config)
        .unwrap()
        .stream(ChatRequest::new("q"), token())
        .unwrap();
    assert_eq!(
        stream.recv().await.unwrap().unwrap_err(),
        ModelError::ResponseTooLarge
    );
}

#[tokio::test]
async fn redirect_does_not_forward_authorization() {
    let mut reply = Reply::json(307, "");
    reply.headers = "Location: http://127.0.0.1:1/steal\r\n".into();
    let fixture = Fixture::start(vec![reply]).await;
    assert_eq!(
        fixture
            .client()
            .complete(ChatRequest::new("q"), token())
            .await
            .unwrap_err(),
        ModelError::Http {
            status: 307,
            attempts: 1
        }
    );
}

#[test]
fn rejects_credential_bearing_urls_and_invalid_configuration() {
    for url in [
        "https://user:secret@models.example/v1",
        "https://models.example/v1?key=secret",
        "file:///secret",
    ] {
        let error = Client::new(ModelConfig::new(url, "model")).unwrap_err();
        assert!(!format!("{error:?}").contains("secret"));
    }
    let mut config = ModelConfig::new("https://models.example/v1", "model");
    config.retry.max_attempts = 0;
    assert!(matches!(
        Client::new(config),
        Err(ModelError::InvalidConfig(_))
    ));
}
