use llmbase_core::{
    Error, Library, LibraryConfig,
    entities_model::{EntitiesRequest, EntityArticleFormat},
    jobs::JobStatus,
};
use llmbase_model::{CancellationToken, Client, ModelConfig, ModelError};
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
    finish: &'static str,
    status: u16,
    delay: u64,
    edit: Option<(PathBuf, String)>,
}
impl Reply {
    fn value(value: Value) -> Self {
        Self {
            content: value.to_string(),
            finish: "stop",
            status: 200,
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
            "entities-fixture",
        );
        cfg.retry.max_attempts = 1;
        cfg.request_timeout = Duration::from_millis(400);
        let client = Client::new(cfg).unwrap();
        let (tx, requests) = mpsc::unbounded_channel();
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
                    if let Some(pos) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        end = pos + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let size: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|s| s.trim().parse().unwrap())
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
                tx.send(serde_json::from_slice(&bytes[end..end + size]).unwrap())
                    .unwrap();
                if let Some((path, text)) = reply.edit {
                    fs::write(path, text).unwrap();
                }
                if reply.delay > 0 {
                    tokio::time::sleep(Duration::from_millis(reply.delay)).await;
                }
                let body=json!({"model":"entities-fixture","choices":[{"message":{"content":reply.content},"finish_reason":reply.finish}],"usage":{"total_tokens":17}}).to_string();
                let wire = format!(
                    "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let dir = TempDir::new().unwrap();
    let mut cfg = LibraryConfig::load(dir.path(), "entities").unwrap();
    cfg.settings = json!({"entities":{"enabled":true}});
    fs::create_dir_all(&cfg.concepts).unwrap();
    fs::write(cfg.concepts.join("a.md"),"---\ntitle: 孔子 / Confucius\nsummary: Original 國都𠮷\ntags: [history]\n---\nOriginal evidence").unwrap();
    let mut library = Library::open(cfg).unwrap();
    library.sync().unwrap();
    (dir, library)
}
fn output(name: &str, slug: &str) -> Value {
    json!({"people":[{"name":name,"name_local":"孔子","dates":"551 BCE","articles":[slug]}],"events":[],"places":[]})
}
fn token() -> CancellationToken {
    CancellationToken::new()
}

#[tokio::test]
async fn actual_extraction_is_guarded_durable_unicode_and_uses_effective_prompts() {
    let (dir, mut library) = fixture();
    let mut provider=Provider::start(vec![Reply::value(json!({"people":[{"name":"Confucius","name_local":"孔子𠮷","articles":["a"]}],"events":[{"name":"Teaching","name_local":"講學","date":"c.500 BCE","description":"傳道","articles":["a"]}],"places":[{"name":"Qufu","name_local":"曲阜","coords":[35.6,116.99],"articles":["a"]}]}))]).await;
    let request = EntitiesRequest {
        system_prompt: "Runtime entity profile; preserve 原典𠮷.".into(),
        prompt_template: "Count={count}\n{articles}".into(),
        ..Default::default()
    };
    let report = library
        .generate_entities(&provider.client, &request, false, token())
        .await
        .unwrap();
    let sent = provider.requests.recv().await.unwrap();
    assert_eq!(sent["messages"][0]["content"], request.system_prompt);
    assert!(
        sent["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Original 國都𠮷")
    );
    assert_eq!(report["status"], "succeeded");
    assert_eq!(report["result"]["people"][0]["name_local"], "孔子𠮷");
    assert_eq!(
        report["result"]["consulted_sources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(dir.path().join("wiki/_meta/entities.json").exists());
    assert_eq!(
        library
            .job(report["job_id"].as_str().unwrap())
            .unwrap()
            .status,
        JobStatus::Succeeded
    );
    assert_eq!(
        library.entities(false).unwrap()["places"][0]["coords"],
        json!([35.6, 116.99])
    );
}

#[tokio::test]
async fn public_and_private_artifacts_are_isolated_and_authorization_precedes_http() {
    let (_dir, mut library) = fixture();
    library
        .write_source(
            "compiled",
            "b.md",
            b"---\ntitle: PRIVATE-TITLE\nsummary: PRIVATE-SUMMARY\nprivate: true\n---\nSecret"
                .to_vec(),
            None,
        )
        .unwrap();
    let mut provider = Provider::start(vec![
        Reply::value(output("Public", "a")),
        Reply::value(output("Secret", "b")),
    ])
    .await;
    let public = library
        .generate_entities(
            &provider.client,
            &EntitiesRequest::default(),
            false,
            token(),
        )
        .await
        .unwrap();
    let sent = provider.requests.recv().await.unwrap();
    assert!(!sent.to_string().contains("PRIVATE"));
    let request = EntitiesRequest {
        private_inputs: true,
        ..Default::default()
    };
    assert!(matches!(
        library
            .generate_entities(&provider.client, &request, false, token())
            .await,
        Err(Error::Scope)
    ));
    let private = library
        .generate_entities(&provider.client, &request, true, token())
        .await
        .unwrap();
    assert_eq!(private["cache_file"], "entities.private.json");
    assert_eq!(public["result"]["people"][0]["name"], "Public");
    assert_eq!(
        library.entities(false).unwrap()["people"][0]["name"],
        "Public"
    );
    assert_eq!(
        library.entities(true).unwrap()["people"][0]["name"],
        "Secret"
    );
}

#[tokio::test]
async fn disabled_and_empty_sources_are_not_extraction_successes() {
    let (_dir, mut library) = fixture();
    let provider = Provider::start(vec![]).await;
    library.config.settings["entities"]["enabled"] = json!(false);
    let disabled = library
        .generate_entities(
            &provider.client,
            &EntitiesRequest::default(),
            false,
            token(),
        )
        .await
        .unwrap();
    assert_eq!(disabled["status"], "disabled");
    assert!(library.jobs().unwrap().is_empty());
    let source = library.list(Some("compiled"), true).unwrap().remove(0);
    library
        .delete_source(&source.source_id, &source.content_hash)
        .unwrap();
    let empty = library
        .generate_entities(
            &provider.client,
            &EntitiesRequest {
                force: true,
                ..Default::default()
            },
            false,
            token(),
        )
        .await
        .unwrap();
    assert_eq!(empty["status"], "no_changes");
    assert_eq!(empty["result"]["generated"], false);
}

#[tokio::test]
async fn malformed_empty_truncated_unknown_refs_and_invalid_coords_preserve_previous_artifact() {
    for mut reply in [
        Reply {
            content: "{invalid".into(),
            ..Reply::value(Value::Null)
        },
        Reply {
            content: String::new(),
            ..Reply::value(Value::Null)
        },
        Reply::value(json!({"people":[],"events":[],"places":[]})),
        Reply::value(json!([])),
        Reply::value(output("Unknown", "outside")),
        Reply::value(
            json!({"people":[],"events":[],"places":[{"name":"Invalid","coords":[91,0],"articles":["a"]}]}),
        ),
        Reply::value(output("Valid", "a")),
    ] {
        let (_dir, mut library) = fixture();
        let path = library.config.meta.join("entities.json");
        let prior = output("Old", "a").to_string();
        fs::write(&path, &prior).unwrap();
        if reply.content.contains("Valid") {
            reply.finish = "length";
        }
        let provider = Provider::start(vec![reply]).await;
        assert!(
            library
                .generate_entities(
                    &provider.client,
                    &EntitiesRequest::default(),
                    false,
                    token()
                )
                .await
                .is_err()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), prior);
        assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Failed);
    }
}

#[tokio::test]
async fn unindexed_privacy_change_and_concurrent_artifact_edit_fail_cas() {
    for cache_edit in [false, true] {
        let (_dir, mut library) = fixture();
        let path = library.config.meta.join("entities.json");
        let prior = output("Old", "a").to_string();
        fs::write(&path, &prior).unwrap();
        let mut reply = Reply::value(output("New", "a"));
        reply.edit = Some(if cache_edit {
            (path.clone(), "human accepted edit".into())
        } else {
            (
                library.config.concepts.join("a.md"),
                "---\ntitle: Private now\nprivate: true\n---\nsecret".into(),
            )
        });
        let provider = Provider::start(vec![reply]).await;
        assert!(matches!(
            library
                .generate_entities(
                    &provider.client,
                    &EntitiesRequest::default(),
                    false,
                    token()
                )
                .await,
            Err(Error::Conflict(_))
        ));
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            if cache_edit {
                "human accepted edit".to_owned()
            } else {
                prior
            }
        );
    }
}

#[tokio::test]
async fn bounded_batches_deduplicate_transitively_across_original_script_names() {
    let (_dir, mut library) = fixture();
    for slug in ["b", "c"] {
        library
            .write_source(
                "compiled",
                &format!("{slug}.md"),
                format!("---\ntitle: {slug}\nsummary: summary\n---\nBody").into_bytes(),
                None,
            )
            .unwrap();
    }
    let replies = vec![
        json!({"people":[{"name":"Confucius","dates":"短","articles":["a"]}],"events":[],"places":[]}),
        json!({"people":[{"name_local":"孔子","dates":"長長𠮷","articles":["b"]}],"events":[],"places":[]}),
        json!({"people":[{"name":"CONFUCIUS","name_local":"孔子","articles":["c"]}],"events":[],"places":[]}),
    ];
    let mut provider = Provider::start(replies.into_iter().map(Reply::value).collect()).await;
    let request = EntitiesRequest {
        batch_size: 1,
        article_format: EntityArticleFormat::Tags,
        ..Default::default()
    };
    let report = library
        .generate_entities(&provider.client, &request, false, token())
        .await
        .unwrap();
    let people = report["result"]["people"].as_array().unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0]["articles"], json!(["a", "b", "c"]));
    assert_eq!(people[0]["dates"], "長長𠮷");
    assert_eq!(report["generations"].as_array().unwrap().len(), 3);
    for _ in 0..3 {
        assert!(
            provider.requests.recv().await.unwrap()["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("tags:")
        );
    }
}

#[tokio::test]
async fn cancellation_and_unsupported_callback_profiles_never_publish_success() {
    let (_dir, mut library) = fixture();
    let provider = Provider::start(vec![]).await;
    let request = EntitiesRequest {
        required_capabilities: vec!["python_entity_formatter".into()],
        ..Default::default()
    };
    assert!(matches!(
        library
            .generate_entities(&provider.client, &request, false, token())
            .await,
        Err(Error::Capability(_))
    ));
    let cancel = token();
    cancel.cancel();
    assert!(matches!(
        library
            .generate_entities(&provider.client, &EntitiesRequest::default(), false, cancel)
            .await,
        Err(Error::Model(ModelError::Cancelled))
    ));
    let mut delayed = Reply::value(output("Later", "a"));
    delayed.delay = 200;
    let provider = Provider::start(vec![delayed]).await;
    let cancel = token();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        trigger.cancel();
    });
    assert!(matches!(
        library
            .generate_entities(&provider.client, &EntitiesRequest::default(), false, cancel)
            .await,
        Err(Error::Model(ModelError::Cancelled))
    ));
    assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Cancelled);
    assert!(!library.config.meta.join("entities.json").exists());
}

#[tokio::test]
async fn a_later_batch_failure_does_not_publish_partial_entities() {
    let (_dir, mut library) = fixture();
    library
        .write_source(
            "compiled",
            "b.md",
            b"---\ntitle: Second\n---\nBody".to_vec(),
            None,
        )
        .unwrap();
    let path = library.config.meta.join("entities.json");
    let previous = output("Previous", "a").to_string();
    fs::write(&path, &previous).unwrap();
    let mut invalid = Reply::value(Value::Null);
    invalid.content = "{invalid".into();
    let provider = Provider::start(vec![Reply::value(output("First batch", "a")), invalid]).await;
    let request = EntitiesRequest {
        batch_size: 1,
        ..Default::default()
    };
    assert!(
        library
            .generate_entities(&provider.client, &request, false, token())
            .await
            .is_err()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), previous);
    assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Failed);
}

#[tokio::test]
async fn revoking_an_uncited_consulted_source_withholds_public_aggregate() {
    let (_dir, mut library) = fixture();
    library
        .write_source(
            "compiled",
            "b.md",
            b"---\ntitle: Other public context\nsummary: Details consulted\n---\nBody".to_vec(),
            None,
        )
        .unwrap();
    let provider = Provider::start(vec![Reply::value(output("References only A", "a"))]).await;
    library
        .generate_entities(
            &provider.client,
            &EntitiesRequest::default(),
            false,
            token(),
        )
        .await
        .unwrap();
    assert_eq!(
        library.entities(false).unwrap()["people"][0]["articles"],
        json!(["a"])
    );
    fs::write(
        library.config.concepts.join("b.md"),
        "---\ntitle: Restricted now\nprivate: true\n---\nBody",
    )
    .unwrap();
    // A live private edit is hidden by the source-list check before aggregate
    // validation runs; an indexed revocation reaches the aggregate conflict.
    let denied = library.entities(false);
    assert!(
        matches!(denied, Err(Error::Conflict(_) | Error::NotFound(_))),
        "public aggregate must be withheld: {denied:?}"
    );
    library.sync().unwrap();
    assert!(matches!(library.entities(false), Err(Error::Conflict(_))));
}
