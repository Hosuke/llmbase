use llmbase_core::{Error, Library, LibraryConfig, compat_workflows, document::Document};
use llmbase_model::{CancellationToken, Client, ModelConfig, ModelError};
use serde_json::{Value, json};
use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

struct Provider {
    client: Client,
    calls: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Provider {
    async fn new(replies: Vec<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut cfg = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "workflow-model",
        );
        cfg.retry.max_attempts = 1;
        let client = Client::new(cfg).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (c, r) = (calls.clone(), requests.clone());
        let task = tokio::spawn(async move {
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let end;
                loop {
                    let mut buffer = [0; 4096];
                    let n = socket.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        end = pos + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|s| s.trim().parse().unwrap())
                    })
                    .unwrap();
                while bytes.len() < end + length {
                    let mut buffer = [0; 4096];
                    let n = socket.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                }
                r.lock()
                    .unwrap()
                    .push(serde_json::from_slice(&bytes[end..end + length]).unwrap());
                c.fetch_add(1, Ordering::SeqCst);
                let body = reply.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        Self {
            client,
            calls,
            requests,
            task,
        }
    }
}
fn reply(text: &str) -> Value {
    json!({"choices":[{"message":{"content":text},"finish_reason":"stop"}],"usage":{"total_tokens":13}})
}
fn fixture() -> (TempDir, LibraryConfig) {
    let dir = TempDir::new().unwrap();
    let cfg = LibraryConfig::load(dir.path(), "workflow").unwrap();
    for path in [&cfg.raw, &cfg.concepts, &cfg.outputs, &cfg.meta] {
        fs::create_dir_all(path).unwrap();
    }
    (dir, cfg)
}
fn articles(cfg: &LibraryConfig) {
    let mut rows = Vec::new();
    for (slug, title) in [("a", "Alpha"), ("b", "Beta"), ("c", "Gamma")] {
        fs::write(
            cfg.concepts.join(format!("{slug}.md")),
            format!(
                "---\ntitle: {title}\nsummary: knowledge evidence\n---\n原典𠮷 knowledge evidence"
            ),
        )
        .unwrap();
        rows.push(json!({"slug":slug,"title":title,"summary":"knowledge evidence"}));
    }
    fs::write(cfg.meta.join("index.json"), json!(rows).to_string()).unwrap();
}
fn raw(cfg: &LibraryConfig, name: &str) -> Vec<u8> {
    fs::create_dir_all(cfg.raw.join(name)).unwrap();
    let bytes =
        format!("---\r\ntitle: {name}\r\nunknown: 42\r\n---\r\n 原典𠮷 body\r\nTail \t\r\n")
            .into_bytes();
    fs::write(cfg.raw.join(name).join("index.md"), &bytes).unwrap();
    bytes
}
fn extraction(slug: &str) -> Value {
    reply(&json!({"articles":[{"slug":slug,"title":slug,"summary":"summary","tags":[],"content":"A substantive original body 原典𠮷."}]}).to_string())
}
async fn query(
    cfg: LibraryConfig,
    client: Client,
    args: Value,
    cancel: CancellationToken,
) -> llmbase_core::Result<Value> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || compat_workflows::query(cfg, client, args, cancel, handle))
        .await
        .unwrap()
}
async fn compile(
    cfg: LibraryConfig,
    client: Client,
    args: Value,
    cancel: CancellationToken,
) -> llmbase_core::Result<Value> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        compat_workflows::compile(cfg, client, args, cancel, handle)
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_query_real_selector_answer_judge_and_accepted_fileback() {
    let (_dir, cfg) = fixture();
    articles(&cfg);
    let judge = json!({"promote":true,"slug":"new-concept","title":"New","summary":"summary","tags":[],"content":"New original article knowledge."});
    let provider = Provider::new(vec![
        reply("Alpha\nBeta\nGamma"),
        reply("A complete answer."),
        reply(&judge.to_string()),
    ])
    .await;
    let result=query(cfg.clone(),provider.client.clone(),json!({"mode":"deep","question":"knowledge evidence","file_back":true,"return_context":true,"promote":true,"tone":"scholar"}),CancellationToken::new()).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    assert_eq!(result["value"]["promotion"]["promoted"], true);
    assert!(
        cfg.base
            .join(result["value"]["output_path"].as_str().unwrap())
            .exists()
    );
    let doc = Document::parse(fs::read(cfg.concepts.join("new-concept.md")).unwrap()).unwrap();
    assert!(doc.metadata["sources"][0]["created"].is_string());
    assert_eq!(
        result["trace"]["model_boundary"],
        "llmbase_model::Client::complete"
    );
    assert_eq!(result["trace"]["model_calls"].as_array().unwrap().len(), 3);
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests[0]["max_tokens"], 1024);
    assert!(
        requests[1]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("academic scholar")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_query_bare_wenyan_profile_and_marp_keep_original_context() {
    let (_dir, mut cfg) = fixture();
    articles(&cfg);
    cfg.settings = json!({"languages":{"default":"wy","profiles":{"wy":{"sections":[{"code":"wy","key":"文言","header":""}]}}}});
    let provider = Provider::new(vec![reply("文言答。")]).await;
    let result = query(
        cfg,
        provider.client.clone(),
        json!({"question":"knowledge","output_format":"marp","tone":"wenyan"}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result["value"], "文言答。");
    let requests = provider.requests.lock().unwrap();
    let system = requests[0]["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("Marp slide deck") && system.contains("文言"));
    assert!(
        requests[0]["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("原典𠮷")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_compile_drafts_apply_mark_exact_body_and_skip_idempotently() {
    let (_dir, cfg) = fixture();
    let original = raw(&cfg, "book");
    let provider = Provider::new(vec![extraction("article")]).await;
    let result = compile(
        cfg.clone(),
        provider.client.clone(),
        json!({}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result["value"]["status"], "succeeded");
    assert_eq!(result["value"]["articles_created"], 1);
    assert_eq!(result["value"]["articles"][0], "wiki/concepts/article.md");
    let before = Document::parse(original).unwrap();
    let after = Document::parse(fs::read(cfg.raw.join("book/index.md")).unwrap()).unwrap();
    assert_eq!(
        &before.original[before.body_start..],
        &after.original[after.body_start..]
    );
    assert_eq!(after.metadata["unknown"], 42);
    assert_eq!(after.metadata["compiled"], true);
    let second = compile(
        cfg,
        provider.client.clone(),
        json!({}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(second["value"]["status"], "no_changes");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_compile_partial_retains_accepted_article_without_marking_failure() {
    let (_dir, cfg) = fixture();
    raw(&cfg, "a");
    raw(&cfg, "b");
    let provider = Provider::new(vec![extraction("accepted"), reply("{malformed")]).await;
    let result = compile(
        cfg.clone(),
        provider.client.clone(),
        json!({}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result["value"]["status"], "partial");
    assert_eq!(result["value"]["articles_created"], 1);
    assert!(cfg.concepts.join("accepted.md").exists());
    let a = Document::parse(fs::read(cfg.raw.join("a/index.md")).unwrap()).unwrap();
    let b = Document::parse(fs::read(cfg.raw.join("b/index.md")).unwrap()).unwrap();
    assert_eq!(a.metadata["compiled"], true);
    assert!(b.metadata.get("compiled").is_none());
    let lib = Library::open(cfg).unwrap();
    assert_eq!(lib.list(Some("compiled"), true).unwrap().len(), 1);
}

async fn run_compile_job(
    app: &llmbase_core::application::Application,
    principal: &llmbase_core::application::Principal,
    client: Client,
    id: &str,
) -> llmbase_core::jobs::Job {
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        app.run_job(
            principal,
            "workflow",
            llmbase_core::worker::WorkerModels {
                generation: Some(client),
                embedding: None,
            },
            llmbase_core::worker::WorkerSelection {
                worker_id: "outcome-fixture".into(),
                job_id: Some(id.into()),
            },
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_partial_compile_retries_same_job_without_rewriting_accepted_article() {
    use llmbase_core::{
        application::{Application, Principal},
        jobs::JobStatus,
    };
    let (_dir, cfg) = fixture();
    raw(&cfg, "a");
    raw(&cfg, "b");
    let provider = Provider::new(vec![
        extraction("accepted"),
        reply("{malformed"),
        extraction("recovered"),
    ])
    .await;
    let app = Application::new(vec![cfg.clone()]).unwrap();
    let principal = Principal::local("workflow");
    let queued = Library::open(cfg.clone())
        .unwrap()
        .enqueue_job("legacy_compile", json!({}), 3)
        .unwrap();
    let partial = run_compile_job(&app, &principal, provider.client.clone(), &queued.id).await;
    assert_eq!(partial.status, JobStatus::Partial);
    assert_eq!(
        partial.result.as_ref().unwrap()["articles"],
        json!(["wiki/concepts/accepted.md"])
    );
    assert!(partial.error.is_some());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let accepted = fs::read(cfg.concepts.join("accepted.md")).unwrap();
    let accepted_raw = fs::read(cfg.raw.join("a/index.md")).unwrap();

    // Reopen both storage and application to exercise process-restart semantics.
    drop(app);
    let mut library = Library::open(cfg.clone()).unwrap();
    let saved = library.job(&queued.id).unwrap();
    assert_eq!(saved.result, partial.result);
    let retried = library.retry_job(&queued.id).unwrap();
    assert_eq!(retried.id, queued.id);
    assert_eq!(retried.library_id, "workflow");
    assert_eq!(retried.result, partial.result);
    drop(library);
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let app = Application::new(vec![cfg.clone()]).unwrap();
    let completed = run_compile_job(&app, &principal, provider.client.clone(), &queued.id).await;
    assert_eq!(completed.status, JobStatus::Succeeded);
    assert_eq!(completed.id, queued.id);
    assert_eq!(completed.attempts, 2);
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        3,
        "accepted source must not be regenerated"
    );
    assert_eq!(
        fs::read(cfg.concepts.join("accepted.md")).unwrap(),
        accepted
    );
    assert_eq!(fs::read(cfg.raw.join("a/index.md")).unwrap(), accepted_raw);
    assert!(cfg.concepts.join("recovered.md").exists());
    let events = Library::open(cfg).unwrap().job_events(&queued.id).unwrap();
    assert_eq!(
        events
            .iter()
            .map(|v| v["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["queued", "start", "partial", "queued", "start", "ok"]
    );
    assert_eq!(events[2]["payload"]["result"], partial.result.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_compile_provider_failure_never_persists_success() {
    use llmbase_core::{
        application::{Application, Principal},
        jobs::JobStatus,
    };
    let (_dir, cfg) = fixture();
    let original = raw(&cfg, "bad");
    let provider = Provider::new(vec![reply("{malformed")]).await;
    let app = Application::new(vec![cfg.clone()]).unwrap();
    let principal = Principal::local("workflow");
    let queued = Library::open(cfg.clone())
        .unwrap()
        .enqueue_job("legacy_compile", json!({}), 2)
        .unwrap();
    let failed = run_compile_job(&app, &principal, provider.client.clone(), &queued.id).await;
    assert_eq!(failed.status, JobStatus::Failed);
    assert!(failed.error.is_some());
    assert_eq!(failed.result.as_ref().unwrap()["status"], "failed");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read(cfg.raw.join("bad/index.md")).unwrap(), original);
    let library = Library::open(cfg).unwrap();
    assert!(library.list(Some("compiled"), true).unwrap().is_empty());
    let events = library.job_events(&queued.id).unwrap();
    assert_eq!(events.last().unwrap()["event"], "failed");
    assert!(!events.iter().any(|event| event["event"] == "ok"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_and_required_callbacks_do_not_contact_provider() {
    let (_dir, mut cfg) = fixture();
    articles(&cfg);
    raw(&cfg, "book");
    let provider = Provider::new(vec![]).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        query(
            cfg.clone(),
            provider.client.clone(),
            json!({"question":"knowledge"}),
            cancel.clone()
        )
        .await,
        Err(Error::Model(ModelError::Cancelled))
    ));
    assert!(matches!(
        compile(cfg.clone(), provider.client.clone(), json!({}), cancel).await,
        Err(Error::Model(ModelError::Cancelled))
    ));
    cfg.settings = json!({"query":{"required_capabilities":["python_callback"]}});
    assert!(matches!(
        query(
            cfg,
            provider.client.clone(),
            json!({"question":"knowledge"}),
            CancellationToken::new()
        )
        .await,
        Err(Error::Capability(_))
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_empty_native_compile_is_failed_and_raw_is_not_marked() {
    let (_dir, cfg) = fixture();
    raw(&cfg, "book");
    let provider = Provider::new(vec![reply("{\"articles\":[]}")]).await;
    let result = compile(
        cfg.clone(),
        provider.client.clone(),
        json!({}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result["value"]["status"], "failed");
    assert_eq!(result["value"]["articles_created"], 0);
    assert!(
        Document::parse(fs::read(cfg.raw.join("book/index.md")).unwrap())
            .unwrap()
            .metadata
            .get("compiled")
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_all_forces_new_extraction_despite_legacy_source_receipt() {
    let (_dir, cfg) = fixture();
    raw(&cfg, "book");
    let provider = Provider::new(vec![extraction("article"), extraction("article")]).await;
    let first = compile(
        cfg.clone(),
        provider.client.clone(),
        json!({}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(first["value"]["status"], "succeeded");
    let all = compile(
        cfg,
        provider.client.clone(),
        json!({"mode":"all"}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(all["value"]["status"], "succeeded");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_compile_selects_each_indexed_raw_file_and_preserves_original_assets() {
    let (_dir, cfg) = fixture();
    fs::create_dir_all(cfg.raw.join("book/.original")).unwrap();
    let original_asset = b"%PDF-original-binary\0\xff";
    fs::write(cfg.raw.join("book/.original/source.pdf"), original_asset).unwrap();
    fs::write(
        cfg.raw.join("book/.original/unindexed.md"),
        "Preserved original asset; never an extracted chunk.",
    )
    .unwrap();
    let sources = [
        ("top.md", "Top-level original 原典𠮷", true),
        ("book/index.md", "Index part original 原典𠮷", false),
        ("book/part-two.md", "Second part original 原典𠮷", false),
    ];
    let originals: Vec<_> = sources
        .iter()
        .map(|(relative, body, compiled)| {
            let bytes = format!("---\r\ntitle: {relative}\r\nsource: https://example.invalid/shared-document\r\ncompiled: {compiled}\r\n---\r\n{body}\r\n").into_bytes();
            fs::write(cfg.raw.join(relative), &bytes).unwrap();
            (*relative, bytes)
        })
        .collect();
    fs::write(
        cfg.meta.join("compiled_sources.json"),
        json!(["https://example.invalid/shared-document"]).to_string(),
    )
    .unwrap();
    let provider = Provider::new(vec![
        extraction("first-alpha"),
        extraction("first-beta"),
        extraction("second-gamma"),
        extraction("second-delta"),
        extraction("second-epsilon"),
    ])
    .await;
    let new = compile(
        cfg.clone(),
        provider.client.clone(),
        json!({"mode":"new"}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(new["value"]["status"], "succeeded", "{new}");
    assert_eq!(new["value"]["sources_processed"], 2);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let unchanged = compile(
        cfg.clone(),
        provider.client.clone(),
        json!({"mode":"new"}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(unchanged["value"]["status"], "no_changes");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let all = compile(
        cfg.clone(),
        provider.client.clone(),
        json!({"mode":"all"}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(all["value"]["status"], "succeeded", "{all}");
    assert_eq!(all["value"]["sources_processed"], 3);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 5);
    for (relative, original) in originals {
        let before = Document::parse(original).unwrap();
        let after = Document::parse(fs::read(cfg.raw.join(relative)).unwrap()).unwrap();
        assert_eq!(
            &before.original[before.body_start..],
            &after.original[after.body_start..]
        );
        assert_eq!(after.metadata["compiled"], true);
    }
    assert_eq!(
        fs::read(cfg.raw.join("book/.original/source.pdf")).unwrap(),
        original_asset
    );
    let library = Library::open(cfg).unwrap();
    assert_eq!(library.list(Some("raw"), true).unwrap().len(), 3);
    let requests = provider.requests.lock().unwrap();
    assert!(
        requests
            .iter()
            .any(|request| request.to_string().contains("Top-level original"))
    );
    assert!(
        requests
            .iter()
            .all(|request| !request.to_string().contains("Preserved original asset"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn query_overrides_select_and_answer_but_judge_uses_default_model() {
    let (_dir, cfg) = fixture();
    articles(&cfg);
    let provider = Provider::new(vec![
        reply("Alpha\nBeta\nGamma"),
        reply("answer"),
        reply("{\"promote\":false,\"reason\":\"decline\"}"),
    ])
    .await;
    let default = provider.client.clone();
    let scoped = default
        .scoped_overrides(Some("scoped-query-model".into()), None)
        .unwrap();
    let runtime = tokio::runtime::Handle::current();
    let result = tokio::task::spawn_blocking(move || {
        compat_workflows::query_with_clients(
            cfg,
            default,
            scoped,
            json!({"mode":"deep","question":"knowledge","return_context":true,"promote":true}),
            CancellationToken::new(),
            runtime,
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result["value"]["promotion"]["promoted"], false);
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests[0]["model"], "scoped-query-model");
    assert_eq!(requests[1]["model"], "scoped-query-model");
    assert_eq!(requests[2]["model"], "workflow-model");
}
