use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use llmbase_core::{
    Error, Library, LibraryConfig,
    answers::{AnswerVisibility, SaveAnswerRequest},
    ask::{AskResponse, CitedEvidence, ContextBudget, LabeledEvidence},
    jobs::JobStatus,
    profile::CompileProfile,
};
use llmbase_model::{CancellationToken, ChatResponse, Client, ModelConfig, Usage};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};

struct Provider {
    client: Client,
    calls: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Provider {
    async fn stalled(timeout: Duration) -> (Self, oneshot::Receiver<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "promotion-fixture",
        );
        config.retry.max_attempts = 1;
        config.request_timeout = timeout;
        config.read_timeout = timeout;
        let client = Client::new(config).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let (started, receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await;
            counter.fetch_add(1, Ordering::SeqCst);
            let _ = started.send(());
            std::future::pending::<()>().await;
        });
        (
            Self {
                client,
                calls,
                task,
            },
            receiver,
        )
    }

    async fn new(contents: Vec<(String, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "promotion-fixture",
        );
        config.retry.max_attempts = 1;
        let client = Client::new(config).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let task = tokio::spawn(async move {
            for (content, finish) in contents {
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
                    if let Some(pos) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        end = pos + 4;
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
                let request: Value = serde_json::from_slice(&bytes[end..end + size]).unwrap();
                assert_eq!(request["stream"], false);
                counter.fetch_add(1, Ordering::SeqCst);
                let body=json!({"model":"promotion-fixture","choices":[{"message":{"content":content},"finish_reason":finish}]}).to_string();
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
            task,
        }
    }
}

fn library(name: &str, private: bool) -> (TempDir, Library) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("raw")).unwrap();
    fs::write(dir.path().join("raw/evidence.md"),format!("---\ntitle: Orchard\nvisibility: {}\ncustom: {{keep: 42}}\n---\nOrchards produce fruit. 原典𠮷。",if private{"private"}else{"public"})).unwrap();
    let mut library = Library::open(LibraryConfig::load(dir.path(), name).unwrap()).unwrap();
    library.sync().unwrap();
    (dir, library)
}
fn answer(library: &Library) -> AskResponse {
    let evidence = library
        .search("orchard", 1, Some("raw"), true)
        .unwrap()
        .results[0]
        .evidence
        .clone();
    AskResponse {
        request_id: "fixture-request".into(),
        library_id: library.config.library_id.clone(),
        session_id: Some("fixture-session".into()),
        generation: ChatResponse {
            content: "Orchards produce fruit [E1].".into(),
            model: "generation-fixture".into(),
            finish_reason: Some("stop".into()),
            truncated: false,
            usage: Usage {
                prompt_tokens: Some(12),
                completion_tokens: Some(6),
                total_tokens: Some(18),
                reasoning_tokens: None,
            },
            attempts: 1,
        },
        candidates: vec![evidence.clone()],
        context_evidence: vec![LabeledEvidence {
            label: "E1".into(),
            evidence: evidence.clone(),
        }],
        used_evidence: vec![CitedEvidence {
            label: "E1".into(),
            evidence,
            stale: false,
            deleted: false,
        }],
        unknown_citations: Vec::new(),
        citation_semantics: "model_markers_only_not_entailment".into(),
        retrieval_mode: "lexical".into(),
        degradation: Some("no embedding client configured".into()),
        context_budget: ContextBudget {
            max_evidence_bytes: 8192,
            used_evidence_bytes: 500,
            token_estimate: 500,
            estimate_method: "utf8_bytes_conservative_estimate".into(),
        },
    }
}
fn request(library: &Library) -> SaveAnswerRequest {
    SaveAnswerRequest {
        library_id: library.config.library_id.clone(),
        question: "What do orchards produce?".into(),
        title: Some("Orchard answer".into()),
        visibility: AnswerVisibility::Private,
    }
}
fn promotion(slug: &str) -> String {
    json!({"promote":true,"reason":"A named evidence-backed topic.","article":{"slug":slug,"title":"Orchard knowledge","summary":"Fruit production","tags":["botany"],"content":"Orchards produce fruit, as recorded in the original evidence [E1]."}}).to_string()
}
fn token() -> CancellationToken {
    CancellationToken::new()
}

#[test]
fn explicit_save_retains_provenance_is_private_and_replays_without_duplicates() {
    let (dir, mut library) = library("save", false);
    let raw = fs::read(dir.path().join("raw/evidence.md")).unwrap();
    let response = answer(&library);
    assert!(library.list(Some("note"), true).unwrap().is_empty());
    let saved = library
        .save_answer(&response, &request(&library), true)
        .unwrap();
    assert_eq!(saved.response.generation.usage.total_tokens, Some(18));
    assert_eq!(
        saved.provenance,
        "accepted_submission_not_provider_attestation"
    );
    assert_eq!(saved.evidence_status.len(), 1);
    assert!(!saved.evidence_status[0].stale);
    assert!(library.saved_answer(&saved.id, false).is_err());
    let note = library.get(&saved.note_source_id, None, true).unwrap();
    assert_eq!(note.metadata["visibility"], "private");
    assert_eq!(note.metadata["type"], "generated_answer");
    assert_eq!(
        note.metadata["sources"][0]["source_id"],
        response.used_evidence[0].evidence.source_id
    );
    assert_eq!(note.kind, "note");
    let duplicate = library
        .save_answer(&response, &request(&library), true)
        .unwrap();
    assert_eq!(duplicate.id, saved.id);
    assert_eq!(library.list(Some("note"), true).unwrap().len(), 1);
    assert_eq!(fs::read(dir.path().join("raw/evidence.md")).unwrap(), raw);
    let cfg = library.config.clone();
    drop(library);
    let reopened = Library::open(cfg).unwrap();
    assert_eq!(
        reopened
            .saved_answer(&saved.id, true)
            .unwrap()
            .response
            .generation
            .content,
        response.generation.content
    );
}

#[test]
fn save_rejects_forged_cross_library_and_visibility_downgrade_before_writing() {
    let (_dir, mut private) = library("private", true);
    let response = answer(&private);
    let mut req = request(&private);
    req.visibility = AnswerVisibility::Public;
    assert!(matches!(
        private.save_answer(&response, &req, true),
        Err(Error::Invalid(_))
    ));
    assert!(private.list(Some("note"), true).unwrap().is_empty());
    req.visibility = AnswerVisibility::Private;
    assert!(matches!(
        private.save_answer(&response, &req, false),
        Err(Error::Scope)
    ));
    let (_other_dir, mut other) = library("other", false);
    assert!(matches!(
        other.save_answer(&response, &req, true),
        Err(Error::Scope)
    ));
    let mut forged = response.clone();
    forged.candidates[0].exact_quote = "invented quotation".into();
    assert!(private.save_answer(&forged, &req, true).is_err());
    let mut forged = response;
    forged.used_evidence[0].label = "E99".into();
    assert!(private.save_answer(&forged, &req, true).is_err());
    assert!(private.list(Some("note"), true).unwrap().is_empty());
}

#[test]
fn stale_original_evidence_remains_resolvable_and_note_edits_are_detected() {
    let (dir, mut library) = library("stale", false);
    let response = answer(&library);
    let saved = library
        .save_answer(&response, &request(&library), true)
        .unwrap();
    fs::write(
        dir.path().join("raw/evidence.md"),
        "An updated orchard passage.",
    )
    .unwrap();
    library.sync().unwrap();
    let loaded = library.saved_answer(&saved.id, true).unwrap();
    assert!(loaded.evidence_status[0].stale);
    assert_eq!(loaded.evidence_status[0].evidence, response.candidates[0]);
    let note = library.get(&saved.note_source_id, None, true).unwrap();
    fs::write(
        library.config.outputs.join(note.rel_path),
        "Unindexed user edit.",
    )
    .unwrap();
    assert!(matches!(
        library.saved_answer(&saved.id, true),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn uncited_context_and_historical_private_flag_cannot_be_saved_publicly() {
    let (dir, mut library) = library("historical-private", false);
    fs::write(dir.path().join("raw/evidence.md"), "---\ntitle: Orchard\nvisibility: public\nprivate: true\n---\nOrchards produce fruit. 原典𠮷。").unwrap();
    library.sync().unwrap();
    let mut response = answer(&library);
    response.generation.content = "Orchards produce fruit.".into();
    response.used_evidence.clear();
    let mut accepted = request(&library);
    accepted.visibility = AnswerVisibility::Public;
    assert!(matches!(
        library.save_answer(&response, &accepted, true),
        Err(Error::Invalid(_))
    ));
    fs::write(
        dir.path().join("raw/evidence.md"),
        "---\ntitle: Orchard\nvisibility: public\n---\nA revised public orchard source.",
    )
    .unwrap();
    library.sync().unwrap();
    assert!(matches!(
        library.save_answer(&response, &accepted, true),
        Err(Error::Invalid(_))
    ));
    assert!(library.save_answer(&response, &accepted, false).is_err());
    assert!(library.list(Some("note"), true).unwrap().is_empty());
    accepted.visibility = AnswerVisibility::Private;
    let saved = library.save_answer(&response, &accepted, true).unwrap();
    assert!(saved.evidence_status[0].stale);
}

#[test]
fn save_rejects_unindexed_source_privacy_edit_before_note_creation() {
    let (dir, mut library) = library("unindexed-private", false);
    let response = answer(&library);
    let mut accepted = request(&library);
    accepted.visibility = AnswerVisibility::Public;
    fs::write(
        dir.path().join("raw/evidence.md"),
        "---\nprivate: true\n---\nOrchards produce confidential fruit.",
    )
    .unwrap();
    assert!(matches!(
        library.save_answer(&response, &accepted, true),
        Err(Error::Conflict(_))
    ));
    assert!(library.list(Some("note"), true).unwrap().is_empty());
}

#[tokio::test]
async fn promotion_creates_reviewed_draft_with_original_provenance_and_explicit_apply() {
    let (_dir, mut library) = library("promote", false);
    let response = answer(&library);
    let saved = library
        .save_answer(&response, &request(&library), true)
        .unwrap();
    let provider = Provider::new(vec![(promotion("orchard-concept"), "stop")]).await;
    let report = library
        .promote_answer(
            &provider.client,
            &saved.id,
            &CompileProfile::default(),
            token(),
        )
        .await
        .unwrap();
    assert!(report.promote);
    let draft_id = report.draft_id.unwrap();
    assert!(library.list(Some("compiled"), true).unwrap().is_empty());
    let draft = library.draft(&draft_id).unwrap();
    assert_eq!(draft.source_id, saved.note_source_id);
    let applied = library
        .apply_draft(&draft.id, draft.base_hash.as_deref())
        .unwrap();
    assert_eq!(applied.status, "applied");
    let concept = library.get("orchard-concept", None, true).unwrap();
    assert_eq!(concept.metadata["visibility"], "private");
    assert_eq!(
        concept.metadata["sources"][0]["native_answer"]["response"]["used_evidence"][0]["evidence"]
            ["source_id"],
        response.candidates[0].source_id
    );
    let again = library
        .promote_answer(
            &provider.client,
            &saved.id,
            &CompileProfile::default(),
            token(),
        )
        .await
        .unwrap();
    assert!(again.cached);
    assert_eq!(again.draft_id, Some(draft_id));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        library.job(&report.job_id).unwrap().status,
        JobStatus::Succeeded
    );
}

#[tokio::test]
async fn decline_is_explicit_and_invalid_truncated_or_path_outputs_never_create_a_draft() {
    let (_dir, mut library) = library("decline", false);
    let saved = library
        .save_answer(&answer(&library), &request(&library), true)
        .unwrap();
    let provider = Provider::new(vec![(
        json!({"promote":false,"reason":"No standalone topic."}).to_string(),
        "stop",
    )])
    .await;
    let declined = library
        .promote_answer(
            &provider.client,
            &saved.id,
            &CompileProfile::default(),
            token(),
        )
        .await
        .unwrap();
    assert!(!declined.promote);
    assert!(declined.draft_id.is_none());
    assert_eq!(declined.reason, "No standalone topic.");
    for (content, finish) in [
        ("invalid JSON".into(), "stop"),
        (promotion("../escape"), "stop"),
        (promotion("safe"), "length"),
    ] {
        let (_dir, mut library) = super_library();
        let saved = library
            .save_answer(&answer(&library), &request(&library), true)
            .unwrap();
        let provider = Provider::new(vec![(content, finish)]).await;
        assert!(
            library
                .promote_answer(
                    &provider.client,
                    &saved.id,
                    &CompileProfile::default(),
                    token()
                )
                .await
                .is_err()
        );
        assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Failed);
        assert!(library.list(Some("compiled"), true).unwrap().is_empty());
    }
}
fn super_library() -> (TempDir, Library) {
    library("invalid-promotion", false)
}

#[tokio::test]
async fn promotion_draft_rejects_a_changed_saved_note_and_truncated_saved_answer() {
    let (_dir, mut library) = library("guarded-promotion", false);
    let saved = library
        .save_answer(&answer(&library), &request(&library), true)
        .unwrap();
    let provider = Provider::new(vec![(promotion("safe"), "stop")]).await;
    let report = library
        .promote_answer(
            &provider.client,
            &saved.id,
            &CompileProfile::default(),
            token(),
        )
        .await
        .unwrap();
    let draft = library.draft(report.draft_id.as_ref().unwrap()).unwrap();
    let note = library.get(&saved.note_source_id, None, true).unwrap();
    fs::write(
        library.config.outputs.join(note.rel_path),
        "A changed accepted answer.",
    )
    .unwrap();
    assert!(matches!(
        library.apply_draft(&draft.id, draft.base_hash.as_deref()),
        Err(Error::Conflict(_))
    ));
    assert!(!library.config.concepts.join("safe.md").exists());
    let (_dir, mut other) = super_library();
    let mut partial = answer(&other);
    partial.generation.truncated = true;
    partial.generation.finish_reason = Some("length".into());
    let saved = other.save_answer(&partial, &request(&other), true).unwrap();
    let provider = Provider::new(vec![]).await;
    assert!(matches!(
        other
            .promote_answer(
                &provider.client,
                &saved.id,
                &CompileProfile::default(),
                token()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn promotion_preserves_stale_raw_provenance_and_current_restrictions() {
    let (dir, mut library) = library("stale-promotion", false);
    let response = answer(&library);
    let mut accepted = request(&library);
    accepted.visibility = AnswerVisibility::Public;
    let saved = library.save_answer(&response, &accepted, true).unwrap();
    fs::write(
        dir.path().join("raw/evidence.md"),
        "---\ntitle: Orchard\nprivate: true\n---\nOrchards now have a restricted revision.",
    )
    .unwrap();
    library.sync().unwrap();
    let provider = Provider::new(vec![(promotion("restricted-orchard"), "stop")]).await;
    let report = library
        .promote_answer(
            &provider.client,
            &saved.id,
            &CompileProfile::default(),
            token(),
        )
        .await
        .unwrap();
    let draft = library.draft(report.draft_id.as_ref().unwrap()).unwrap();
    library
        .apply_draft(&draft.id, draft.base_hash.as_deref())
        .unwrap();
    let article = library.get("restricted-orchard", None, true).unwrap();
    assert_eq!(article.metadata["visibility"], "private");
    let support = &article.metadata["sources"][0];
    assert_eq!(support["promotion_evidence_status"][0]["stale"], true);
    assert_eq!(
        support["promotion_evidence_status"][0]["evidence"]["exact_quote"],
        response.candidates[0].exact_quote
    );
    assert_eq!(
        support["promotion_generation"]["model"],
        "promotion-fixture"
    );
    assert_eq!(support["promotion_generation"]["truncated"], false);
}

#[tokio::test]
async fn cancelled_and_timed_out_promotions_persist_terminal_job_states() {
    let (_dir, mut library) = library("cancel-promotion", false);
    let saved = library
        .save_answer(&answer(&library), &request(&library), true)
        .unwrap();
    let (provider, started) = Provider::stalled(Duration::from_secs(2)).await;
    let cancel = token();
    let trigger = cancel.clone();
    let cancel_task = tokio::spawn(async move {
        started.await.unwrap();
        trigger.cancel();
    });
    let result = library
        .promote_answer(
            &provider.client,
            &saved.id,
            &CompileProfile::default(),
            cancel,
        )
        .await;
    cancel_task.await.unwrap();
    assert!(matches!(
        result,
        Err(Error::Model(llmbase_model::ModelError::Cancelled))
    ));
    assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Cancelled);
    let (provider, _) = Provider::stalled(Duration::from_millis(30)).await;
    assert!(matches!(
        library
            .promote_answer(
                &provider.client,
                &saved.id,
                &CompileProfile::default(),
                token()
            )
            .await,
        Err(Error::Model(llmbase_model::ModelError::Timeout))
    ));
    let jobs = library.jobs().unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().any(|job| job.status == JobStatus::Failed));
    assert!(library.list(Some("compiled"), true).unwrap().is_empty());
}

#[tokio::test]
async fn promotion_apply_rejects_privacy_changes_after_review_draft_creation() {
    for indexed in [false, true] {
        let (dir, mut library) = library("guarded-raw-promotion", false);
        let mut accepted = request(&library);
        accepted.visibility = AnswerVisibility::Public;
        let saved = library
            .save_answer(&answer(&library), &accepted, true)
            .unwrap();
        let provider = Provider::new(vec![(promotion("must-not-publish"), "stop")]).await;
        let report = library
            .promote_answer(
                &provider.client,
                &saved.id,
                &CompileProfile::default(),
                token(),
            )
            .await
            .unwrap();
        let draft = library.draft(report.draft_id.as_ref().unwrap()).unwrap();
        fs::write(
            dir.path().join("raw/evidence.md"),
            "---\nprivate: true\n---\nOrchards produce restricted fruit.",
        )
        .unwrap();
        if indexed {
            library.sync().unwrap();
        }
        assert!(matches!(
            library.apply_draft(&draft.id, draft.base_hash.as_deref()),
            Err(Error::Conflict(_))
        ));
        assert!(!library.config.concepts.join("must-not-publish.md").exists());
        assert_eq!(library.draft(&draft.id).unwrap().status, "draft");
    }
}

#[tokio::test]
async fn incomplete_promotion_guard_receipt_cannot_apply_or_replay_as_cached_success() {
    let (_dir, mut library) = library("missing-guards", false);
    let saved = library
        .save_answer(&answer(&library), &request(&library), true)
        .unwrap();
    let provider = Provider::new(vec![(promotion("guarded"), "stop")]).await;
    let report = library
        .promote_answer(
            &provider.client,
            &saved.id,
            &CompileProfile::default(),
            token(),
        )
        .await
        .unwrap();
    let draft = library.draft(report.draft_id.as_ref().unwrap()).unwrap();
    let db = rusqlite::Connection::open(library.config.meta.join("llmbase.sqlite3")).unwrap();
    db.execute(
        "DELETE FROM compile_draft_guards WHERE draft_id=?1",
        [&draft.id],
    )
    .unwrap();
    assert!(matches!(
        library.apply_draft(&draft.id, draft.base_hash.as_deref()),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        library
            .promote_answer(
                &provider.client,
                &saved.id,
                &CompileProfile::default(),
                token()
            )
            .await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(!library.config.concepts.join("guarded.md").exists());
}
