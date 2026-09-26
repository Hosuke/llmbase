use llmbase_core::{
    Error, Library, LibraryConfig,
    application::{Application, Capability, Principal},
    ask::{AskAccess, AskEventKind, AskModels},
    transcription::{
        TranscriptionInputKind, TranscriptionRequest, TranscriptionWritePolicy,
        collect_transcription_ask, prepare_transcription, start_transcription_ask,
    },
};
use llmbase_model::{CancellationToken, Client, ModelConfig, ModelError};
use serde_json::{Value, json};
use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
    task::JoinHandle,
};

const FIXTURE: &str =
    include_str!("../../../tests/rust_migration/fixtures/downstream/transcription.json");
const CONFIG: &str =
    include_str!("../../../tests/rust_migration/fixtures/downstream/library/config.yaml");
const ORIGINAL: &[u8] = include_bytes!(
    "../../../tests/rust_migration/fixtures/downstream/library/originals/lunyu/index.md"
);
const COMPILED: &[u8] = include_bytes!(
    "../../../tests/rust_migration/fixtures/downstream/library/knowledge/entries/xue.md"
);

fn request() -> TranscriptionRequest {
    let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("expected_question");
    object.remove("expected_evidence");
    serde_json::from_value(value).unwrap()
}

fn fixture() -> (TempDir, LibraryConfig) {
    let directory = TempDir::new().unwrap();
    fs::write(directory.path().join("config.yaml"), CONFIG).unwrap();
    fs::create_dir_all(directory.path().join("originals/lunyu")).unwrap();
    fs::create_dir_all(directory.path().join("knowledge/entries")).unwrap();
    fs::write(directory.path().join("originals/lunyu/index.md"), ORIGINAL).unwrap();
    fs::write(directory.path().join("knowledge/entries/xue.md"), COMPILED).unwrap();
    let config = LibraryConfig::load(directory.path(), "siwen-fixture").unwrap();
    (directory, config)
}

fn indexed_fixture() -> (TempDir, LibraryConfig) {
    let (directory, config) = fixture();
    Library::open(config.clone()).unwrap().sync().unwrap();
    (directory, config)
}

fn delta(text: &str) -> String {
    format!(
        "data: {}\n\n",
        json!({"choices":[{"index":0,"delta":{"content":text}}]})
    )
}

struct Provider {
    client: Client,
    calls: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    gate: Arc<Notify>,
    disconnected: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Provider {
    async fn start(gated: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "transcription-fixture",
        );
        config.request_timeout = Duration::from_secs(3);
        config.retry.max_attempts = 1;
        let client = Client::new(config).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(Notify::new());
        let disconnected = Arc::new(AtomicBool::new(false));
        let (c, r, g, d) = (
            calls.clone(),
            requests.clone(),
            gate.clone(),
            disconnected.clone(),
        );
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let end;
            loop {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                if count == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(position) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    end = position + 4;
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&bytes[..end]);
            assert!(headers.starts_with("POST /v1/chat/completions "));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < end + length {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                if count == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            r.lock()
                .unwrap()
                .push(serde_json::from_slice(&bytes[end..end + length]).unwrap());
            c.fetch_add(1, Ordering::SeqCst);
            let first = delta("出自《論語》");
            let rest = format!(
                "{}data: {{\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":30,\"completion_tokens\":10,\"total_tokens\":40}}}}\n\ndata: [DONE]\n\n",
                delta("〈學而第一〉[E1]。")
            );
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                first.len() + rest.len()
            );
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(first.as_bytes()).await.unwrap();
            if gated {
                let mut probe = [0; 1];
                tokio::select! {
                    _ = g.notified() => {},
                    result = socket.read(&mut probe) => {
                        d.store(matches!(result, Ok(0) | Err(_)), Ordering::SeqCst);
                        return;
                    }
                }
            }
            let _ = socket.write_all(rest.as_bytes()).await;
        });
        Self {
            client,
            calls,
            requests,
            gate,
            disconnected,
            task,
        }
    }

    fn models(&self) -> AskModels {
        AskModels {
            generation: self.client.clone(),
            embedding: None,
        }
    }
}

#[test]
fn preparation_matches_frozen_mapping_preserves_text_and_creates_no_library() {
    let (_directory, config) = fixture();
    let request = request();
    let prepared = prepare_transcription(&config.library_id, &request).unwrap();
    let expected: Value = serde_json::from_str(FIXTURE).unwrap();
    let mapped = serde_json::to_value(&prepared.question).unwrap();
    for (key, value) in expected["expected_question"].as_object().unwrap() {
        if key != "write_policy" {
            assert_eq!(&mapped[key], value);
        }
    }
    assert_eq!(
        prepared.transcription.write_policy,
        TranscriptionWritePolicy::None
    );
    assert!(!prepared.transcription.ocr_performed);
    assert_eq!(
        prepared.transcription.text_sha256,
        llmbase_core::document::hash(request.text.as_ref().unwrap().as_bytes())
    );
    assert!(!config.database().exists());
    let mut original_spacing = request.clone();
    original_spacing.text = Some("  原典𠮷 e\u{301}。\n".into());
    let preserved = prepare_transcription(&config.library_id, &original_spacing).unwrap();
    assert_eq!(preserved.question.question, original_spacing.text.unwrap());
    assert!(preserved.transcription.utf8_bytes > preserved.transcription.unicode_scalars);
}

#[tokio::test]
async fn frozen_transcription_reaches_real_stream_scoped_citations_and_no_saved_note() {
    let (_directory, config) = indexed_fixture();
    fs::write(
        config.raw.join("private.md"),
        "---\nprivate: true\n---\n學而時習之 private-only-secret",
    )
    .unwrap();
    Library::open(config.clone()).unwrap().sync().unwrap();
    let app = Application::new(vec![config.clone()]).unwrap();
    let mut principal = Principal::public(&config.library_id);
    principal.capabilities.insert(Capability::Model);
    let provider = Provider::start(true).await;
    let mut stream = app
        .start_transcription_ask(
            &principal,
            provider.models(),
            request(),
            CancellationToken::new(),
        )
        .unwrap();
    assert!(!stream.transcription().ocr_performed);
    let expected: Value = serde_json::from_str(FIXTURE).unwrap();
    let request_id = stream.request_id().to_owned();
    let mut sequence = 0;
    let mut delta_seen = false;
    let mut done_seen = false;
    let mut events = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(2), stream.recv())
        .await
        .unwrap()
    {
        events.push(serde_json::to_value(&event).unwrap());
        assert_eq!(event.transcription_id, expected["transcription_id"]);
        assert_eq!(event.ask.request_id, request_id);
        assert_eq!(
            event.ask.session_id.as_deref(),
            expected["session_id"].as_str()
        );
        assert_eq!(event.ask.version, 1);
        assert_eq!(event.ask.sequence, sequence);
        sequence += 1;
        match event.ask.event {
            AskEventKind::Delta { content } if !delta_seen => {
                assert_eq!(content, "出自《論語》");
                assert!(!done_seen);
                delta_seen = true;
                provider.gate.notify_one();
            }
            AskEventKind::Done { .. } => {
                assert!(delta_seen);
                done_seen = true;
            }
            AskEventKind::Error { message, .. } => panic!("{message}"),
            _ => {}
        }
    }
    assert!(done_seen);
    let response = stream.collect().await.unwrap();
    assert_eq!(
        response.answer.generation.content,
        "出自《論語》〈學而第一〉[E1]。"
    );
    assert_eq!(
        response.answer.citation_semantics,
        "model_markers_only_not_entailment"
    );
    assert!(response.answer.unknown_citations.is_empty());
    let library = Library::open(config.clone()).unwrap();
    assert!(library.list(Some("note"), true).unwrap().is_empty());
    assert_eq!(response.answer.used_evidence.len(), 1);
    let cited = &response.answer.used_evidence[0].evidence;
    assert_eq!(cited.library_id, config.library_id);
    assert_eq!(
        cited.source_url.as_deref(),
        expected["expected_evidence"]["source_url"].as_str()
    );
    assert!(
        cited.exact_quote.contains(
            expected["expected_evidence"]["exact_quote"]
                .as_str()
                .unwrap()
        )
    );
    let resolved = library.resolve_evidence(cited, false).unwrap();
    assert!(!resolved.stale && !resolved.deleted);
    let source = library
        .get(&cited.source_id, Some(cited.revision), false)
        .unwrap();
    assert_eq!(
        config.raw.join(&source.rel_path),
        config.base.join(
            expected["expected_evidence"]["source_relative_path"]
                .as_str()
                .unwrap()
        )
    );
    assert_eq!(
        source.metadata["chapter"],
        expected["expected_evidence"]["chapter"]
    );
    assert_eq!(
        &source.content.as_bytes()[cited.start..cited.end],
        cited.exact_quote.as_bytes()
    );
    assert_eq!(
        fs::read(config.raw.join("lunyu/index.md")).unwrap(),
        ORIGINAL
    );
    assert_eq!(fs::read(config.concepts.join("xue.md")).unwrap(), COMPILED);
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let prompt = requests[0].to_string();
    assert!(prompt.contains("學而時習之，出於何篇？") && prompt.contains("at most 2 sentences"));
    assert!(prompt.contains("English is the default"));
    assert!(!prompt.contains("private-only-secret") && !prompt.contains("Tom Marvolo Riddle"));
    if let Ok(path) = std::env::var("LLMBASE_TRANSCRIPTION_AUDIT_PATH") {
        let artifact = json!({
            "schema_version":1,
            "fixture_sha256":llmbase_core::document::hash(FIXTURE.as_bytes()),
            "provenance":{"execution":"actual native transcription preparation, ask retrieval, HTTP SSE and evidence resolution","generation":"scripted loopback HTTP fixture","ocr_performed":false,"real_provider_quality_evaluated":false,"device_repository_modified":false},
            "input":request(),
            "prepared":app.prepare_transcription(&principal, request()).unwrap(),
            "actual_provider_requests":*requests,
            "actual_events":events,
            "actual_response":response,
            "checks":{"fixed_question_mapping":true,"original_source_bytes_unchanged":true,"original_compiled_bytes_unchanged":true,"private_context_excluded":true,"exact_evidence_resolves":true,"no_saved_note":true,"delta_before_done":true}
        });
        fs::write(path, serde_json::to_vec_pretty(&artifact).unwrap()).unwrap();
    }
}

#[tokio::test]
async fn collected_transcription_uses_same_real_ask_and_receipt() {
    let (_directory, config) = indexed_fixture();
    let provider = Provider::start(false).await;
    let mut request = request();
    request.input_kind = TranscriptionInputKind::SuppliedTranscription;
    request.answer_preferences.answer_language = Some("wenyan".into());
    let response = collect_transcription_ask(
        config,
        provider.models(),
        AskAccess::default(),
        request,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        response.transcription.input_kind,
        TranscriptionInputKind::SuppliedTranscription
    );
    assert_eq!(
        response.answer.session_id.as_deref(),
        Some("fixture-device-session-001")
    );
    assert!(
        provider.requests.lock().unwrap()[0]
            .to_string()
            .contains("profile code: wenyan")
    );
}

#[tokio::test]
async fn scope_capability_unsupported_images_and_invalid_inputs_fail_before_http_or_indexing() {
    let (_directory, config) = fixture();
    let app = Application::new(vec![config.clone()]).unwrap();
    let public = Principal::public(&config.library_id);
    let local = Principal::local(&config.library_id);
    let provider = Provider::start(false).await;
    assert!(app.prepare_transcription(&public, request()).is_ok());
    assert!(matches!(
        app.start_transcription_ask(
            &public,
            provider.models(),
            request(),
            CancellationToken::new()
        ),
        Err(Error::Scope)
    ));
    let mut no_read = local.clone();
    no_read.capabilities.remove(&Capability::Read);
    assert!(matches!(
        app.prepare_transcription(&no_read, request()),
        Err(Error::Scope)
    ));
    let wrong_library = Principal::local("another-library");
    assert!(matches!(
        app.start_transcription_ask(
            &wrong_library,
            provider.models(),
            request(),
            CancellationToken::new()
        ),
        Err(Error::Scope)
    ));
    for kind in [
        TranscriptionInputKind::Image,
        TranscriptionInputKind::Handwriting,
    ] {
        let mut image = request();
        image.input_kind = kind;
        image.text = None;
        image.image_base64 = Some("synthetic-image-bytes".into());
        assert!(matches!(
            app.start_transcription_ask(&local, provider.models(), image, CancellationToken::new()),
            Err(Error::Capability(_))
        ));
    }
    for field in [
        "api_key",
        "include_private",
        "write_policy",
        "path",
        "image_url",
    ] {
        let mut invalid = serde_json::to_value(request()).unwrap();
        invalid[field] = json!("untrusted");
        assert!(serde_json::from_value::<TranscriptionRequest>(invalid).is_err());
    }
    for mode in 0..5 {
        let mut invalid = request();
        match mode {
            0 => invalid.version = 2,
            1 => invalid.text = Some(" \n".into()),
            2 => invalid.text = Some("𠮷".repeat(4097)),
            3 => invalid.answer_preferences.max_sentences = Some(0),
            _ => invalid.session_id = "../other".into(),
        }
        assert!(matches!(
            app.start_transcription_ask(
                &local,
                provider.models(),
                invalid,
                CancellationToken::new()
            ),
            Err(Error::Invalid(_))
        ));
    }
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        start_transcription_ask(
            config.clone(),
            provider.models(),
            AskAccess::default(),
            request(),
            cancel
        ),
        Err(Error::Model(ModelError::Cancelled))
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert!(!config.database().exists());
}

#[tokio::test]
async fn cancelling_or_dropping_device_stream_disconnects_the_actual_provider() {
    for drop_stream in [false, true] {
        let (_directory, config) = indexed_fixture();
        let provider = Provider::start(true).await;
        let mut stream = start_transcription_ask(
            config,
            provider.models(),
            AskAccess::default(),
            request(),
            CancellationToken::new(),
        )
        .unwrap();
        loop {
            let event = tokio::time::timeout(Duration::from_secs(2), stream.recv())
                .await
                .unwrap()
                .unwrap();
            if matches!(event.ask.event, AskEventKind::Delta { .. }) {
                break;
            }
        }
        if drop_stream {
            drop(stream);
        } else {
            stream.cancel();
            while let Some(event) = tokio::time::timeout(Duration::from_secs(2), stream.recv())
                .await
                .unwrap()
            {
                assert!(!matches!(event.ask.event, AskEventKind::Done { .. }));
            }
            assert!(matches!(
                stream.collect().await,
                Err(Error::Model(ModelError::Cancelled))
            ));
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while !provider.disconnected.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
