use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use llmbase_cli::{
    Gateway,
    http::{AuthGrant, HttpState, Surface, router},
};
use llmbase_core::{
    LibraryConfig,
    application::{Application, Principal},
    ask::AskModels,
};
use llmbase_model::{Client, ModelConfig};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};
use tower::ServiceExt;

const RAW: &str = "---\ntitle: Orchard source\nunknown: {preserved: true}\n---\nOrchards grow fruit. 學而時習之。";
fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("raw")).unwrap();
    std::fs::write(dir.path().join("raw/original.md"), RAW).unwrap();
    dir
}

struct Provider {
    url: String,
    calls: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Provider {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let end = loop {
                    let mut buffer = [0; 4096];
                    let n = socket.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        break pos + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let size: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                while bytes.len() < end + size {
                    let mut buffer = [0; 4096];
                    let n = socket.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let request: Value = serde_json::from_slice(&bytes[end..end + size]).unwrap();
                assert_eq!(request["model"], "fixture");
                count.fetch_add(1, Ordering::SeqCst);
                let article=json!({"articles":[{"slug":"orchard","title":"Orchard","summary":"Fruit cultivation","tags":[],"aliases":[],"content":"Orchards grow fruit. 學而時習之。"}]}).to_string();
                let body=json!({"model":"fixture","choices":[{"message":{"content":article},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}}).to_string();
                let wire = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(wire.as_bytes()).await;
            }
        });
        Self { url, calls, task }
    }
    fn command(&self, dir: &TempDir) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_llmbase"));
        command
            .env_clear()
            .env("LLMBASE_BASE_URL", &self.url)
            .env("LLMBASE_MODEL", "fixture")
            .arg("--base-dir")
            .arg(dir.path())
            .args(["--library-id", "test", "--json"]);
        command
    }
}

#[tokio::test]
async fn actual_cli_creates_reviewable_draft_then_applies_exact_bytes_without_python() {
    let dir = fixture();
    let provider = Provider::new().await;
    let output = provider
        .command(&dir)
        .args(["compile", "source", "original"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "succeeded");
    let id = report["draft_ids"][0].as_str().unwrap();
    assert!(!dir.path().join("wiki/concepts/orchard.md").exists());
    let output = provider
        .command(&dir)
        .args(["compile", "draft", id])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let draft: Value = serde_json::from_slice(&output.stdout).unwrap();
    let output = provider
        .command(&dir)
        .args(["compile", "apply", id])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let accepted: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(accepted["status"], "applied");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("wiki/concepts/orchard.md")).unwrap(),
        draft["content"]
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("raw/original.md")).unwrap(),
        RAW
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn http_uses_configured_model_and_denies_public_compile_before_provider_call() {
    let dir = fixture();
    let provider = Provider::new().await;
    let app = Application::new(vec![LibraryConfig::load(dir.path(), "test").unwrap()]).unwrap();
    let models = AskModels {
        generation: Client::new(ModelConfig::new(&provider.url, "fixture")).unwrap(),
        embedding: None,
    };
    let token = "test-only-compile-bearer";
    let app = router(
        HttpState::new(
            Gateway::new(app, "test".into()),
            Surface::Agent,
            true,
            vec![AuthGrant::new(token, Principal::local("test")).unwrap()],
        )
        .with_models(Some(models)),
    );
    for (authorized, expected) in [(false, 403), (true, 200)] {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/v1/op")
            .header("content-type", "application/json");
        if authorized {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response=app.clone().oneshot(request.body(Body::from(json!({"library_id":"test","operation":"compile","args":{"source_id":"original"}}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), expected);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        if authorized {
            let result: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(result["result"]["status"], "succeeded");
        } else {
            assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        }
    }
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(!dir.path().join("wiki/concepts/orchard.md").exists());
}

#[tokio::test]
async fn mcp_discovers_and_executes_additive_draft_tools_through_application() {
    let dir = fixture();
    let provider = Provider::new().await;
    let mut child = provider
        .command(&dir)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"compile-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"compile_source","arguments":{"source_id":"original"}}}),
    ];
    for (index, request) in requests.into_iter().enumerate() {
        input
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        let line = tokio::time::timeout(Duration::from_secs(10), output.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert!(value.get("error").is_none(), "{value}");
        if index == 0 {
            input
                .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
                .await
                .unwrap();
        }
        if index == 1 {
            assert!(
                value["result"]["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == "compile_source")
            );
        }
        if index == 2 {
            assert_ne!(value["result"]["isError"], true);
            let report: Value =
                serde_json::from_str(value["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(report["status"], "succeeded");
            assert_eq!(report["draft_ids"].as_array().unwrap().len(), 1);
        }
    }
    drop(input);
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("raw/original.md")).unwrap(),
        RAW
    );
}
