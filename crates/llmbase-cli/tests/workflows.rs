use axum::{Json, Router, extract::State, routing::post};
use llmbase_cli::Gateway;
use llmbase_core::{
    LibraryConfig,
    application::{Application, Capability, Principal, Request},
    ask::AskModels,
};
use llmbase_model::{Client, ModelConfig};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    process::Command,
    sync::{Arc, Mutex},
};
use tempfile::TempDir;

#[derive(Clone)]
struct Responses {
    pending: Arc<Mutex<VecDeque<String>>>,
    requests: Arc<Mutex<Vec<Value>>>,
}
async fn complete(State(state): State<Responses>, Json(request): Json<Value>) -> Json<Value> {
    state.requests.lock().unwrap().push(request);
    let content = state
        .pending
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected model call");
    Json(
        json!({"choices":[{"message":{"content":content},"finish_reason":"stop"}],"usage":{"total_tokens":11}}),
    )
}
struct Provider {
    endpoint: String,
    client: Client,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Provider {
    async fn new(replies: &[&str]) -> Self {
        let state = Responses {
            pending: Arc::new(Mutex::new(replies.iter().map(|s| s.to_string()).collect())),
            requests: Default::default(),
        };
        let requests = state.requests.clone();
        let router = Router::new()
            .route("/v1/chat/completions", post(complete))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let mut config = ModelConfig::new(&endpoint, "transport-fixture");
        config.retry.max_attempts = 1;
        let client = Client::new(config).unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            endpoint,
            client,
            requests,
            task,
        }
    }
}
fn fixture() -> (TempDir, Application) {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    for name in ["Alpha", "Beta", "Gamma"] {
        fs::write(root.path().join(format!("wiki/concepts/{}.md",name.to_lowercase())),format!("---\ntitle: {name}\nsummary: evidence knowledge\n---\n## 文言\n學而時習之，知識可考。\n")).unwrap();
    }
    let app = Application::new(vec![LibraryConfig::load(root.path(), "test").unwrap()]).unwrap();
    app.execute(
        &Principal::local("test"),
        Request {
            library_id: "test".into(),
            operation: "rebuild_metadata".into(),
            args: json!({}),
        },
    )
    .unwrap();
    (root, app)
}
fn model_reader() -> Principal {
    let mut p = Principal::public("test");
    p.capabilities.insert(Capability::Model);
    p
}

#[tokio::test]
async fn legacy_ask_preserves_defaults_and_gates_writes_before_calling_provider() {
    let (root, app) = fixture();
    let provider = Provider::new(&["Alpha\nBeta\nGamma", "Read-only answer", "Filed answer"]).await;
    let gateway = Gateway::new(app, "test".into()).with_models(Some(AskModels {
        generation: provider.client.clone(),
        embedding: None,
    }));
    let denied = gateway
        .legacy(
            model_reader(),
            "test".into(),
            "kb_ask",
            json!({"question":"knowledge","file_back":true}),
        )
        .await;
    assert!(matches!(denied, Err(llmbase_core::Error::Scope)));
    assert!(provider.requests.lock().unwrap().is_empty());
    let answer = gateway
        .legacy(
            model_reader(),
            "test".into(),
            "kb_ask",
            json!({"question":"knowledge"}),
        )
        .await
        .unwrap();
    assert_eq!(answer["answer"], "Read-only answer");
    assert_eq!(answer["consulted"].as_array().unwrap().len(), 3);
    assert!(answer["output_path"].is_null());
    assert!(
        !root.path().join("wiki/outputs").exists()
            || fs::read_dir(root.path().join("wiki/outputs"))
                .unwrap()
                .count()
                == 0
    );
    let filed = gateway
        .legacy(
            Principal::local("test"),
            "test".into(),
            "kb_ask",
            json!({"question":"knowledge","deep":false,"file_back":true}),
        )
        .await
        .unwrap();
    assert_eq!(filed["answer"], "Filed answer");
    assert_eq!(
        fs::read_dir(root.path().join("wiki/outputs"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(provider.requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn legacy_cli_query_uses_old_format_tone_and_explicit_file_back_flags() {
    let (root, _) = fixture();
    let provider = Provider::new(&["文言答覆。", "---\nmarp: true\n---\n# Answer"]).await;
    for (format, tone, write) in [("markdown", "wenyan", false), ("marp", "default", true)] {
        let base = root.path().to_path_buf();
        let endpoint = provider.endpoint.clone();
        let output = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_llmbase"));
            cmd.env_clear()
                .env("LLMBASE_BASE_URL", endpoint)
                .env("LLMBASE_MODEL", "transport-fixture")
                .arg("--base-dir")
                .arg(base)
                .args([
                    "--library-id",
                    "test",
                    "query",
                    "knowledge",
                    "--format",
                    format,
                    "--tone",
                    tone,
                ]);
            if write {
                cmd.arg("--file-back");
            }
            cmd.output().unwrap()
        })
        .await
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            serde_json::from_slice::<Value>(&output.stdout)
                .unwrap()
                .is_string()
        );
    }
    let requests = provider.requests.lock().unwrap();
    assert!(requests[0].to_string().contains("文言"));
    assert!(requests[1].to_string().contains("Marp"));
    assert_eq!(
        fs::read_dir(root.path().join("wiki/outputs"))
            .unwrap()
            .count(),
        1
    );
}

#[tokio::test]
async fn legacy_get_section_uses_scalar_offsets_and_hides_missing_content() {
    let (_root, app) = fixture();
    let gateway = Gateway::new(app, "test".into());
    let sections = gateway
        .legacy(
            Principal::public("test"),
            "test".into(),
            "kb_get_sections",
            json!({"slug":"alpha"}),
        )
        .await
        .unwrap();
    let result = gateway
        .legacy(
            Principal::public("test"),
            "test".into(),
            "kb_get",
            json!({"slug":"alpha","section":sections["sections"][0]["anchor"]}),
        )
        .await
        .unwrap();
    assert_eq!(result["section_found"], true);
    assert!(result["content"].as_str().unwrap().starts_with("## 文言"));
    let missing = gateway
        .legacy(
            Principal::public("test"),
            "test".into(),
            "kb_get",
            json!({"slug":"alpha","section":"absent"}),
        )
        .await
        .unwrap();
    assert_eq!(missing["found"], true);
    assert_eq!(missing["section_found"], false);
    assert!(missing.get("content").is_none());
}

#[tokio::test]
async fn legacy_backlinks_keeps_unknown_targets_without_private_incoming_articles() {
    let (root, app) = fixture();
    fs::write(
        root.path().join("wiki/concepts/link.md"),
        "---\ntitle: Link\n---\n[[missing]] [[Alpha]]",
    )
    .unwrap();
    fs::write(
        root.path().join("wiki/concepts/secret.md"),
        "---\nprivate: true\n---\n[[missing]] [[Alpha]]",
    )
    .unwrap();
    app.execute(
        &Principal::local("test"),
        Request {
            library_id: "test".into(),
            operation: "index".into(),
            args: json!({}),
        },
    )
    .unwrap();
    let gateway = Gateway::new(app, "test".into());
    let missing = gateway
        .legacy(
            Principal::public("test"),
            "test".into(),
            "kb_backlinks",
            json!({"slug":"missing"}),
        )
        .await
        .unwrap();
    assert_eq!(missing, json!({"slug":"missing","cited_by":["link"]}));
    let alias = gateway
        .legacy(
            Principal::public("test"),
            "test".into(),
            "kb_backlinks",
            json!({"slug":"Alpha"}),
        )
        .await
        .unwrap();
    assert_eq!(alias["cited_by"], json!([]));
}

#[tokio::test]
async fn failed_accepted_compile_is_a_failed_durable_job_not_empty_success() {
    let (root, app) = fixture();
    fs::create_dir_all(root.path().join("raw/new")).unwrap();
    fs::write(
        root.path().join("raw/new/index.md"),
        "---\ntitle: New\n---\nOriginal bytes.",
    )
    .unwrap();
    let provider = Provider::new(&["invalid structured extraction"]).await;
    let gateway = Gateway::new(app, "test".into()).with_models(Some(AskModels {
        generation: provider.client.clone(),
        embedding: None,
    }));
    let principal = Principal::local("test");
    let job = gateway
        .execute(
            principal.clone(),
            Request {
                library_id: "test".into(),
                operation: "job_enqueue".into(),
                args: json!({"operation":"legacy_compile","args":{"mode":"new"}}),
            },
        )
        .await
        .unwrap();
    assert!(job.is_object());
    let result = gateway
        .execute(
            principal,
            Request {
                library_id: "test".into(),
                operation: "job_run_one".into(),
                args: json!({}),
            },
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(result["result"]["status"], "failed");
    assert_eq!(
        fs::read_to_string(root.path().join("raw/new/index.md")).unwrap(),
        "---\ntitle: New\n---\nOriginal bytes."
    );
}
