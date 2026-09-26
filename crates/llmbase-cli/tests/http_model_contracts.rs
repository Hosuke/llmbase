use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    routing::post,
};
use http_body_util::BodyExt;
use llmbase_cli::{
    Gateway,
    http::{AuthGrant, HttpState, Surface, router},
};
use llmbase_core::{
    Library, LibraryConfig,
    application::{Application, Principal},
    ask::AskModels,
};
use llmbase_model::{Client, ModelConfig};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;
use tower::ServiceExt;
#[derive(Clone)]
struct Replies {
    pending: Arc<Mutex<VecDeque<String>>>,
    requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
}
async fn complete(
    State(state): State<Replies>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Json<Value> {
    state.requests.lock().unwrap().push((headers, request));
    let text = state
        .pending
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected provider call");
    Json(
        json!({"model":"http-contract-fixture","choices":[{"message":{"content":text},"finish_reason":"stop"}],"usage":{"total_tokens":12}}),
    )
}
struct Provider {
    client: Client,
    requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort()
    }
}
impl Provider {
    async fn new(replies: &[&str]) -> Self {
        let state = Replies {
            pending: Arc::new(Mutex::new(replies.iter().map(|s| s.to_string()).collect())),
            requests: Default::default(),
        };
        let requests = state.requests.clone();
        let app = Router::new()
            .route("/v1/chat/completions", post(complete))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "http-contract-fixture",
        );
        config.retry.max_attempts = 1;
        config.request_timeout = Duration::from_secs(2);
        let client = Client::new(config).unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            client,
            requests,
            task,
        }
    }
}
fn fixture(surface: Surface, provider: &Provider) -> (TempDir, LibraryConfig, Router) {
    let root = TempDir::new().unwrap();
    let mut config = LibraryConfig::load(root.path(), "http-model").unwrap();
    config.settings["entities"] = json!({"enabled":true});
    fs::create_dir_all(&config.concepts).unwrap();
    for name in ["Alpha", "Beta", "Gamma"] {
        fs::write(config.concepts.join(format!("{}.md",name.to_lowercase())),format!("---\ntitle: {name}\nsummary: knowledge evidence\ntags: [{name}]\nunknown: 42\n---\nThis is substantive accepted article content about {name}, with enough detail to preserve it during cleanup. 原典𠮷.\n")).unwrap();
    }
    let mut library = Library::open(config.clone()).unwrap();
    library.rebuild_metadata().unwrap();
    let gateway = Gateway::new(
        Application::new(vec![config.clone()]).unwrap(),
        "http-model".into(),
    );
    let grant =
        AuthGrant::new("fixture-owner-model-token", Principal::local("http-model")).unwrap();
    let app = router(
        HttpState::new(gateway, surface, true, vec![grant]).with_models(Some(AskModels {
            generation: provider.client.clone(),
            embedding: None,
        })),
    );
    (root, config, app)
}
async fn call(
    app: Router,
    method: &str,
    path: &str,
    body: Value,
    owner: bool,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if owner {
        req = req.header("authorization", "Bearer fixture-owner-model-token")
    }
    if let Some(key) = key {
        req = req.header("x-llm-key", key)
    }
    let response = app
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_http_defaults_are_basic_and_file_back_with_distinct_legacy_envelopes() {
    for surface in [Surface::Web, Surface::Agent] {
        let provider = Provider::new(&["Accepted answer grounded in the articles."]).await;
        let (_root, config, app) = fixture(surface, &provider);
        let (status, value) = call(
            app,
            "POST",
            "/api/ask",
            json!({"question":"knowledge evidence"}),
            true,
            Some("ephemeral-fixture-provider-key"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["answer"], "Accepted answer grounded in the articles.");
        assert!(value.get("consulted").is_none());
        if surface == Surface::Web {
            assert!(value.get("status").is_none());
            assert!(value["output_path"].as_str().is_some());
        } else {
            assert_eq!(value["status"], "ok");
            assert!(value.get("output_path").is_none());
        }
        assert!(fs::read_dir(&config.outputs).unwrap().next().is_some());
        let requests = provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].0["authorization"],
            "Bearer ephemeral-fixture-provider-key"
        );
        drop(requests);
        let db = fs::read(config.database()).unwrap();
        assert!(!String::from_utf8_lossy(&db).contains("ephemeral-fixture-provider-key"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deep_http_returns_real_consulted_sources_and_keeps_read_only_opt_out() {
    let provider = Provider::new(&["Alpha\nBeta\nGamma", "Research answer."]).await;
    let (_root, config, app) = fixture(Surface::Web, &provider);
    let (status, value) = call(
        app,
        "POST",
        "/api/ask",
        json!({"question":"knowledge evidence","deep":true,"file_back":false,"tone":"wenyan"}),
        true,
        Some("deep-fixture-provider-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["answer"], "Research answer.");
    assert!(!value["consulted"].as_array().unwrap().is_empty());
    assert!(value.get("output_path").is_none());
    assert!(!config.outputs.exists() || fs::read_dir(config.outputs).unwrap().next().is_none());
    let requests = provider.requests.lock().unwrap();
    assert_eq!(
        requests[0].0["authorization"],
        "Bearer deep-fixture-provider-key"
    );
    assert_eq!(
        requests[1].0["authorization"],
        "Bearer deep-fixture-provider-key"
    );
    assert!(requests[1].1.to_string().contains("文言"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn credentials_capabilities_and_invalid_overrides_fail_before_provider_or_writes() {
    let provider = Provider::new(&[]).await;
    let (_root, _config, app) = fixture(Surface::Web, &provider);
    for body in [
        json!({"question":"q","meta":{"API-Key":"body-key"}}),
        json!({"question":"q","model":42}),
        json!({"question":"q","deep":"false"}),
    ] {
        assert_eq!(
            call(app.clone(), "POST", "/api/ask", body, true, None)
                .await
                .0,
            StatusCode::BAD_REQUEST
        )
    }
    assert_eq!(
        call(
            app.clone(),
            "POST",
            "/api/ask",
            json!({"question":"q"}),
            true,
            Some(&"x".repeat(501))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    for path in [
        "/api/ask",
        "/api/compile",
        "/api/lint/fix",
        "/api/entities/extract",
        "/api/xici/generate",
    ] {
        let (status, value) = call(
            app.clone(),
            "POST",
            path,
            json!({"question":"q"}),
            false,
            Some("key-does-not-grant-authority"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {value}")
    }
    assert!(provider.requests.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compile_both_surfaces_report_only_actual_accepted_articles() {
    for surface in [Surface::Web, Surface::Agent] {
        let extraction=json!({"articles":[{"slug":"accepted-new","title":"Accepted new","summary":"Real summary","tags":["learning"],"content":"A substantial article actually compiled from the supplied original text."}]}).to_string();
        let provider = Provider::new(&[&extraction]).await;
        let (_root, config, app) = fixture(surface, &provider);
        fs::create_dir_all(config.raw.join("original")).unwrap();
        fs::write(
            config.raw.join("original/index.md"),
            "---\ntitle: Original\nunknown: 42\n---\nSource original body 原典𠮷.",
        )
        .unwrap();
        let (status, value) = call(app, "POST", "/api/compile", json!({}), true, None).await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["articles_created"], 1);
        assert!(config.concepts.join("accepted-new.md").is_file());
        let raw = fs::read_to_string(config.raw.join("original/index.md")).unwrap();
        assert!(raw.contains("Source original body 原典𠮷."));
        assert!(raw.contains("42"));
        assert_eq!(provider.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn xici_entities_and_tones_routes_use_real_core_results_and_profile_defaults() {
    let provider=Provider::new(&["學問之道，求諸實證。",r#"{"people":[{"name":"Alpha thinker","name_local":"甲","articles":["alpha"]}],"events":[],"places":[]}"#]).await;
    let (_root, config, app) = fixture(Surface::Web, &provider);
    let (status, value) = call(
        app.clone(),
        "POST",
        "/api/xici/generate",
        json!({}),
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["lang"], "zh");
    assert_eq!(value["text"], "學問之道，求諸實證。");
    assert!(config.meta.join("xici-zh.json").exists());
    let (status, value) = call(
        app.clone(),
        "POST",
        "/api/entities/extract",
        json!({}),
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["people"][0]["name"], "Alpha thinker");
    assert!(config.meta.join("entities.json").exists());
    let (status, value) = call(app.clone(), "GET", "/api/tones", json!({}), false, None).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["tones"].as_array().unwrap().len(), 5);
    let (status, value) = call(
        app,
        "POST",
        "/api/xici/generate",
        json!({"lang":"missing"}),
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(value["valid"].as_array().unwrap().contains(&json!("zh")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deep_lint_both_envelopes_and_background_web_fix_are_real_and_job_specific() {
    for surface in [Surface::Web, Surface::Agent] {
        let provider = Provider::new(&["The current articles lack cross-links."]).await;
        let (_root, _config, app) = fixture(surface, &provider);
        let (status, value) =
            call(app, "POST", "/api/lint", json!({"deep":true}), true, None).await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["report"], "The current articles lack cross-links.");
        assert_eq!(
            value.get("status").and_then(Value::as_str),
            if surface == Surface::Agent {
                Some("ok")
            } else {
                None
            }
        );
    }
    let provider = Provider::new(&[]).await;
    let (_root, config, app) = fixture(Surface::Web, &provider);
    let mut library = Library::open(config.clone()).unwrap();
    let other = library.enqueue_job("doctor", json!({}), 3).unwrap();
    drop(library);
    let (status, value) = call(app, "POST", "/api/lint/fix", json!({}), true, None).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["status"], "started");
    let id = value["job_id"].as_str().unwrap();
    let mut complete = false;
    let mut last_state = String::new();
    for _ in 0..100 {
        if let Ok(library) = Library::open(config.clone()) {
            let job = library.job(id).unwrap();
            last_state = format!("{job:?}");
            if job.status == llmbase_core::jobs::JobStatus::Succeeded {
                assert_eq!(
                    library.job(&other.id).unwrap().status,
                    llmbase_core::jobs::JobStatus::Queued
                );
                complete = true;
                break;
            }
            assert!(
                !matches!(
                    job.status,
                    llmbase_core::jobs::JobStatus::Failed | llmbase_core::jobs::JobStatus::Partial
                ),
                "{job:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(complete, "background repair did not complete: {last_state}");
    let report: Value =
        serde_json::from_slice(&fs::read(config.meta.join("health.json")).unwrap()).unwrap();
    assert!(report["results"]["total_issues"].is_number());
    let result: Value =
        serde_json::from_slice(&fs::read(config.meta.join("last_fix.json")).unwrap()).unwrap();
    assert_eq!(
        result,
        json!({"status":"completed","fixes":[],"fix_count":0})
    );
    assert!(provider.requests.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn background_web_fix_retries_post_claim_recovery_lock_contention_with_same_job() {
    use llmbase_core::store::WriterSession;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex as StdMutex, mpsc};
    let provider = Provider::new(&[]).await;
    let root = TempDir::new().unwrap();
    let mut config = LibraryConfig::load(root.path(), "http-model").unwrap();
    config.settings["entities"] = json!({"enabled":true});
    fs::create_dir_all(&config.concepts).unwrap();
    for name in ["Alpha", "Beta", "Gamma"] {
        fs::write(config.concepts.join(format!("{}.md",name.to_lowercase())),format!("---\ntitle: {name}\nsummary: knowledge evidence\ntags: [{name}]\nunknown: 42\n---\nThis is substantive accepted article content about {name}, with enough detail to preserve it during cleanup. 原典𠮷.\n")).unwrap();
    }
    let mut library = Library::open(config.clone()).unwrap();
    library.rebuild_metadata().unwrap();
    let other = library.enqueue_job("doctor", json!({}), 3).unwrap();
    drop(library);
    // Fire the barrier once, exactly when the claimed repair operation is
    // about to open its library handle (post-claim, pre-operation).
    let fired = Arc::new(AtomicBool::new(false));
    let (open_tx, open_rx) = mpsc::channel::<()>();
    let (held_tx, held_rx) = mpsc::channel::<()>();
    let held_rx = Arc::new(StdMutex::new(held_rx));
    let mut application = Application::new(vec![config.clone()]).unwrap();
    application.set_operation_open_barrier(Some({
        let fired = fired.clone();
        Arc::new(move || {
            if !fired.swap(true, Ordering::SeqCst) {
                let _ = open_tx.send(());
                let _ = held_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30));
            }
        })
    }));
    let gateway = Gateway::new(application, "http-model".into());
    let grant =
        AuthGrant::new("fixture-owner-model-token", Principal::local("http-model")).unwrap();
    let app = router(
        HttpState::new(gateway, Surface::Web, true, vec![grant]).with_models(Some(AskModels {
            generation: provider.client.clone(),
            embedding: None,
        })),
    );
    let (status, value) = call(app, "POST", "/api/lint/fix", json!({}), true, None).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["status"], "started");
    let id = value["job_id"].as_str().unwrap().to_owned();
    open_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("claimed operation never reached its library open");
    // A concurrent status reader holds the brief recovery writer lock across
    // the first operation-open attempts, then releases it.
    let session = WriterSession::acquire(&config).unwrap();
    held_tx.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    drop(session);
    let mut complete = false;
    let mut last_state = String::new();
    for _ in 0..100 {
        if let Ok(library) = Library::open(config.clone()) {
            let job = library.job(&id).unwrap();
            last_state = format!("{job:?}");
            if job.status == llmbase_core::jobs::JobStatus::Succeeded {
                assert_eq!(job.attempts, 1, "same lease; the job was never re-claimed");
                assert_eq!(
                    library.job(&other.id).unwrap().status,
                    llmbase_core::jobs::JobStatus::Queued
                );
                complete = true;
                break;
            }
            assert!(
                !matches!(
                    job.status,
                    llmbase_core::jobs::JobStatus::Failed | llmbase_core::jobs::JobStatus::Partial
                ),
                "{job:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(complete, "background repair did not complete: {last_state}");
    assert!(fired.load(Ordering::SeqCst));
    let result: Value =
        serde_json::from_slice(&fs::read(config.meta.join("last_fix.json")).unwrap()).unwrap();
    assert_eq!(
        result,
        json!({"status":"completed","fixes":[],"fix_count":0}),
        "exactly one accepted repair outcome; nothing replayed"
    );
    assert!(provider.requests.lock().unwrap().is_empty());
}
