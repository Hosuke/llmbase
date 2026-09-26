use llmbase_core::{
    Error, Library, LibraryConfig,
    maintenance_model::{IntroductionStrategy, TaxonomyRequest, XiciRequest},
};
use llmbase_model::{CancellationToken, Client, ModelConfig};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
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
    finish: &'static str,
    delay: u64,
    edit: Option<(PathBuf, String)>,
}
impl Reply {
    fn text(text: &str) -> Self {
        Self {
            content: text.into(),
            status: 200,
            finish: "stop",
            delay: 0,
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
        self.task.abort();
    }
}
impl Provider {
    async fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut cfg = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "native-fixture",
        );
        cfg.retry.max_attempts = 1;
        cfg.request_timeout = Duration::from_millis(300);
        let client = Client::new(cfg).unwrap();
        let (tx, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let end;
                loop {
                    let mut buf = [0; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        end = at + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let len = headers
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                while bytes.len() < end + len {
                    let mut buf = [0; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                }
                tx.send(serde_json::from_slice(&bytes[end..end + len]).unwrap())
                    .unwrap();
                if let Some((path, text)) = reply.edit {
                    fs::write(path, text).unwrap();
                }
                if reply.delay > 0 {
                    tokio::time::sleep(Duration::from_millis(reply.delay)).await;
                }
                let body=if reply.status==200{json!({"model":"native-fixture","choices":[{"message":{"content":reply.content},"finish_reason":reply.finish}],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}})}else{json!({"error":{"message":"PROVIDER-SECRET-MESSAGE"}})}.to_string();
                let wire = format!(
                    "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.status,
                    body.len(),
                    body
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
fn fixture() -> (TempDir, Library) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    fs::write(root.path().join("wiki/concepts/a.md"),"---\ntitle: Learning\nsummary: Accepted learning summary\ntags: [practice]\nunknown: [42, null, 文言]\n---\nHuman accepted article body, preserved exactly during metadata-only category synchronization.\n").unwrap();
    fs::write(root.path().join("wiki/concepts/b.md"),"---\ntitle: Cultivation\nsummary: Accepted cultivation summary\ntags: [practice]\n---\nA distinct accepted article that must appear once in the generated taxonomy.\n").unwrap();
    fs::write(root.path().join("wiki/concepts/private.md"),"---\ntitle: SECRET TITLE\nsummary: SECRET SUMMARY\nprivate: true\ntags: [secret]\n---\nSECRET BODY").unwrap();
    let mut library =
        Library::open(LibraryConfig::load(root.path(), "model-maintenance").unwrap()).unwrap();
    library.sync().unwrap();
    (root, library)
}
const TREE: &str =
    r#"[{"id":"learning","label":"Learning","children":[],"article_slugs":["a","a"]}]"#;

#[tokio::test]
async fn actual_taxonomy_generation_assigns_all_public_articles_and_journals_requested_tag_sync() {
    let (_root, mut library) = fixture();
    let before = library.get("a", None, false).unwrap();
    let mut provider = Provider::start(vec![Reply::text(TREE)]).await;
    let request = TaxonomyRequest {
        sync_category_tags: true,
        ..Default::default()
    };
    let result = library
        .generate_taxonomy(&provider.client, &request, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(result["result"]["generated"], true);
    assert_eq!(result["result"]["article_count"], 2);
    assert_eq!(
        result["result"]["categories"][0]["article_slugs"],
        json!(["a"])
    );
    assert_eq!(
        result["result"]["categories"][1]["article_slugs"],
        json!(["b"])
    );
    assert_eq!(result["generations"][0]["usage"]["total_tokens"], 18);
    let sent = provider.requests.recv().await.unwrap();
    assert!(!sent.to_string().contains("SECRET"));
    assert!(sent.to_string().contains("English"));
    assert_eq!(sent["max_tokens"], 8192);
    let after = library.get("a", None, false).unwrap();
    assert_eq!(after.metadata["unknown"], before.metadata["unknown"]);
    assert_eq!(
        &after.content[after.body_start..],
        &before.content[before.body_start..]
    );
    assert!(
        after.metadata["tags"]
            .as_array()
            .unwrap()
            .contains(&json!("category:learning"))
    );
    let cache: Value =
        serde_json::from_slice(&fs::read(library.config.meta.join("taxonomy.json")).unwrap())
            .unwrap();
    assert_eq!(cache["categories"], result["result"]["categories"]);
    assert_eq!(
        library
            .job(result["job_id"].as_str().unwrap())
            .unwrap()
            .status,
        llmbase_core::jobs::JobStatus::Succeeded
    );
}

#[tokio::test]
async fn locked_accepted_taxonomy_is_not_regenerated_or_overwritten() {
    let (_root, mut library) = fixture();
    let mut private_provider = Provider::start(vec![Reply::text("must not be called")]).await;
    let private_request = TaxonomyRequest {
        private_inputs: true,
        sync_category_tags: true,
        ..Default::default()
    };
    assert!(matches!(
        library
            .generate_taxonomy(
                &private_provider.client,
                &private_request,
                true,
                CancellationToken::new()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(private_provider.requests.try_recv().is_err());
    library.set_taxonomy(json!({"categories":[{"id":"manual","label":"Human accepted","article_slugs":["a","b"],"children":[]}],"custom":"retain"}),true).unwrap();
    let mut provider = Provider::start(vec![Reply::text("must not be called")]).await;
    let result = library
        .generate_taxonomy(
            &provider.client,
            &TaxonomyRequest::default(),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["reason"], "locked");
    assert_eq!(result["cached"], true);
    assert!(provider.requests.try_recv().is_err());
    assert!(!library.config.meta.join("taxonomy.json").exists());
}

#[tokio::test]
async fn model_failure_and_unknown_taxonomy_references_are_errors_with_failed_jobs() {
    for reply in [
        Reply::text("not JSON"),
        Reply::text(r#"[{"id":"bad","label":"Bad","article_slugs":["private"],"children":[]}]"#),
        Reply {
            finish: "length",
            ..Reply::text(TREE)
        },
        Reply {
            status: 500,
            ..Reply::text("")
        },
    ] {
        let (_root, mut library) = fixture();
        let provider = Provider::start(vec![reply]).await;
        assert!(
            library
                .generate_taxonomy(
                    &provider.client,
                    &TaxonomyRequest::default(),
                    true,
                    CancellationToken::new()
                )
                .await
                .is_err()
        );
        assert!(!library.config.meta.join("taxonomy.json").exists());
        let jobs = library.jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, llmbase_core::jobs::JobStatus::Failed);
        assert!(!jobs[0].error.as_ref().unwrap().contains("PROVIDER-SECRET"));
    }
}

#[tokio::test]
async fn source_changes_or_new_manual_cache_during_provider_work_cannot_be_overwritten() {
    for change_source in [true, false] {
        let (root, mut library) = fixture();
        let file = if change_source {
            root.path().join("wiki/concepts/a.md")
        } else {
            library.config.meta.join("taxonomy.json")
        };
        let accepted = if change_source {
            "---\nprivate: true\n---\nNEW HUMAN CONTENT"
        } else {
            r#"{"locked":true,"categories":[],"custom":"NEW HUMAN TAXONOMY"}"#
        };
        let reply = Reply {
            edit: Some((file.clone(), accepted.into())),
            ..Reply::text(TREE)
        };
        let provider = Provider::start(vec![reply]).await;
        assert!(matches!(
            library
                .generate_taxonomy(
                    &provider.client,
                    &TaxonomyRequest::default(),
                    true,
                    CancellationToken::new()
                )
                .await,
            Err(Error::Conflict(_))
        ));
        assert_eq!(fs::read_to_string(file).unwrap(), accepted);
    }
}

#[tokio::test]
async fn direct_english_and_explicit_private_introductions_use_separate_real_caches() {
    let (_root, mut library) = fixture();
    let mut provider = Provider::start(vec![
        Reply::text("A public English introduction."),
        Reply::text("Private introduction."),
    ])
    .await;
    let public = library
        .generate_xici(
            &provider.client,
            &XiciRequest::default(),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(public["result"]["lang"], "en");
    assert_eq!(public["result"]["article_count"], 2);
    assert_eq!(
        library.xici("en", false).unwrap()["text"],
        "A public English introduction."
    );
    let input = provider.requests.recv().await.unwrap();
    assert!(input.to_string().contains("Requested language: en"));
    assert!(!input.to_string().contains("SECRET"));
    let request = XiciRequest {
        private_inputs: true,
        ..Default::default()
    };
    let private = library
        .generate_xici(&provider.client, &request, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(private["cache_file"], "xici-private-en.json");
    assert_eq!(private["result"]["article_count"], 3);
    assert!(
        provider
            .requests
            .recv()
            .await
            .unwrap()
            .to_string()
            .contains("SECRET")
    );
    assert!(library.xici("private-en", false).is_err());
    assert_eq!(
        library.xici("en", false).unwrap()["text"],
        "A public English introduction."
    );
    assert!(matches!(
        library
            .generate_xici(&provider.client, &request, false, CancellationToken::new())
            .await,
        Err(Error::Scope)
    ));
}

#[tokio::test]
async fn explicit_legacy_zh_base_translates_and_reuses_only_matching_source_fingerprint() {
    let (_root, mut library) = fixture();
    let mut provider = Provider::start(vec![
        Reply::text("學問之道，在於踐履。"),
        Reply::text("Learning is completed in practice."),
        Reply::text("学びは実践にある。"),
    ])
    .await;
    let mut request = XiciRequest {
        strategy: IntroductionStrategy::LegacyZhBase,
        ..Default::default()
    };
    let result = library
        .generate_xici(&provider.client, &request, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["generations"].as_array().unwrap().len(), 2);
    assert_eq!(
        library.xici("zh", false).unwrap()["text"],
        "學問之道，在於踐履。"
    );
    assert!(
        provider
            .requests
            .recv()
            .await
            .unwrap()
            .to_string()
            .contains("文言文")
    );
    assert!(
        provider
            .requests
            .recv()
            .await
            .unwrap()
            .to_string()
            .contains("學問之道")
    );
    request.lang = "ja".into();
    let result = library
        .generate_xici(&provider.client, &request, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["reused_base"], true);
    assert_eq!(result["generations"].as_array().unwrap().len(), 1);
    assert_eq!(
        library.xici("ja", false).unwrap()["text"],
        "学びは実践にある。"
    );
}

#[tokio::test]
async fn failed_translation_and_timeout_do_not_publish_empty_or_wrong_language_success() {
    let (_root, mut library) = fixture();
    let old = r#"{"text":"accepted old English","themes":[],"lang":"en","article_count":2}"#;
    fs::write(library.config.meta.join("xici-en.json"), old).unwrap();
    let provider = Provider::start(vec![
        Reply::text("學問之道。"),
        Reply {
            status: 500,
            ..Reply::text("")
        },
    ])
    .await;
    let request = XiciRequest {
        strategy: IntroductionStrategy::LegacyZhBase,
        ..Default::default()
    };
    assert!(
        library
            .generate_xici(&provider.client, &request, true, CancellationToken::new())
            .await
            .is_err()
    );
    assert!(!library.config.meta.join("xici-zh.json").exists());
    assert_eq!(
        fs::read_to_string(library.config.meta.join("xici-en.json")).unwrap(),
        old
    );
    let provider = Provider::start(vec![Reply {
        delay: 600,
        ..Reply::text("too late")
    }])
    .await;
    assert!(
        library
            .generate_xici(
                &provider.client,
                &XiciRequest::default(),
                true,
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(library.config.meta.join("xici-en.json")).unwrap(),
        old
    );
}

#[tokio::test]
async fn crash_replay_checks_generated_metadata_source_guards_before_publishing() {
    let (root, mut library) = fixture();
    let provider = Provider::start(vec![Reply::text("A generated introduction.")]).await;
    library
        .generate_xici(
            &provider.client,
            &XiciRequest::default(),
            true,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let config = library.config.clone();
    let db = rusqlite::Connection::open(config.database()).unwrap();
    db.execute("UPDATE maintenance_projection_batches SET status='pending' WHERE artifacts LIKE '%xici-en.json%'",[]).unwrap();
    fs::remove_file(config.meta.join("xici-en.json")).unwrap();
    fs::write(
        root.path().join("wiki/concepts/a.md"),
        "---\nprivate: true\n---\nChanged before crash replay",
    )
    .unwrap();
    drop(db);
    drop(library);
    assert!(matches!(
        Library::open(config.clone()),
        Err(Error::Conflict(_))
    ));
    assert!(!config.meta.join("xici-en.json").exists());
}

#[tokio::test]
async fn explicit_fallback_is_marked_partial_and_cancelled_work_never_calls_provider() {
    let (_root, mut library) = fixture();
    let mut provider = Provider::start(vec![Reply::text("invalid JSON")]).await;
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(
        library
            .generate_xici(&provider.client, &XiciRequest::default(), true, cancelled)
            .await
            .is_err()
    );
    assert!(provider.requests.try_recv().is_err());
    let request = TaxonomyRequest {
        fallback_to_tags: true,
        ..Default::default()
    };
    let result = library
        .generate_taxonomy(&provider.client, &request, true, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["result"]["generated"], false);
    assert!(result["degradation"].as_str().unwrap().contains("fallback"));
    assert_eq!(
        library
            .job(result["job_id"].as_str().unwrap())
            .unwrap()
            .status,
        llmbase_core::jobs::JobStatus::Partial
    );
}
