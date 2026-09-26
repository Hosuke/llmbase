use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use llmbase_core::{
    Error, Library, LibraryConfig,
    ask::{
        AskAccess, AskEvent, AskEventKind, AskKind, AskModels, AskRequest, AskResponse, AskStream,
        collect_ask, start_ask,
    },
};
use llmbase_model::{CancellationToken, Client, ModelConfig, ModelError};
use serde_json::Value;
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Notify, mpsc},
    task::JoinHandle,
};

struct Provider {
    client: Client,
    gate: Arc<Notify>,
    disconnected: Arc<AtomicBool>,
    requests: mpsc::UnboundedReceiver<Value>,
    task: JoinHandle<()>,
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Provider {
    async fn start(first: &str, rest: &str, gated: bool, timeout: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "fixture-generation",
        );
        config.request_timeout = timeout;
        config.retry.max_attempts = 1;
        let client = Client::new(config).unwrap();
        let gate = Arc::new(Notify::new());
        let disconnected = Arc::new(AtomicBool::new(false));
        let (sender, requests) = mpsc::unbounded_channel();
        let (g, d) = (gate.clone(), disconnected.clone());
        let (first, rest) = (first.as_bytes().to_vec(), rest.as_bytes().to_vec());
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
                if let Some(position) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    end = position + 4;
                    break;
                }
            }
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            assert!(headers.starts_with("POST /v1/chat/completions "));
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
                let count = socket.read(&mut buffer).await.unwrap();
                if count == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            sender
                .send(serde_json::from_slice(&bytes[end..end + size]).unwrap())
                .unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                first.len() + rest.len()
            );
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(&first).await.unwrap();
            if gated {
                let mut probe = [0u8; 1];
                tokio::select! {
                    _ = g.notified() => {},
                    result = socket.read(&mut probe) => {
                        d.store(matches!(result, Ok(0) | Err(_)),Ordering::SeqCst);
                        return;
                    }
                }
            }
            let _ = socket.write_all(&rest).await;
        });
        Self {
            client,
            gate,
            disconnected,
            requests,
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

fn delta(text: &str) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"choices":[{"index":0,"delta":{"content":text}}]})
    )
}
fn finish(reason: &str) -> String {
    format!(
        "data: {}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":4,\"total_tokens\":14}}}}\n\ndata: [DONE]\n\n",
        serde_json::json!({"choices":[{"index":0,"delta":{},"finish_reason":reason}]})
    )
}
fn token() -> CancellationToken {
    CancellationToken::new()
}

/// Captures real native retrieval/stream/citation results for an independently
/// reviewed, fixed semantic audit. Provider answers are scripted adversarial
/// fixtures, never represented as paid-provider generation quality.
#[tokio::test]
async fn capture_fixed_grounding_acceptance_cases() {
    let cases = [
        (
            "supported",
            vec!["Orchard record: the gate opens at 10:00. 原典𠮷。"],
            "When does the orchard gate open?",
            "The orchard record says the gate opens at 10:00 [E1].",
            None,
            "supported",
            "The claim reports exactly the opening time stated in the cited record.",
        ),
        (
            "unsupported",
            vec!["Orchard record: apple trees flower in spring. 原典𠮷。"],
            "How old is the orchard?",
            "The orchard is 500 years old [E1].",
            None,
            "unsupported",
            "The quote exists, but flowering season provides no orchard age. A valid marker does not support the 500-year inference.",
        ),
        (
            "conflicting_sources_acknowledged",
            vec![
                "Orchard notice A: the gate opens at 10:00.",
                "Orchard notice B: the gate opens at 12:00.",
            ],
            "When does the orchard gate open?",
            "The orchard notices disagree: one gives 10:00, the other 12:00 [E1][E2].",
            None,
            "supported_disagreement",
            "Both recorded times exist. Reporting disagreement is supported; choosing a definitive current time would require additional evidence.",
        ),
        (
            "conflicting_sources_erased",
            vec![
                "Orchard notice A: the gate opens at 10:00.",
                "Orchard notice B: the gate opens at 12:00.",
            ],
            "When does the orchard gate open?",
            "Both orchard notices agree that the gate opens at 10:00 [E1][E2].",
            None,
            "contradicted",
            "The second notice gives 12:00, contradicting the claim of agreement even though both citation labels resolve.",
        ),
        (
            "stale_current_claim",
            vec!["Orchard schedule revision one: the gate opens at 10:00. 原典𠮷。"],
            "When does the orchard gate currently open?",
            "The current orchard gate opens at 10:00 [E1].",
            Some("Orchard schedule revision two: the gate opens at 12:00. 原典𠮷。"),
            "historical_support_only_current_claim_contradicted",
            "The original quote supports revision one's time. It does not support a current-time claim after revision two changes the time to 12:00. The native response correctly marks the cited evidence stale.",
        ),
    ];
    let mut records = Vec::new();
    for (name, sources, question, answer, revision_two, judgment, reason) in cases {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("raw")).unwrap();
        let inputs: Vec<_> = sources
            .iter()
            .enumerate()
            .map(|(index, content)| {
                let relative = format!("source-{index}.md");
                fs::write(dir.path().join("raw").join(&relative), content).unwrap();
                serde_json::json!({"relative_path":relative,"original_content":content})
            })
            .collect();
        let config = LibraryConfig::load(dir.path(), format!("audit-{name}")).unwrap();
        let mut library = Library::open(config.clone()).unwrap();
        library.sync().unwrap();
        let mut provider = Provider::start(
            &delta(answer),
            &finish("stop"),
            revision_two.is_some(),
            Duration::from_secs(2),
        )
        .await;
        let mut stream = start_ask(
            config,
            provider.models(),
            AskAccess::default(),
            AskRequest {
                question: question.into(),
                library_id: format!("audit-{name}"),
                request_id: Some(format!("audit-{name}")),
                kind: AskKind::Raw,
                ..Default::default()
            },
            token(),
        )
        .unwrap();
        let mut events = Vec::new();
        let mut changed = false;
        let mut initial_evidence = None;
        let response = loop {
            let event = next(&mut stream).await;
            events.push(serde_json::to_value(&event).unwrap());
            match event.event {
                AskEventKind::Evidence {
                    context_evidence, ..
                } => {
                    initial_evidence = context_evidence.first().map(|item| item.evidence.clone());
                }
                AskEventKind::Delta { .. } if !changed => {
                    let Some(updated) = revision_two else {
                        continue;
                    };
                    let evidence = initial_evidence.as_ref().unwrap();
                    let source = library.get(&evidence.source_id, None, false).unwrap();
                    library
                        .write_source(
                            "raw",
                            &source.rel_path,
                            updated.as_bytes().to_vec(),
                            Some(&source.content_hash),
                        )
                        .unwrap();
                    changed = true;
                    provider.gate.notify_one();
                }
                AskEventKind::Done { response } => break *response,
                AskEventKind::Error { code, message, .. } => {
                    panic!("audit {name}: {code}: {message}")
                }
                _ => {}
            }
        };
        assert_eq!(response.generation.content, answer);
        assert!(!response.used_evidence.is_empty());
        assert!(response.unknown_citations.is_empty());
        assert_eq!(
            response.citation_semantics,
            "model_markers_only_not_entailment"
        );
        let quote_checks:Vec<_>=response.used_evidence.iter().map(|cited|{let resolved=library.resolve_evidence(&cited.evidence,false).unwrap();assert_eq!(resolved.stale,revision_two.is_some());serde_json::json!({"label":cited.label,"quote_resolves_at_recorded_revision":true,"stale":resolved.stale,"deleted":resolved.deleted,"current_revision":resolved.current_revision})}).collect();
        records.push(serde_json::json!({"case_id":name,"source_inputs":inputs,"source_revision_two":revision_two,"question":question,"scripted_provider_output":answer,"actual_provider_request":provider.requests.recv().await.unwrap(),"actual_native_events":events,"actual_native_response":response,"quote_checks":quote_checks,"manual_source_review":{"entailment_judgment":judgment,"reason":reason,"automated_entailment_verification":false}}));
    }
    if let Ok(path) = std::env::var("LLMBASE_GROUNDING_AUDIT_PATH") {
        let artifact = serde_json::json!({"schema_version":1,"provenance":{"runtime":"actual Rust retrieval, HTTP SSE, and evidence resolution","generation":"fixed scripted loopback HTTP responses","paid_provider_requests":0,"provider_quality_evaluation":false,"review":"manual source comparison by migration audit agent"},"cases":records});
        fs::write(path, serde_json::to_vec_pretty(&artifact).unwrap()).unwrap();
    }
}

fn library() -> (TempDir, Library) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("raw")).unwrap();
    fs::write(
        dir.path().join("raw/a.md"),
        "---\ntitle: Orchard history\n---\nAn orchard grows apples. 原典𠮷 remains unchanged.",
    )
    .unwrap();
    fs::write(dir.path().join("raw/b.md"), "Orchard trees have flowers.").unwrap();
    fs::write(
        dir.path().join("raw/private.md"),
        "---\nvisibility: private\n---\nOrchard secret private detail.",
    )
    .unwrap();
    let mut library = Library::open(LibraryConfig::load(dir.path(), "ask-test").unwrap()).unwrap();
    library.sync().unwrap();
    (dir, library)
}

fn request() -> AskRequest {
    AskRequest {
        question: "orchard".into(),
        library_id: "ask-test".into(),
        request_id: Some("request-test".into()),
        session_id: Some("session-test".into()),
        ..AskRequest::default()
    }
}

async fn next(stream: &mut AskStream) -> AskEvent {
    tokio::time::timeout(Duration::from_secs(2), stream.recv())
        .await
        .unwrap()
        .unwrap()
}

async fn done(stream: &mut AskStream) -> AskResponse {
    loop {
        match next(stream).await.event {
            AskEventKind::Done { response } => return *response,
            AskEventKind::Error { code, message, .. } => {
                panic!("unexpected ask error {code}: {message}")
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn evidence_precedes_real_delta_and_citation_markers_are_not_claimed_as_entailment() {
    let (dir, library) = library();
    let before = fs::read(dir.path().join("raw/a.md")).unwrap();
    let mut provider = Provider::start(
        &delta("First claim [E1]. "),
        &(delta("Unknown [E99].") + &finish("stop")),
        true,
        Duration::from_secs(2),
    )
    .await;
    let mut stream = start_ask(
        library.config.clone(),
        provider.models(),
        AskAccess::default(),
        request(),
        token(),
    )
    .unwrap();
    let first = next(&mut stream).await;
    assert_eq!(first.version, 1);
    assert_eq!(first.sequence, 0);
    assert!(matches!(first.event, AskEventKind::Progress { .. }));
    let evidence = next(&mut stream).await;
    let AskEventKind::Evidence {
        candidates,
        context_evidence,
        context_budget,
        ..
    } = evidence.event
    else {
        panic!("evidence missing")
    };
    assert_eq!(candidates.len(), 2);
    assert_eq!(context_evidence.len(), 2);
    assert!(context_budget.used_evidence_bytes <= context_budget.max_evidence_bytes);
    assert!(
        !serde_json::to_string(&candidates)
            .unwrap()
            .contains("secret")
    );
    assert!(matches!(
        next(&mut stream).await.event,
        AskEventKind::Progress { .. }
    ));
    let delta = next(&mut stream).await;
    assert_eq!(delta.sequence, 3);
    assert!(matches!(delta.event, AskEventKind::Delta { .. }));
    // Provider cannot send finish until the consumer observes its first delta.
    provider.gate.notify_one();
    let response = done(&mut stream).await;
    assert_eq!(
        response.generation.content,
        "First claim [E1]. Unknown [E99]."
    );
    assert_eq!(response.generation.usage.total_tokens, Some(14));
    assert_eq!(response.used_evidence.len(), 1);
    assert_eq!(response.used_evidence[0].label, "E1");
    assert_eq!(response.unknown_citations, vec!["E99"]);
    assert_eq!(
        response.citation_semantics,
        "model_markers_only_not_entailment"
    );
    assert_eq!(response.session_id.as_deref(), Some("session-test"));
    library
        .resolve_evidence(&response.used_evidence[0].evidence, false)
        .unwrap();
    assert_eq!(fs::read(dir.path().join("raw/a.md")).unwrap(), before);
    assert!(library.list(Some("compiled"), true).unwrap().is_empty());
    assert!(library.list(Some("note"), true).unwrap().is_empty());
    let request = provider.requests.recv().await.unwrap();
    assert_eq!(request["stream"], true);
    assert!(
        request["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("code: en")
    );
    assert!(stream.recv().await.is_none());
}

#[tokio::test]
async fn citations_resolve_original_revision_and_mark_concurrent_edit_stale() {
    let (dir, library) = library();
    let provider = Provider::start(
        &delta("Claim [E1]."),
        &finish("stop"),
        true,
        Duration::from_secs(2),
    )
    .await;
    let mut stream = start_ask(
        library.config.clone(),
        provider.models(),
        AskAccess::default(),
        request(),
        token(),
    )
    .unwrap();
    let mut first_evidence = None;
    loop {
        match next(&mut stream).await.event {
            AskEventKind::Evidence {
                context_evidence, ..
            } => first_evidence = Some(context_evidence[0].evidence.clone()),
            AskEventKind::Delta { .. } => break,
            _ => {}
        }
    }
    let original = first_evidence.unwrap();
    let mut writer = Library::open(library.config.clone()).unwrap();
    let source = writer.get(&original.source_id, None, false).unwrap();
    fs::write(
        dir.path().join("raw").join(source.rel_path),
        "An orchard changed after retrieval.",
    )
    .unwrap();
    writer.sync().unwrap();
    provider.gate.notify_one();
    let response = done(&mut stream).await;
    assert!(response.used_evidence[0].stale);
    assert!(!response.used_evidence[0].deleted);
    assert_eq!(response.used_evidence[0].evidence, original);
    writer.resolve_evidence(&original, false).unwrap();
}

#[tokio::test]
async fn cancellation_is_terminal_and_drop_disconnects_provider() {
    for drop_stream in [false, true] {
        let (_dir, library) = library();
        let provider = Provider::start(
            &delta("Partial answer"),
            &finish("stop"),
            true,
            Duration::from_secs(2),
        )
        .await;
        let parent = token();
        let mut stream = start_ask(
            library.config.clone(),
            provider.models(),
            AskAccess::default(),
            request(),
            parent.clone(),
        )
        .unwrap();
        loop {
            if matches!(next(&mut stream).await.event, AskEventKind::Delta { .. }) {
                break;
            }
        }
        if drop_stream {
            drop(stream);
        } else {
            stream.cancel();
            let AskEventKind::Error { code, partial, .. } = next(&mut stream).await.event else {
                panic!("missing cancellation")
            };
            assert_eq!(code, "cancelled");
            assert!(partial);
            assert!(stream.recv().await.is_none());
        }
        tokio::time::timeout(Duration::from_secs(1), async {
            while !provider.disconnected.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!parent.is_cancelled());
    }
}

#[tokio::test]
async fn timeout_and_midstream_failure_never_emit_done() {
    for timeout in [true, false] {
        let (_dir, library) = library();
        let rest = if timeout {
            finish("stop")
        } else {
            "data: invalid JSON\n\n".into()
        };
        let provider =
            Provider::start(&delta("Partial"), &rest, timeout, Duration::from_millis(60)).await;
        let mut stream = start_ask(
            library.config.clone(),
            provider.models(),
            AskAccess::default(),
            request(),
            token(),
        )
        .unwrap();
        let mut saw_partial = false;
        loop {
            match next(&mut stream).await.event {
                AskEventKind::Delta { .. } => saw_partial = true,
                AskEventKind::Error { code, partial, .. } => {
                    assert_eq!(code, if timeout { "timeout" } else { "model_error" });
                    assert!(partial && saw_partial);
                    break;
                }
                AskEventKind::Done { .. } => panic!("partial failure cannot finish successfully"),
                _ => {}
            }
        }
        assert!(stream.recv().await.is_none());
    }
}

#[tokio::test]
async fn collected_ask_preserves_length_finish_and_language_override() {
    let (_dir, library) = library();
    let mut provider = Provider::start(
        &delta("原典𠮷[E1]"),
        &finish("length"),
        false,
        Duration::from_secs(2),
    )
    .await;
    let mut req = request();
    req.answer_language = Some("wenyan".into());
    req.short_answer = true;
    req.max_sentences = Some(2);
    let response = collect_ask(
        library.config.clone(),
        provider.models(),
        AskAccess::default(),
        req,
        token(),
    )
    .await
    .unwrap();
    assert!(response.generation.truncated);
    assert_eq!(response.generation.finish_reason.as_deref(), Some("length"));
    let wire = provider.requests.recv().await.unwrap();
    assert!(
        wire["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("code: wenyan")
    );
    assert!(
        wire["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("at most 2 sentences")
    );
}

#[tokio::test]
async fn no_evidence_budget_and_untrusted_access_fields_are_rejected_without_generation() {
    let (_dir, library) = library();
    let provider = Provider::start(
        &delta("should not run"),
        &finish("stop"),
        false,
        Duration::from_secs(2),
    )
    .await;
    let mut req = request();
    req.context_token_budget = 2;
    assert!(matches!(
        collect_ask(
            library.config.clone(),
            provider.models(),
            AskAccess::default(),
            req,
            token()
        )
        .await,
        Err(Error::NotFound(_))
    ));
    let malicious =
        serde_json::json!({"question":"q","library_id":"ask-test","include_private":true});
    assert!(serde_json::from_value::<AskRequest>(malicious).is_err());
    let mut req = request();
    req.library_id = "another-library".into();
    assert!(matches!(
        start_ask(
            library.config.clone(),
            provider.models(),
            AskAccess::default(),
            req,
            token()
        ),
        Err(Error::Scope)
    ));
    let mut req = request();
    req.kind = AskKind::Compiled;
    assert!(matches!(
        collect_ask(
            library.config.clone(),
            provider.models(),
            AskAccess::default(),
            req,
            token()
        )
        .await,
        Err(Error::NotFound(_))
    ));
    let cancel = token();
    cancel.cancel();
    assert!(matches!(
        collect_ask(
            library.config.clone(),
            provider.models(),
            AskAccess::default(),
            request(),
            cancel
        )
        .await,
        Err(Error::Model(ModelError::Cancelled))
    ));
}
