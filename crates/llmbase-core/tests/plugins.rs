#![cfg(unix)]
use llmbase_core::{
    Error, Library, LibraryConfig,
    application::{Application, Principal},
    jobs::JobStatus,
    plugins::AdapterRequest,
};
use llmbase_model::CancellationToken;
use serde_json::{Value, json};
use tempfile::TempDir;

fn setup(script: &str) -> (TempDir, Application, LibraryConfig) {
    let root = TempDir::new().unwrap();
    let mut config = LibraryConfig::load(root.path(), "plugins").unwrap();
    config.settings = json!({"plugins":{"process_adapters":{"fixture":{"command":["/bin/sh","-c",script],"timeout_seconds":2,"max_output_bytes":4096}}}});
    let app = Application::new(vec![config.clone()]).unwrap();
    (root, app, config)
}
fn script(reply: Value) -> String {
    let output = reply.to_string();
    assert!(!output.contains(char::from(39)));
    format!("cat >/dev/null\nprintf '%s' '{output}'")
}
fn request() -> AdapterRequest {
    AdapterRequest {
        adapter: "fixture".into(),
        input: json!({"text":"untrusted $(touch should-not-exist); echo injection","library_id":"another-library"}),
    }
}

#[tokio::test]
async fn configured_process_has_real_protocol_and_native_revision_receipts() {
    let body = "---\ntitle: 文言原典\nunknown: {retained: 42}\n---\n學而時習之。\n";
    let (root, app, config) = setup(&script(
        json!({"protocol_version":1,"documents":[{"path":"classic/source.md","content":body}],"assets":[{"path":"classic/source.original","content_bytes":[0,255,128,13,10]}]}),
    ));
    let value = app
        .ingest_with_adapter(
            &Principal::local("plugins"),
            "plugins",
            request(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(value["status"], "succeeded");
    assert_eq!(value["accepted"].as_array().unwrap().len(), 1);
    assert_eq!(value["original_assets"].as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(root.path().join("raw/classic/source.original")).unwrap(),
        vec![0, 255, 128, 13, 10]
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("raw/classic/source.md")).unwrap(),
        body
    );
    assert!(!root.path().join("should-not-exist").exists());
    let library = Library::open(config.clone()).unwrap();
    let source = library.get("source", None, true).unwrap();
    assert_eq!(source.metadata["unknown"]["retained"], 42);
    assert_eq!(
        library
            .job(value["job_id"].as_str().unwrap())
            .unwrap()
            .status,
        JobStatus::Succeeded
    );
    drop(library);
    let repeated = app
        .ingest_with_adapter(
            &Principal::local("plugins"),
            "plugins",
            request(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        repeated["accepted"][0]["index_report"]["passages_written"],
        0
    );
    assert_eq!(
        Library::open(config)
            .unwrap()
            .get("source", None, true)
            .unwrap()
            .revision,
        1
    );
}

#[tokio::test]
async fn scope_denial_precedes_process_and_traversal_rejects_entire_reply() {
    let (root, app, config) = setup(&script(
        json!({"protocol_version":1,"documents":[{"path":"safe.md","content":"safe"},{"path":"../escape.md","content":"unsafe"}]}),
    ));
    assert!(matches!(
        app.ingest_with_adapter(
            &Principal::public("plugins"),
            "plugins",
            request(),
            CancellationToken::new()
        )
        .await,
        Err(Error::Scope)
    ));
    assert!(!config.database().exists());
    assert!(matches!(
        app.ingest_with_adapter(
            &Principal::local("plugins"),
            "plugins",
            request(),
            CancellationToken::new()
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(!root.path().join("raw/safe.md").exists());
    let library = Library::open(config).unwrap();
    assert_eq!(library.jobs().unwrap()[0].status, JobStatus::Failed);
}

#[tokio::test]
async fn process_failure_does_not_echo_stderr_and_cancellation_kills_child() {
    let (_root, app, config) = setup("cat >/dev/null\nprintf 'fixture-secret' >&2\nexit 7");
    let error = app
        .ingest_with_adapter(
            &Principal::local("plugins"),
            "plugins",
            request(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("fixture-secret"));
    assert!(
        !Library::open(config).unwrap().jobs().unwrap()[0]
            .error
            .as_ref()
            .unwrap()
            .contains("fixture-secret")
    );
    let (_root, app, config) = setup("cat >/dev/null\nexec /bin/sleep 20");
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();
    });
    let started = std::time::Instant::now();
    assert!(
        app.ingest_with_adapter(&Principal::local("plugins"), "plugins", request(), token)
            .await
            .is_err()
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(
        Library::open(config).unwrap().jobs().unwrap()[0].status,
        JobStatus::Cancelled
    );
}

#[tokio::test]
async fn nonzero_exit_oversize_and_unconfigured_adapter_cannot_succeed() {
    let (_root, app, config) =
        setup("cat >/dev/null\nwhile :; do printf '012345678901234567890123456789'; done");
    assert!(
        app.ingest_with_adapter(
            &Principal::local("plugins"),
            "plugins",
            request(),
            CancellationToken::new()
        )
        .await
        .is_err()
    );
    assert_eq!(
        Library::open(config).unwrap().jobs().unwrap()[0].status,
        JobStatus::Failed
    );
    let mut missing = request();
    missing.adapter = "unknown".into();
    assert!(matches!(
        app.ingest_with_adapter(
            &Principal::local("plugins"),
            "plugins",
            missing,
            CancellationToken::new()
        )
        .await,
        Err(Error::Capability(_))
    ));
}
