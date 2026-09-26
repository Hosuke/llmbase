//! Deterministic HTTP embeddings prove engine mechanics, not semantic quality.
use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use llmbase_core::{Error, Library, LibraryConfig, vector::EmbeddingSpec};
use llmbase_model::{CancellationToken, Client, ModelConfig, ModelError};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Notify, mpsc},
    task::JoinHandle,
};

struct Provider {
    client: Client,
    calls: Arc<AtomicUsize>,
    inputs: Arc<Mutex<Vec<Vec<String>>>>,
    fail_on: Arc<AtomicUsize>,
    gate_on: Arc<AtomicUsize>,
    gate: Arc<Notify>,
    requests: mpsc::UnboundedReceiver<usize>,
    task: JoinHandle<()>,
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Provider {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = ModelConfig::new(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            "vectors",
        );
        config.retry.max_attempts = 1;
        config.request_timeout = Duration::from_secs(3);
        let client = Client::new(config).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let fail_on = Arc::new(AtomicUsize::new(usize::MAX));
        let gate_on = Arc::new(AtomicUsize::new(usize::MAX));
        let gate = Arc::new(Notify::new());
        let (sender, requests) = mpsc::unbounded_channel();
        let (c, i, f, g, n) = (
            calls.clone(),
            inputs.clone(),
            fail_on.clone(),
            gate_on.clone(),
            gate.clone(),
        );
        let task = tokio::spawn(async move {
            loop {
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
                    assert!(bytes.len() < 100_000);
                }
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                assert!(headers.starts_with("POST /v1/embeddings "));
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
                let body: Value = serde_json::from_slice(&bytes[end..end + size]).unwrap();
                let texts = body["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>();
                i.lock().unwrap().push(texts.clone());
                let call = c.fetch_add(1, Ordering::SeqCst) + 1;
                sender.send(call).unwrap();
                if g.load(Ordering::SeqCst) == call {
                    n.notified().await;
                }
                let failed = f.load(Ordering::SeqCst) == call;
                let status = if failed { 503 } else { 200 };
                let dimension = body["dimensions"].as_u64().unwrap() as usize;
                let data = texts
                    .iter()
                    .enumerate()
                    .map(|(index, text)| {
                        let mut vector = vec![0.0; dimension];
                        let semantic = text.contains("orchard") || text == "fruit";
                        vector[if semantic || dimension == 1 { 0 } else { 1 }] = 1.0;
                        json!({"index":index,"embedding":vector})
                    })
                    .collect::<Vec<_>>();
                let response = if failed { json!({"error":{"message":"fixture unavailable"}}) } else { json!({"model":body["model"],"data":data,"usage":{"prompt_tokens":texts.len(),"total_tokens":texts.len()}}) }.to_string();
                let wire = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                );
                let _ = socket.write_all(wire.as_bytes()).await;
            }
        });
        Self {
            client,
            calls,
            inputs,
            fail_on,
            gate_on,
            gate,
            requests,
            task,
        }
    }

    fn spec(&self) -> EmbeddingSpec {
        EmbeddingSpec::new(&self.client, "vectors", "fixture-r1", 2)
    }
    fn count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    async fn entered(&mut self, wanted: usize) {
        while tokio::time::timeout(Duration::from_secs(1), self.requests.recv())
            .await
            .unwrap()
            .unwrap()
            < wanted
        {}
    }
}

fn token() -> CancellationToken {
    CancellationToken::new()
}
fn library(name: &str) -> (TempDir, Library) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("raw")).unwrap();
    let lib = Library::open(LibraryConfig::load(dir.path(), name).unwrap()).unwrap();
    (dir, lib)
}
fn write(dir: &TempDir, name: &str, text: &str) {
    fs::write(dir.path().join("raw").join(format!("{name}.md")), text).unwrap();
}
fn active(lib: &Library) -> Option<String> {
    use rusqlite::OptionalExtension;
    rusqlite::Connection::open(lib.config.database())
        .unwrap()
        .query_row(
            "SELECT generation_id FROM vector_active WHERE library_id=?1",
            [&lib.config.library_id],
            |r| r.get(0),
        )
        .optional()
        .unwrap()
}

#[tokio::test]
async fn idempotent_persisted_cache_and_incremental_change() {
    let provider = Provider::new().await;
    let (dir, mut lib) = library("cache");
    write(&dir, "a", "orchard flowers");
    write(&dir, "b", "history parchment");
    lib.sync().unwrap();
    let first = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    assert_eq!(first.embedded, 2);
    assert_eq!(provider.count(), 1);
    let cfg = lib.config.clone();
    drop(lib);
    let mut lib = Library::open(cfg).unwrap();
    let second = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    assert!(second.reused_active);
    assert_eq!(second.generation_id, first.generation_id);
    assert_eq!(second.cache_hits, 2);
    assert_eq!(provider.count(), 1);
    write(&dir, "b", "changed parchment");
    lib.sync().unwrap();
    let third = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    assert_eq!(third.embedded, 1);
    assert_eq!(third.cache_hits, 1);
    assert_ne!(third.generation_id, first.generation_id);
    assert_eq!(
        provider.inputs.lock().unwrap()[1],
        vec!["changed parchment"]
    );
}

#[tokio::test]
async fn full_model_spec_separates_generations_and_dimensions() {
    let provider = Provider::new().await;
    let (dir, mut lib) = library("spec");
    write(&dir, "a", "same input");
    lib.sync().unwrap();
    let a = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    let mut spec = provider.spec();
    spec.model = "another-model".into();
    spec.dimension = 3;
    let b = lib
        .build_vectors(&provider.client, &spec, token())
        .await
        .unwrap();
    assert_ne!(a.generation_id, b.generation_id);
    assert_eq!(b.embedded, 1);
    assert_eq!(provider.count(), 2);
    spec.revision = "fixture-r2".into();
    let c = lib
        .build_vectors(&provider.client, &spec, token())
        .await
        .unwrap();
    assert_ne!(b.generation_id, c.generation_id);
    assert_eq!(c.embedded, 1);
    let reused = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    assert_eq!(reused.generation_id, a.generation_id);
    assert_eq!(reused.embedded, 0);
    assert_eq!(active(&lib), Some(a.generation_id));
}

#[tokio::test]
async fn failed_partial_build_retains_active_and_resumes_successful_batches() {
    let provider = Provider::new().await;
    let (dir, mut lib) = library("resume");
    for n in 0..40 {
        write(&dir, &format!("p{n}"), &format!("document number {n}"));
    }
    lib.sync().unwrap();
    let original = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    let mut next = provider.spec();
    next.revision = "next".into();
    provider
        .fail_on
        .store(provider.count() + 2, Ordering::SeqCst);
    assert!(matches!(
        lib.build_vectors(&provider.client, &next, token()).await,
        Err(Error::Model(ModelError::Http { status: 503, .. }))
    ));
    assert_eq!(active(&lib), Some(original.generation_id));
    let resumed = lib
        .build_vectors(&provider.client, &next, token())
        .await
        .unwrap();
    assert_eq!(resumed.cache_hits, 32);
    assert_eq!(resumed.embedded, 8);
    assert_eq!(active(&lib), Some(resumed.generation_id));
}

#[tokio::test]
async fn cancellation_leaves_active_generation_and_does_not_publish_partial_vectors() {
    let mut provider = Provider::new().await;
    let (dir, mut lib) = library("cancel");
    write(&dir, "a", "orchard");
    lib.sync().unwrap();
    let original = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    let mut next = provider.spec();
    next.revision = "next".into();
    let call = provider.count() + 1;
    provider.gate_on.store(call, Ordering::SeqCst);
    let cancel = token();
    let client = provider.client.clone();
    let task_cancel = cancel.clone();
    let (result, ()) = tokio::join!(lib.build_vectors(&client, &next, task_cancel), async {
        provider.entered(call).await;
        cancel.cancel();
    });
    assert!(matches!(result, Err(Error::Model(ModelError::Cancelled))));
    assert_eq!(active(&lib), Some(original.generation_id));
}

#[tokio::test]
async fn concurrent_source_change_prevents_activation_and_stale_search_degrades() {
    let mut provider = Provider::new().await;
    let (dir, mut lib) = library("concurrent");
    write(&dir, "a", "orchard");
    lib.sync().unwrap();
    let original = lib
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    let mut next = provider.spec();
    next.revision = "next".into();
    let mut writer = Library::open(lib.config.clone()).unwrap();
    let call = provider.count() + 1;
    provider.gate_on.store(call, Ordering::SeqCst);
    let client = provider.client.clone();
    let (result, ()) = tokio::join!(lib.build_vectors(&client, &next, token()), async {
        provider.entered(call).await;
        write(&dir, "a", "replacement orchard");
        writer.sync().unwrap();
        provider.gate.notify_one();
    });
    assert!(matches!(result, Err(Error::Conflict(_))));
    assert_eq!(active(&lib), Some(original.generation_id));
    let before = provider.count();
    let search = lib
        .search_hybrid("orchard", 10, None, false, Some(&provider.client), token())
        .await
        .unwrap();
    assert_eq!(search.mode, "lexical");
    assert!(search.degradation.unwrap().contains("stale"));
    assert_eq!(provider.count(), before);
    assert!(
        search.results[0]
            .evidence
            .exact_quote
            .contains("replacement")
    );
}

#[tokio::test]
async fn hybrid_uses_semantic_candidates_and_enforces_scope_visibility_and_deletion() {
    let provider = Provider::new().await;
    let (dir, mut lib) = library("first");
    write(&dir, "literal", "fruit is a word in a glossary");
    write(&dir, "semantic", "orchard flowers");
    write(
        &dir,
        "private",
        "---\nvisibility: private\n---\norchard secret",
    );
    write(
        &dir,
        "internal",
        "---\nvisibility: internal\n---\norchard internal",
    );
    lib.sync().unwrap();
    lib.build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    let lexical = lib.search("fruit", 10, None, false).unwrap();
    assert_eq!(lexical.results.len(), 1);
    let hybrid = lib
        .search_hybrid("fruit", 10, None, false, Some(&provider.client), token())
        .await
        .unwrap();
    assert_eq!(hybrid.mode, "hybrid");
    assert_eq!(hybrid.degradation, None);
    assert_eq!(hybrid.results.len(), 2);
    assert!(hybrid.results.iter().any(|h| h.slug == "semantic"));
    assert!(
        hybrid
            .results
            .iter()
            .all(|h| h.evidence.library_id == "first")
    );
    assert!(
        hybrid
            .results
            .iter()
            .all(|h| !h.evidence.exact_quote.contains("secret")
                && !h.evidence.exact_quote.contains("internal"))
    );
    for hit in &hybrid.results {
        lib.resolve_evidence(&hit.evidence, false).unwrap();
    }
    let calls = provider.count();
    let full = lib
        .search_hybrid("fruit", 10, None, true, Some(&provider.client), token())
        .await
        .unwrap();
    assert_eq!(full.results.len(), 4);
    assert_eq!(
        provider.count(),
        calls,
        "query vector cache should be reused"
    );

    let (other_dir, mut other) = library("second");
    write(&other_dir, "semantic", "orchard flowers");
    other.sync().unwrap();
    let built = other
        .build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    assert_eq!(built.embedded, 1, "cache must not leak across libraries");
    let scoped = other
        .search_hybrid("fruit", 10, None, false, Some(&provider.client), token())
        .await
        .unwrap();
    assert_eq!(scoped.results.len(), 1);
    assert_eq!(scoped.results[0].evidence.library_id, "second");

    fs::rename(
        dir.path().join("raw/semantic.md"),
        dir.path().join("deleted-fixture.md"),
    )
    .unwrap();
    lib.sync().unwrap();
    let stale = lib
        .search_hybrid("fruit", 10, None, false, Some(&provider.client), token())
        .await
        .unwrap();
    assert_eq!(stale.mode, "lexical");
    assert!(stale.results.iter().all(|h| h.slug != "semantic"));
    lib.build_vectors(&provider.client, &provider.spec(), token())
        .await
        .unwrap();
    let rebuilt = lib
        .search_hybrid("fruit", 10, None, false, Some(&provider.client), token())
        .await
        .unwrap();
    assert_eq!(rebuilt.mode, "hybrid");
    assert!(rebuilt.results.iter().all(|h| h.slug != "semantic"));
}

#[tokio::test]
async fn missing_vectors_degrade_and_cancelled_or_mismatched_requests_never_call_provider() {
    let provider = Provider::new().await;
    let (dir, mut lib) = library("boundaries");
    write(&dir, "a", "fruit");
    lib.sync().unwrap();
    let no_client = lib
        .search_hybrid("fruit", 1, None, false, None, token())
        .await
        .unwrap();
    assert_eq!(no_client.mode, "lexical");
    assert!(no_client.degradation.is_some());
    let no_vectors = lib
        .search_hybrid("fruit", 1, None, false, Some(&provider.client), token())
        .await
        .unwrap();
    assert_eq!(no_vectors.mode, "lexical");
    let cancel = token();
    cancel.cancel();
    assert!(matches!(
        lib.build_vectors(&provider.client, &provider.spec(), cancel)
            .await,
        Err(Error::Model(ModelError::Cancelled))
    ));
    let mut wrong = provider.spec();
    wrong.provider = "https://different.invalid/v1".into();
    assert!(matches!(
        lib.build_vectors(&provider.client, &wrong, token()).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(provider.count(), 0);
}
