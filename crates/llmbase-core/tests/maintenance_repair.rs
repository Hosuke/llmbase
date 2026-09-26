use llmbase_core::{
    Error, Library, LibraryConfig,
    jobs::JobStatus,
    maintenance_repair::{RepairMode, RepairRequest},
};
use llmbase_model::{CancellationToken, Client, ModelConfig};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
    task::JoinHandle,
};
struct Reply {
    content: String,
    status: u16,
    edit: Option<(PathBuf, String)>,
}
impl Reply {
    fn text(text: &str) -> Self {
        Self {
            content: text.into(),
            status: 200,
            edit: None,
        }
    }
}
struct Provider {
    client: Client,
    requests: mpsc::UnboundedReceiver<Value>,
    task: JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort()
    }
}
impl Provider {
    async fn start(replies: Vec<Reply>) -> Self {
        Self::start_with_reply_lock(replies, None).await
    }

    async fn start_with_reply_lock(replies: Vec<Reply>, mut reply_lock: Option<PathBuf>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut cfg = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "repair-fixture",
        );
        cfg.retry.max_attempts = 1;
        cfg.request_timeout = Duration::from_millis(500);
        let client = Client::new(cfg).unwrap();
        let (tx, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let end;
                loop {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&b[..n]);
                    if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        end = at + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let len = headers
                    .lines()
                    .find_map(|s| {
                        let (k, v) = s.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                while bytes.len() < end + len {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&b[..n])
                }
                tx.send(serde_json::from_slice(&bytes[end..end + len]).unwrap())
                    .unwrap();
                if let Some(path) = reply_lock.take() {
                    // A model request must never retain the library writer lock.
                    // Hold it across the reply to force post-model contention.
                    let file = fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(path)
                        .unwrap();
                    fs2::FileExt::try_lock_exclusive(&file).unwrap();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(150));
                        fs2::FileExt::unlock(&file).unwrap();
                    });
                }
                if let Some((path, text)) = reply.edit {
                    fs::write(path, text).unwrap()
                }
                let body=if reply.status==200{json!({"model":"repair-fixture","choices":[{"message":{"content":reply.content},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}})}else{json!({"error":{"message":"SECRET PROVIDER ERROR"}})}.to_string();
                let wire = format!(
                    "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    reply.status,
                    body.len()
                );
                let _ = socket.write_all(wire.as_bytes()).await;
            }
        });
        Self {
            client,
            requests,
            task,
        }
    }
}
fn source(root: &Path, slug: &str, metadata: Value, body: &str) {
    let path = root.join("wiki/concepts").join(format!("{slug}.md"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("---\n{}\n---\n{body}", metadata)).unwrap()
}
fn open(root: &Path) -> Library {
    let mut library = Library::open(LibraryConfig::load(root, "repair-tests").unwrap()).unwrap();
    library.sync().unwrap();
    library
}
fn request(mode: RepairMode) -> RepairRequest {
    RepairRequest {
        mode,
        regenerate_taxonomy: false,
        ..Default::default()
    }
}
const BODY: &str = "This is accepted article prose with enough detail to distinguish real content from a near-empty stub. It must remain exactly preserved.";

fn hold_repair_writer(library: &Library) -> fs::File {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(library.config.meta.join("llmbase.writer.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    file
}

#[tokio::test]
async fn repair_waits_for_internal_writer_before_accepting_one_job() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "article",
        json!({"title":"Article","summary":"Known","tags":["known"]}),
        BODY,
    );
    let mut library = open(root.path());
    let original = fs::read(root.path().join("wiki/concepts/article.md")).unwrap();
    let observer = open(root.path());
    let lock = hold_repair_writer(&library);
    let req = request(RepairMode::Clean);
    let mut work = Box::pin(library.repair(None, &req, true, CancellationToken::new()));
    tokio::select! {
        result = &mut work => panic!("repair completed while its writer was held: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(60)) => {}
    }
    assert!(observer.jobs().unwrap().is_empty());
    fs2::FileExt::unlock(&lock).unwrap();
    let result = work.await.unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(library.jobs().unwrap().len(), 1);
    assert_eq!(
        fs::read(root.path().join("wiki/concepts/article.md")).unwrap(),
        original
    );
}

#[tokio::test]
async fn repair_internal_writer_wait_cancels_without_accepting_work() {
    let root = TempDir::new().unwrap();
    let mut library = open(root.path());
    let lock = hold_repair_writer(&library);
    let cancel = CancellationToken::new();
    let req = request(RepairMode::Clean);
    let mut work = Box::pin(library.repair(None, &req, true, cancel.clone()));
    tokio::select! {
        result = &mut work => panic!("repair did not wait for its writer: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(60)) => {}
    }
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_millis(250), work)
        .await
        .unwrap();
    assert!(matches!(
        result,
        Err(Error::Model(llmbase_model::ModelError::Cancelled))
    ));
    assert!(library.jobs().unwrap().is_empty());
    fs2::FileExt::unlock(&lock).unwrap();
}

#[tokio::test]
async fn repair_internal_writer_exhaustion_accepts_no_job() {
    let root = TempDir::new().unwrap();
    let mut library = open(root.path());
    let lock = hold_repair_writer(&library);
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        library.repair(
            None,
            &request(RepairMode::Clean),
            true,
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(Error::Busy)));
    assert!(library.jobs().unwrap().is_empty());
    fs2::FileExt::unlock(&lock).unwrap();
}

#[tokio::test]
async fn repair_waits_after_model_without_replaying_model_or_accepted_write() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "article",
        json!({"title":"Article","tags":["known"],"unknown":42}),
        BODY,
    );
    let mut library = open(root.path());
    let mut provider = Provider::start_with_reply_lock(
        vec![Reply::text("One accepted summary.")],
        Some(library.config.meta.join("llmbase.writer.lock")),
    )
    .await;
    let result = library
        .repair(
            Some(&provider.client),
            &request(RepairMode::Metadata),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(result["journal_changes"].as_array().unwrap().len(), 1);
    let jobs = library.jobs().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, JobStatus::Succeeded);
    let accepted = library.get("article", None, true).unwrap();
    assert_eq!(accepted.metadata["summary"], "One accepted summary.");
    assert_eq!(accepted.metadata["unknown"], 42);
    assert_eq!(&accepted.content[accepted.body_start..], BODY);
    assert!(provider.requests.try_recv().is_ok());
    assert!(provider.requests.try_recv().is_err());
}

#[tokio::test]
async fn last_fix_receipts_have_real_counts_and_separate_private_storage() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "public-stub",
        json!({"title":"Public"}),
        "Short",
    );
    source(
        root.path(),
        "private-stub",
        json!({"title":"Secret", "private":true}),
        "Short",
    );
    let mut library = open(root.path());
    let mut req = request(RepairMode::Clean);
    req.persist_last_fix = true;
    let public = library
        .repair(None, &req, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(public["fix_count"], 1);
    let public_bytes = fs::read(library.config.meta.join("last_fix.json")).unwrap();
    let receipt: Value = serde_json::from_slice(&public_bytes).unwrap();
    assert_eq!(
        receipt,
        json!({"status":"completed","fixes":public["fixes"],"fix_count":1})
    );
    assert!(!String::from_utf8_lossy(&public_bytes).contains("private-stub"));
    req.private_inputs = true;
    let private = library
        .repair(None, &req, true, CancellationToken::new())
        .await
        .unwrap();
    let receipt: Value = serde_json::from_slice(
        &fs::read(library.config.meta.join("last_fix-private.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["fix_count"], private["fix_count"]);
    assert_eq!(receipt["status"], "completed");
    assert!(receipt.to_string().contains("private-stub"));
    assert_eq!(
        fs::read(library.config.meta.join("last_fix.json")).unwrap(),
        public_bytes
    );
    req.mode = RepairMode::DeepLint;
    assert!(matches!(
        library
            .repair(None, &req, true, CancellationToken::new())
            .await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn failed_model_repair_never_publishes_completed_last_fix() {
    let root = TempDir::new().unwrap();
    source(root.path(), "article", json!({"title":"Article"}), BODY);
    let mut library = open(root.path());
    let provider = Provider::start(vec![Reply {
        content: String::new(),
        status: 500,
        edit: None,
    }])
    .await;
    let mut req = request(RepairMode::Metadata);
    req.persist_last_fix = true;
    assert!(
        library
            .repair(Some(&provider.client), &req, true, CancellationToken::new())
            .await
            .is_err()
    );
    let receipt: Value =
        serde_json::from_slice(&fs::read(library.config.meta.join("last_fix.json")).unwrap())
            .unwrap();
    assert_eq!(receipt, json!({"status":"failed","fixes":[],"fix_count":0}));
    assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Failed);
}

#[tokio::test]
async fn cleanup_and_url_slug_repair_use_real_journals_preserve_cjk_and_pipe_links() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "garbage",
        json!({"title":"English Title / 中文标题"}),
        "placeholder",
    );
    source(root.path(), "仁", json!({"title":"仁"}), BODY);
    source(
        root.path(),
        "odd/?ref=x",
        json!({"title":"Real article","slug":"odd/?ref=x","unknown":42}),
        BODY,
    );
    source(
        root.path(),
        "links",
        json!({"title":"Links"}),
        &format!("{BODY} [[odd/?ref=x|A label]]"),
    );
    let mut library = open(root.path());
    let garbage = library.get("garbage", None, true).unwrap();
    let cleaned = library
        .repair(
            None,
            &request(RepairMode::Clean),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(cleaned["status"], "succeeded");
    assert!(library.get("仁", None, false).is_ok());
    assert!(library.get(&garbage.source_id, None, true).unwrap().deleted);
    assert!(
        library
            .get(&garbage.source_id, Some(1), true)
            .unwrap()
            .content
            .contains("placeholder")
    );
    let healed = library
        .repair(
            None,
            &request(RepairMode::HealSlugs),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(healed["status"], "succeeded");
    let article = library.get("odd-ref=x", None, false).unwrap();
    assert_eq!(article.metadata["slug"], "odd-ref=x");
    assert_eq!(article.metadata["unknown"], 42);
    assert!(
        library
            .get("links", None, false)
            .unwrap()
            .content
            .contains("[[odd-ref=x|A label]]")
    );
    assert!(!root.path().join("wiki/concepts/odd/?ref=x.md").exists());
    assert!(!healed["journal_changes"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn missing_metadata_uses_real_models_preserves_body_unknown_fields_and_old_revision() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "a",
        json!({"title":"Accepted","unknown":[42,null,"文言"]}),
        BODY,
    );
    let mut library = open(root.path());
    let old = library.get("a", None, false).unwrap();
    let mut provider = Provider::start(vec![
        Reply::text("An evidence-grounded summary."),
        Reply::text(r#"["practice","learning"]"#),
    ])
    .await;
    let result = library
        .repair(
            Some(&provider.client),
            &request(RepairMode::Metadata),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(
        result["stages"][0]["generations"].as_array().unwrap().len(),
        2
    );
    let new = library.get("a", None, false).unwrap();
    assert_eq!(new.metadata["summary"], "An evidence-grounded summary.");
    assert_eq!(new.metadata["tags"], json!(["practice", "learning"]));
    assert_eq!(new.metadata["unknown"], old.metadata["unknown"]);
    assert_eq!(
        &new.content[new.body_start..],
        &old.content[old.body_start..]
    );
    assert_eq!(
        library
            .get(&old.source_id, Some(old.revision), false)
            .unwrap()
            .content,
        old.content
    );
    assert!(
        provider
            .requests
            .recv()
            .await
            .unwrap()
            .to_string()
            .contains("en")
    );
}

#[tokio::test]
async fn aggregate_tag_normalization_completes_multiple_articles_without_cross_visibility_leak() {
    let root = TempDir::new().unwrap();
    for slug in ["a", "b"] {
        source(
            root.path(),
            slug,
            json!({"title":slug,"summary":"Known","tags":["old","alpha","beta","gamma","delta"]}),
            BODY,
        )
    }
    source(
        root.path(),
        "private",
        json!({"title":"SECRET","private":true,"tags":["secretold","SECRET1","SECRET2","SECRET3","SECRET4"]}),
        BODY,
    );
    let mut library = open(root.path());
    let mut provider = Provider::start(vec![
        Reply::text(r#"{"old":"canonical"}"#),
        Reply::text(r#"{"secretold":"privatecanonical"}"#),
    ])
    .await;
    let mut req = request(RepairMode::NormalizeTags);
    req.private_inputs = true;
    let result = library
        .repair(Some(&provider.client), &req, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    for slug in ["a", "b"] {
        let article = library.get(slug, None, false).unwrap();
        assert!(
            article.metadata["tags"]
                .as_array()
                .unwrap()
                .contains(&json!("canonical"))
        );
        assert!(!article.content.contains("privatecanonical"))
    }
    assert!(
        library
            .get("private", None, true)
            .unwrap()
            .content
            .contains("privatecanonical")
    );
    assert!(
        !provider
            .requests
            .recv()
            .await
            .unwrap()
            .to_string()
            .contains("SECRET")
    );
    assert!(
        provider
            .requests
            .recv()
            .await
            .unwrap()
            .to_string()
            .contains("secret")
    );
}

#[tokio::test]
async fn provider_failure_after_an_accepted_write_reports_partial_with_job_owned_receipts() {
    let root = TempDir::new().unwrap();
    for slug in ["a", "b"] {
        source(
            root.path(),
            slug,
            json!({"title":slug,"tags":["known"]}),
            BODY,
        )
    }
    let mut library = open(root.path());
    let provider = Provider::start(vec![
        Reply::text("First article summary."),
        Reply {
            status: 500,
            ..Reply::text("")
        },
    ])
    .await;
    let result = library
        .repair(
            Some(&provider.client),
            &request(RepairMode::Metadata),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(
        library.get("a", None, false).unwrap().metadata["summary"],
        "First article summary."
    );
    assert!(
        library
            .get("b", None, false)
            .unwrap()
            .metadata
            .get("summary")
            .is_none()
    );
    assert_eq!(result["journal_changes"].as_array().unwrap().len(), 1);
    let job = library.job(result["job_id"].as_str().unwrap()).unwrap();
    assert_eq!(job.status, JobStatus::Partial);
    assert!(
        !serde_json::to_string(&job)
            .unwrap()
            .contains("SECRET PROVIDER")
    );
}

#[tokio::test]
async fn concurrent_private_edit_is_not_overwritten_and_invalid_model_json_is_not_success() {
    for invalid in [false, true] {
        let root = TempDir::new().unwrap();
        source(
            root.path(),
            "a",
            json!({"title":"A","summary":"Known"}),
            BODY,
        );
        let mut library = open(root.path());
        let original = fs::read(root.path().join("wiki/concepts/a.md")).unwrap();
        let replacement = "---\nprivate: true\ntitle: Human replacement\n---\nNever overwrite this accepted edit.";
        let reply = if invalid {
            Reply::text("tags: fake")
        } else {
            Reply {
                edit: Some((root.path().join("wiki/concepts/a.md"), replacement.into())),
                ..Reply::text(r#"["one","two"]"#)
            }
        };
        let provider = Provider::start(vec![reply]).await;
        assert!(
            library
                .repair(
                    Some(&provider.client),
                    &request(RepairMode::Metadata),
                    true,
                    CancellationToken::new()
                )
                .await
                .is_err()
        );
        let current = fs::read(root.path().join("wiki/concepts/a.md")).unwrap();
        assert_eq!(
            current,
            if invalid {
                original
            } else {
                replacement.as_bytes().to_vec()
            }
        );
        assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Failed)
    }
}

#[tokio::test]
async fn model_confirmed_duplicate_merge_preserves_both_sources_and_rewrites_links() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "learning",
        json!({"title":"Learning","summary":"Known","tags":["practice"],"sources":["raw-a"]}),
        &format!("{BODY} Additional detail makes the primary longer."),
    );
    source(
        root.path(),
        "learning-variant",
        json!({"title":"Learning variant","summary":"Known","tags":["practice"],"sources":["raw-b"]}),
        "A different accepted treatment with a contested claim that must survive a merge, with its provenance.",
    );
    source(
        root.path(),
        "reader",
        json!({"title":"Reader","summary":"Known","tags":["reader"]}),
        &format!("{BODY} [[learning-variant|Learn]]"),
    );
    let mut library = open(root.path());
    let secondary = library.get("learning-variant", None, false).unwrap();
    let provider = Provider::start(vec![Reply::text(
        r#"{"duplicate":true,"reason":"Same concept, preserve both treatments"}"#,
    )])
    .await;
    let result = library
        .repair(
            Some(&provider.client),
            &request(RepairMode::Deduplicate),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    let primary = library.get("learning", None, false).unwrap();
    assert!(primary.content.contains("contested claim"));
    assert_eq!(primary.metadata["sources"], json!(["raw-a", "raw-b"]));
    assert!(
        library
            .get(&secondary.source_id, None, true)
            .unwrap()
            .deleted
    );
    assert!(
        library
            .get(&secondary.source_id, Some(secondary.revision), true)
            .unwrap()
            .content
            .contains("contested claim")
    );
    assert!(
        library
            .get("reader", None, false)
            .unwrap()
            .content
            .contains("[[learning|Learn]]")
    );
}

#[tokio::test]
async fn related_topics_and_cross_visibility_candidates_are_not_merged() {
    let root = TempDir::new().unwrap();
    for (slug, is_private) in [
        ("ethics", false),
        ("ethics-other", false),
        ("ethics-secret", true),
    ] {
        source(
            root.path(),
            slug,
            json!({"title":slug,"summary":"Known","private":is_private,"tags":["ethics"]}),
            BODY,
        )
    }
    let mut library = open(root.path());
    let mut provider = Provider::start(vec![Reply::text(
        r#"{"duplicate":false,"reason":"Opposing positions"}"#,
    )])
    .await;
    let mut req = request(RepairMode::Deduplicate);
    req.private_inputs = true;
    let result = library
        .repair(Some(&provider.client), &req, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(result["fix_count"], 0);
    assert_eq!(library.list(Some("compiled"), true).unwrap().len(), 3);
    assert!(
        !provider
            .requests
            .recv()
            .await
            .unwrap()
            .to_string()
            .contains("ethics-secret")
    );
    assert!(provider.requests.try_recv().is_err());
}

#[tokio::test]
async fn broken_link_generation_has_real_support_guards_and_no_placeholder_fallback() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "a",
        json!({"title":"A","summary":"Known","tags":["one"]}),
        &format!("{BODY} [[missing]] is the practice of careful attention."),
    );
    let mut library = open(root.path());
    let provider=Provider::start(vec![Reply::text("The available reference describes missing as a practice of careful attention. Its further details are not established.")]).await;
    let result = library
        .repair(
            Some(&provider.client),
            &request(RepairMode::BrokenLinks),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    let stub = library.get("missing", None, false).unwrap();
    assert_eq!(stub.metadata["stub"], true);
    assert_eq!(stub.metadata["generated_from"][0]["revision"], 1);
    assert!(stub.content.contains("not established"));
    source(
        root.path(),
        "b",
        json!({"title":"B","summary":"Known"}),
        &format!("{BODY} [[unsupported]]"),
    );
    library.sync().unwrap();
    let provider = Provider::start(vec![Reply::text("CANNOT_GENERATE")]).await;
    assert!(
        library
            .repair(
                Some(&provider.client),
                &request(RepairMode::BrokenLinks),
                true,
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    assert!(!root.path().join("wiki/concepts/unsupported.md").exists());
}

#[tokio::test]
async fn heal_persists_actual_check_fix_recheck_report_and_deep_lint_is_read_only() {
    let root = TempDir::new().unwrap();
    source(
        root.path(),
        "a",
        json!({"title":"A","summary":"Known","tags":["one"]}),
        BODY,
    );
    let mut library = open(root.path());
    library.rebuild_metadata().unwrap();
    let provider = Provider::start(vec![]).await;
    let mut req = request(RepairMode::Heal);
    req.fix_broken_links = false;
    let result = library
        .repair(Some(&provider.client), &req, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    let report: Value =
        serde_json::from_slice(&fs::read(library.config.meta.join("health.json")).unwrap())
            .unwrap();
    assert_eq!(report["results"]["total_issues"], report["issues_after"]);
    assert!(report["issues_after"].as_u64().unwrap() > 0);
    let before = fs::read(root.path().join("wiki/concepts/a.md")).unwrap();
    let provider = Provider::start(vec![Reply::text(
        "Article a lacks incoming connections; this is a suggestion, not a factual contradiction.",
    )])
    .await;
    let result = library
        .repair(
            Some(&provider.client),
            &request(RepairMode::DeepLint),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(
        result["stages"][0]["value"]["report"]
            .as_str()
            .unwrap()
            .contains("Article a")
    );
    assert_eq!(
        fs::read(root.path().join("wiki/concepts/a.md")).unwrap(),
        before
    );
}

#[tokio::test]
async fn cancellation_and_scope_rejection_happen_before_any_provider_or_source_change() {
    let root = TempDir::new().unwrap();
    source(root.path(), "a", json!({"title":"A"}), BODY);
    let mut library = open(root.path());
    let mut provider = Provider::start(vec![]).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        library
            .repair(
                Some(&provider.client),
                &request(RepairMode::Metadata),
                true,
                cancel
            )
            .await
            .is_err()
    );
    let mut req = request(RepairMode::Metadata);
    req.private_inputs = true;
    assert!(matches!(
        library
            .repair(
                Some(&provider.client),
                &req,
                false,
                CancellationToken::new()
            )
            .await,
        Err(Error::Scope)
    ));
    assert!(provider.requests.try_recv().is_err());
    assert!(library.jobs().unwrap().is_empty());
}

#[tokio::test]
async fn merging_refreshes_legacy_and_accepted_taxonomy_without_changing_manual_structure() {
    for accepted in [false, true] {
        let root = TempDir::new().unwrap();
        source(
            root.path(),
            "learning",
            json!({"title":"Learning","tags":["one"]}),
            &format!("{BODY} Additional primary explanation."),
        );
        source(
            root.path(),
            "learning-variant",
            json!({"title":"Learning variant","tags":["one"]}),
            BODY,
        );
        source(
            root.path(),
            "private",
            json!({"title":"Secret","private":true}),
            BODY,
        );
        let mut library = open(root.path());
        let tree = json!({"categories":[{"id":"human","label":{"en":"Human classification"},"children":[],"article_slugs":["learning","learning-variant"]}],"locked":true,"unknown":{"keep":42}});
        if accepted {
            library.set_taxonomy(tree, true).unwrap();
        } else {
            fs::write(
                library.config.meta.join("taxonomy.json"),
                serde_json::to_vec(&tree).unwrap(),
            )
            .unwrap();
        }
        let provider = Provider::start(vec![]).await;
        let mut req = request(RepairMode::Deduplicate);
        req.confirm_duplicates = false;
        let result = library
            .repair(Some(&provider.client), &req, true, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(result["status"], "succeeded");
        let tree = if accepted {
            let db = rusqlite::Connection::open(library.config.database()).unwrap();
            let text: String = db
                .query_row(
                    "SELECT payload FROM catalog_documents WHERE kind='taxonomy'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            serde_json::from_str::<Value>(&text).unwrap()
        } else {
            serde_json::from_slice::<Value>(
                &fs::read(library.config.meta.join("taxonomy.json")).unwrap(),
            )
            .unwrap()
        };
        assert_eq!(tree["locked"], true);
        assert_eq!(tree["unknown"]["keep"], 42);
        assert_eq!(tree["categories"][0]["article_slugs"], json!(["learning"]));
        assert_eq!(tree["categories"].as_array().unwrap().len(), 1);
        assert!(!tree.to_string().contains("private"));
    }
}
