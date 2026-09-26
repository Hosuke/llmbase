use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use llmbase_core::{
    Error, Library, LibraryConfig,
    jobs::JobStatus,
    profile::{CompileProfile, LanguageProfile, LanguageSection},
};
use llmbase_model::{CancellationToken, Client, ModelConfig};
use serde_json::{Value, json};
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
        let mut config = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "compiler-fixture",
        );
        config.retry.max_attempts = 1;
        let client = Client::new(config).unwrap();
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
                    if let Some(position) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        end = position + 4;
                        break;
                    }
                }
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let size = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
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
                r.lock()
                    .unwrap()
                    .push(serde_json::from_slice(&bytes[end..end + size]).unwrap());
                c.fetch_add(1, Ordering::SeqCst);
                let body = reply.to_string();
                let wire = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(wire.as_bytes()).await;
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

fn response(content: &str, finish: &str) -> Value {
    json!({"model":"compiler-fixture","choices":[{"message":{"content":content},"finish_reason":finish}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}})
}
fn article(slug: &str, body: &str) -> Value {
    json!({"slug":slug,"title":"Orchard knowledge","summary":"A short summary","tags":["botany"],"aliases":[],"content":body})
}
fn output(articles: Vec<Value>) -> Value {
    response(&json!({"articles":articles}).to_string(), "stop")
}
fn token() -> CancellationToken {
    CancellationToken::new()
}
fn library(raw: &str) -> (TempDir, Library, String) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("raw")).unwrap();
    fs::write(dir.path().join("raw/original.md"), raw).unwrap();
    let mut library =
        Library::open(LibraryConfig::load(dir.path(), "compile-test").unwrap()).unwrap();
    library.sync().unwrap();
    let id = library.list(Some("raw"), true).unwrap()[0]
        .source_id
        .clone();
    (dir, library, id)
}

#[tokio::test]
async fn public_compile_never_sends_private_existing_descriptions_or_merges_private_aliases() {
    let (_dir, mut library, id) = library("---\ntitle: Public raw\n---\nPublic evidence.");
    library.write_source("compiled","private.md",b"---\ntitle: Secret title\nsummary: SECRET-SUMMARY\nprivate: true\naliases: [public-output]\n---\nSecret body".to_vec(),None).unwrap();
    library.write_source("compiled","internal.md",b"---\ntitle: Internal title\nsummary: INTERNAL-SUMMARY\nvisibility: internal\n---\nInternal body".to_vec(),None).unwrap();
    library
        .write_source(
            "compiled",
            "visible.md",
            b"---\ntitle: Visible title\nsummary: visible-summary\n---\nVisible body".to_vec(),
            None,
        )
        .unwrap();
    let provider = Provider::new(vec![output(vec![article(
        "public-output",
        "Public generated content.",
    )])])
    .await;
    let report = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    let prompt = provider.requests.lock().unwrap()[0]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        !prompt.contains("SECRET")
            && !prompt.contains("INTERNAL")
            && !prompt.contains("Secret title")
    );
    assert!(prompt.contains("visible-summary"));
    let draft = library.draft(&report.draft_ids[0]).unwrap();
    assert_eq!(draft.target_path, "public-output.md");
    assert!(draft.base_hash.is_none());
    library.apply_draft(&draft.id, None).unwrap();
    assert!(
        library
            .get("private", None, true)
            .unwrap()
            .content
            .contains("Secret body")
    );
    assert_eq!(
        library.get("public-output", None, false).unwrap().metadata["visibility"],
        "public"
    );
}

#[tokio::test]
async fn internal_compile_excludes_private_description_context() {
    let (_dir, mut library, id) = library("---\nvisibility: internal\n---\nInternal evidence.");
    library
        .write_source(
            "compiled",
            "private.md",
            b"---\ntitle: Secret\nsummary: PRIVATE-SUMMARY\nvisibility: private\n---\nSecret"
                .to_vec(),
            None,
        )
        .unwrap();
    library
        .write_source(
            "compiled",
            "internal.md",
            b"---\ntitle: Internal\nsummary: INTERNAL-SUMMARY\nvisibility: internal\n---\nInternal"
                .to_vec(),
            None,
        )
        .unwrap();
    let provider = Provider::new(vec![output(vec![article(
        "new-internal",
        "Internal generated content.",
    )])])
    .await;
    let report = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    let prompt = provider.requests.lock().unwrap()[0]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!prompt.contains("PRIVATE-SUMMARY"));
    assert!(prompt.contains("INTERNAL-SUMMARY"));
    let draft = library.draft(&report.draft_ids[0]).unwrap();
    library.apply_draft(&draft.id, None).unwrap();
    assert_eq!(
        library.get("new-internal", None, true).unwrap().metadata["visibility"],
        "internal"
    );
}

#[tokio::test]
async fn consulted_description_becoming_private_blocks_new_public_draft_apply() {
    let (_dir, mut library, id) = library("Public source body.");
    library
        .write_source(
            "compiled",
            "consulted.md",
            b"---\ntitle: Consulted\nsummary: currently-public-summary\n---\nEarlier content"
                .to_vec(),
            None,
        )
        .unwrap();
    let provider = Provider::new(vec![output(vec![article(
        "new-public",
        "Content derived from supplied description.",
    )])])
    .await;
    let report = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    let consulted = library.get("consulted", None, true).unwrap();
    library
        .write_source(
            "compiled",
            "consulted.md",
            b"---\ntitle: Consulted\nprivate: true\n---\nRestricted now".to_vec(),
            Some(&consulted.content_hash),
        )
        .unwrap();
    assert!(matches!(
        library.apply_draft(&report.draft_ids[0], None),
        Err(Error::Conflict(_))
    ));
    assert!(library.get("new-public", None, true).is_err());
}

#[tokio::test]
async fn source_draft_review_apply_is_native_idempotent_and_preserves_raw_bytes() {
    let raw = "---\ntitle: Source\nunknown: {retained: 42}\nsource_url: https://example.invalid/source\n---\nOriginal orchard evidence 𠮷.";
    let (dir, mut library, id) = library(raw);
    let unrelated = library.enqueue_job("unrelated", json!({}), 1).unwrap();
    let provider = Provider::new(vec![output(vec![article(
        "orchard",
        "Orchards grow fruit.",
    )])])
    .await;
    let report = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    assert_eq!(report.status, JobStatus::Succeeded);
    assert_eq!(report.draft_ids.len(), 1);
    assert_eq!(
        library.job(&unrelated.id).unwrap().status,
        JobStatus::Queued,
        "compile must claim its own job only"
    );
    assert!(library.list(Some("compiled"), true).unwrap().is_empty());
    let draft = library.draft(&report.draft_ids[0]).unwrap();
    assert_eq!(draft.status, "draft");
    assert!(draft.base_hash.is_none());
    let accepted = library.apply_draft(&draft.id, None).unwrap();
    assert_eq!(accepted.status, "applied");
    let compiled = library.get("orchard", None, true).unwrap();
    assert_eq!(compiled.kind, "compiled");
    assert_eq!(compiled.metadata["sources"][0]["unknown"]["retained"], 42);
    assert_eq!(compiled.metadata["sources"][0]["source_id"], id);
    assert_eq!(
        fs::read(dir.path().join("raw/original.md")).unwrap(),
        raw.as_bytes()
    );
    let hits = library
        .search("fruit", 10, Some("compiled"), false)
        .unwrap();
    assert_eq!(hits.results.len(), 1);
    library
        .resolve_evidence(&hits.results[0].evidence, false)
        .unwrap();
    let cfg = library.config.clone();
    drop(library);
    let mut library = Library::open(cfg).unwrap();
    assert_eq!(
        library.apply_draft(&draft.id, None).unwrap().status,
        "applied"
    );
    let again = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    assert!(again.cached);
    assert_eq!(again.draft_ids, report.draft_ids);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn malformed_empty_truncated_and_all_invalid_outputs_never_succeed() {
    for reply in [
        response("", "stop"),
        response("not JSON", "stop"),
        response(
            &json!({"articles":[article("a","prefix")]}).to_string(),
            "length",
        ),
        output(vec![]),
        output(vec![article("../escape", "body")]),
    ] {
        let (dir, mut library, id) = library("Original source.");
        let provider = Provider::new(vec![reply]).await;
        assert!(
            library
                .compile_source(&provider.client, &id, &CompileProfile::default(), token())
                .await
                .is_err()
        );
        let jobs = library.jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Failed);
        assert!(library.list(Some("compiled"), true).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(dir.path().join("raw/original.md")).unwrap(),
            "Original source."
        );
    }
}

#[tokio::test]
async fn partial_article_batch_persists_good_drafts_and_can_retry() {
    let (_dir, mut library, id) = library("Original source.");
    let provider = Provider::new(vec![
        output(vec![
            article("first", "A usable article."),
            article("../invalid", "bad"),
        ]),
        output(vec![
            article("first", "A usable article."),
            article("second", "Another usable article."),
        ]),
    ])
    .await;
    let first = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    assert_eq!(first.status, JobStatus::Partial);
    assert_eq!(first.draft_ids.len(), 1);
    assert!(!first.errors.is_empty());
    assert_eq!(
        library.job(&first.job_id).unwrap().status,
        JobStatus::Partial
    );
    let second = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    assert_eq!(second.status, JobStatus::Succeeded);
    assert_eq!(second.draft_ids.len(), 2);
    assert!(!second.cached);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    for draft in second.draft_ids {
        library.apply_draft(&draft, None).unwrap();
    }
    assert_eq!(library.list(Some("compiled"), true).unwrap().len(), 2);
}

#[tokio::test]
async fn draft_apply_checks_reviewed_target_hash_and_source_revision() {
    let (dir, mut library, id) = library("Original source.");
    fs::create_dir_all(&library.config.concepts).unwrap();
    fs::write(
        library.config.concepts.join("orchard.md"),
        "---\ntitle: Orchard\n---\nOld body",
    )
    .unwrap();
    library.sync().unwrap();
    let provider = Provider::new(vec![output(vec![article(
        "orchard",
        "A substantially longer new article body.",
    )])])
    .await;
    let report = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    let draft = library.draft(&report.draft_ids[0]).unwrap();
    assert!(draft.base_hash.is_some());
    assert!(matches!(
        library.apply_draft(&draft.id, None),
        Err(Error::Conflict(_))
    ));
    fs::write(
        library.config.concepts.join("orchard.md"),
        "Human edit that must survive.",
    )
    .unwrap();
    library.sync().unwrap();
    assert!(matches!(
        library.apply_draft(&draft.id, draft.base_hash.as_deref()),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        fs::read_to_string(library.config.concepts.join("orchard.md")).unwrap(),
        "Human edit that must survive."
    );
    fs::write(dir.path().join("raw/original.md"), "Changed source.").unwrap();
    library.sync().unwrap();
    assert!(matches!(
        library.apply_draft(&draft.id, draft.base_hash.as_deref()),
        Err(Error::Conflict(_))
    ));
}

#[tokio::test]
async fn aliases_unknown_metadata_sources_and_bare_body_override_survive_merge() {
    let (_dir, mut library, id) = library("---\ntitle: 原典\n---\n原典𠮷。");
    fs::create_dir_all(&library.config.concepts).unwrap();
    fs::write(library.config.concepts.join("canonical.md"),"---\ntitle: Existing\naliases: [仁義]\nunknown: {nested: [true, 42]}\ntags: [old]\nsources: [{title: earlier}]\n---\n仁者愛人。").unwrap();
    library.sync().unwrap();
    let mut generated = article("new-slug", "仁者愛人，義者正己，𠮷字存真。");
    generated["title"] = json!("仁義");
    let provider = Provider::new(vec![output(vec![generated])]).await;
    let profile = CompileProfile {
        language: LanguageProfile {
            sections: vec![LanguageSection {
                code: "wenyan".into(),
                key: "文言".into(),
                header: String::new(),
                extra: Default::default(),
            }],
            default_lang: "wenyan".into(),
            api_default_lang: "wenyan".into(),
            views: Vec::new(),
        },
        ..CompileProfile::default()
    };
    let report = library
        .compile_source(&provider.client, &id, &profile, token())
        .await
        .unwrap();
    let draft = library.draft(&report.draft_ids[0]).unwrap();
    assert_eq!(draft.target_path, "canonical.md");
    library
        .apply_draft(&draft.id, draft.base_hash.as_deref())
        .unwrap();
    let article = library.get("new-slug", None, true).unwrap();
    assert_eq!(article.metadata["unknown"]["nested"][1], 42);
    assert_eq!(article.metadata["sources"].as_array().unwrap().len(), 2);
    assert!(
        article.metadata["tags"]
            .as_array()
            .unwrap()
            .contains(&json!("old"))
    );
    assert!(article.content[article.body_start..].contains("𠮷"));
    assert!(!article.content[article.body_start..].starts_with("##"));
    assert!(
        provider.requests.lock().unwrap()[0]["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("wenyan")
    );
}

#[tokio::test]
async fn draft_apply_rejects_raw_file_edit_before_the_index_is_synced() {
    let (dir, mut library, id) = library("Original source.");
    let provider = Provider::new(vec![output(vec![article(
        "fresh",
        "Generated reviewed body.",
    )])])
    .await;
    let report = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    fs::write(
        dir.path().join("raw/original.md"),
        "External unindexed edit.",
    )
    .unwrap();
    assert!(matches!(
        library.apply_draft(&report.draft_ids[0], None),
        Err(Error::Conflict(_))
    ));
    assert!(!library.config.concepts.join("fresh.md").exists());
    assert_eq!(library.draft(&report.draft_ids[0]).unwrap().status, "draft");
}

#[tokio::test]
async fn restrictive_source_visibility_cannot_leak_through_public_merge() {
    let (_dir, mut library, id) = library("---\nvisibility: private\n---\nPrivate source detail.");
    fs::create_dir_all(&library.config.concepts).unwrap();
    fs::write(library.config.concepts.join("orchard.md"), "Public old.").unwrap();
    library.sync().unwrap();
    let provider = Provider::new(vec![output(vec![article(
        "orchard",
        "Private source detail appears in the much longer compiled body.",
    )])])
    .await;
    let report = library
        .compile_source(&provider.client, &id, &CompileProfile::default(), token())
        .await
        .unwrap();
    let draft = library.draft(&report.draft_ids[0]).unwrap();
    library
        .apply_draft(&draft.id, draft.base_hash.as_deref())
        .unwrap();
    assert!(library.list(Some("compiled"), false).unwrap().is_empty());
    assert_eq!(
        library.list(Some("compiled"), true).unwrap()[0].metadata["visibility"],
        "private"
    );
}

#[tokio::test]
async fn unsupported_callbacks_and_foreign_drafts_are_explicit_errors() {
    let (_dir, mut library, id) = library("Source.");
    let provider = Provider::new(vec![]).await;
    let profile = CompileProfile {
        required_capabilities: vec!["python.taxonomy_callback".into()],
        ..CompileProfile::default()
    };
    assert!(matches!(
        library
            .compile_source(&provider.client, &id, &profile, token())
            .await,
        Err(Error::Capability(_))
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert!(library.jobs().unwrap().is_empty());
    assert!(matches!(
        library.draft("foreign-id"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn english_native_default_configured_trilingual_and_exact_legacy_merge_threshold() {
    let english = LanguageProfile::from_config(&json!({})).unwrap();
    assert_eq!(english.default_lang, "en");
    assert!(english.bare_body());
    assert_eq!(
        english.merge("12345", "123456"),
        "12345",
        "exactly 1.2x retains original"
    );
    assert_eq!(english.merge("12345", "1234567"), "1234567");
    assert_eq!(
        english.merge("甲乙丙丁戊", "𠮷甲乙丙丁戊"),
        "甲乙丙丁戊",
        "count Unicode scalars, not UTF-8 bytes"
    );
    let profile=LanguageProfile::from_config(&json!({"languages":{"default":"tri","profiles":{"tri":{"default_lang":"zh-en","api_default_lang":"zh","sections":[{"code":"en","key":"english","header":"## English"},{"code":"zh","key":"中文","header":"## 中文"},{"code":"ja","key":"日本語","header":"## 日本語"}],"views":[{"code":"zh-en","show":["en","zh"]}]}}}})).unwrap();
    assert_eq!(profile.default_lang, "zh-en");
    assert_eq!(profile.sections.len(), 3);
    let merged = profile.merge(
        "Preamble\n## English\n12345\n## 中文\n原文。",
        "## english\n1234567\n## 日本語\n文章。",
    );
    assert!(merged.contains("Preamble"));
    assert!(merged.contains("1234567"));
    assert!(merged.contains("原文。"));
    assert!(merged.contains("文章。"));
}

#[tokio::test]
async fn application_compile_requires_each_capability_and_uses_config_profile() {
    use llmbase_core::application::{Application, Capability, CompileRequest, Principal, Request};
    let (_dir, mut library, id) = library("Original English source.");
    library.config.settings = json!({"languages":{"default":"english-headed","profiles":{"english-headed":{"sections":[{"code":"en","key":"English","header":"## English"}]}}}});
    let app = Application::new(vec![library.config.clone()]).unwrap();
    let provider = Provider::new(vec![output(vec![article(
        "applied-through-service",
        "## English\n\nA reviewed English article.",
    )])])
    .await;
    let make_request = || CompileRequest {
        source_id: id.clone(),
        profile: None,
    };
    for capability in [
        Capability::Read,
        Capability::Write,
        Capability::Model,
        Capability::Private,
    ] {
        let mut principal = Principal::local("compile-test");
        principal.capabilities.remove(&capability);
        assert!(matches!(
            app.compile_source(
                &principal,
                "compile-test",
                provider.client.clone(),
                make_request(),
                token()
            )
            .await,
            Err(Error::Scope)
        ));
    }
    assert!(matches!(
        app.compile_source(
            &Principal::local("other"),
            "compile-test",
            provider.client.clone(),
            make_request(),
            token()
        )
        .await,
        Err(Error::Scope)
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    let principal = Principal::local("compile-test");
    let report = app
        .compile_source(
            &principal,
            "compile-test",
            provider.client.clone(),
            make_request(),
            token(),
        )
        .await
        .unwrap();
    assert_eq!(report.status, JobStatus::Succeeded);
    let prompt = provider.requests.lock().unwrap()[0]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(prompt.contains("## English"));
    let applied = app
        .execute(
            &principal,
            Request {
                library_id: "compile-test".into(),
                operation: "draft_apply".into(),
                args: json!({"draft_id":report.draft_ids[0],"expected_hash":null}),
            },
        )
        .unwrap();
    assert_eq!(applied["status"], "applied");
    assert!(
        library
            .config
            .concepts
            .join("applied-through-service.md")
            .exists()
    );
}
