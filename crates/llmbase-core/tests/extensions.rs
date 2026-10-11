use llmbase_core::{
    Error, Library, LibraryConfig,
    application::{Application, Capability, Principal, Request},
    extensions::{EXTENSION_API_VERSION, Extension, ExtensionSpec, JsonSchema},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "llmbase_core::schemars")]
#[serde(deny_unknown_fields)]
struct Input {
    text: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "llmbase_core::schemars")]
struct Output {
    value: Value,
}

fn spec(name: &str, libraries: &[&str], capabilities: Vec<Capability>) -> ExtensionSpec {
    ExtensionSpec {
        name: name.into(),
        description: "test contract".into(),
        contract_version: EXTENSION_API_VERSION,
        libraries: libraries.iter().map(|s| (*s).into()).collect(),
        required_capabilities: capabilities,
    }
}

fn echo(spec: ExtensionSpec, calls: Arc<AtomicUsize>) -> llmbase_core::Result<Extension> {
    Extension::typed::<Input, Output, _, _>(spec, move |ctx, input| {
        calls.fetch_add(1, Ordering::SeqCst);
        async move {
            Ok(Output {
                value: json!({"text":input.text,"library":ctx.library_id()}),
            })
        }
    })
}

fn request(lib: &str, op: &str, args: Value) -> Request {
    Request {
        library_id: lib.into(),
        operation: op.into(),
        args,
    }
}

fn config() -> (TempDir, LibraryConfig) {
    let dir = TempDir::new().unwrap();
    let cfg = LibraryConfig::load(dir.path(), "a").unwrap();
    (dir, cfg)
}

#[test]
fn registration_rejects_collisions_versions_invalid_policy_and_non_objects() {
    let calls = Arc::new(AtomicUsize::new(0));
    for name in [
        "get",
        "source_write",
        "llmbase.read",
        "core.read",
        "bad..read",
        "Bad.read",
        "bad.1read",
        ".read",
    ] {
        assert!(
            echo(spec(name, &["a"], vec![Capability::Read]), calls.clone()).is_err(),
            "{name}"
        );
    }
    let mut unsupported = spec("example.read", &["a"], vec![Capability::Read]);
    unsupported.contract_version += 1;
    assert!(echo(unsupported, calls.clone()).is_err());
    for libs in [vec![], vec!["a", "a"], vec!["*"]] {
        assert!(
            echo(
                spec("example.read", &libs, vec![Capability::Read]),
                calls.clone()
            )
            .is_err()
        );
    }
    for caps in [vec![], vec![Capability::Read, Capability::Read]] {
        assert!(echo(spec("example.read", &["a"], caps), calls.clone()).is_err());
    }
    assert!(
        Extension::typed::<String, Output, _, _>(
            spec("example.scalar", &["a"], vec![Capability::Read]),
            |_, _| async { Ok(Output { value: json!(null) }) }
        )
        .is_err()
    );
    assert!(
        Extension::typed::<Input, String, _, _>(
            spec("example.scalar", &["a"], vec![Capability::Read]),
            |_, input| async { Ok(input.text) }
        )
        .is_err()
    );
    let (_dir, cfg) = config();
    let extension = echo(
        spec("example.read", &["a"], vec![Capability::Read]),
        calls.clone(),
    )
    .unwrap();
    assert!(matches!(
        Application::with_extensions(vec![cfg.clone()], vec![extension.clone(), extension]),
        Err(Error::Conflict(_))
    ));
    let foreign = echo(spec("example.read", &["b"], vec![Capability::Read]), calls).unwrap();
    assert!(matches!(
        Application::with_extensions(vec![cfg], vec![foreign]),
        Err(Error::Scope)
    ));
}

#[tokio::test]
async fn authorization_and_typed_input_precede_handler_and_storage() {
    let (dir, cfg) = config();
    let calls = Arc::new(AtomicUsize::new(0));
    let extension = echo(
        spec(
            "example.read",
            &["a"],
            vec![Capability::Read, Capability::Private],
        ),
        calls.clone(),
    )
    .unwrap();
    let app = Application::with_extensions(vec![cfg.clone()], vec![extension]).unwrap();
    let req = || request("a", "example.read", json!({"text":"accepted"}));
    assert!(matches!(
        app.execute_extension(&Principal::public("a"), req(), CancellationToken::new())
            .await,
        Err(Error::Scope)
    ));
    for input in [
        json!({"text":1}),
        json!({}),
        json!({"text":"ok","extra":true}),
        json!([]),
    ] {
        assert!(matches!(
            app.execute_extension(
                &Principal::local("a"),
                request("a", "example.read", input),
                CancellationToken::new()
            )
            .await,
            Err(Error::Invalid(_))
        ));
    }
    assert!(
        app.extension_descriptors(&Principal::public("a"), "a")
            .unwrap()
            .is_empty()
    );
    let descriptors = app
        .extension_descriptors(&Principal::local("a"), "a")
        .unwrap();
    assert_eq!(descriptors[0].input_schema["additionalProperties"], false);
    assert_eq!(descriptors[0].output_schema["type"], "object");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!cfg.database().exists());
    assert!(matches!(
        app.execute(&Principal::local("a"), req()),
        Err(Error::Capability(_))
    ));
    assert!(!cfg.database().exists());
    let accepted = app
        .execute_extension(&Principal::local("a"), req(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(accepted["value"]["text"], "accepted");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn two_library_scope_is_explicit_and_discovery_is_sorted() {
    let (_a, cfg_a) = config();
    let b = TempDir::new().unwrap();
    let cfg_b = LibraryConfig::load(b.path(), "b").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let extensions = ["zeta.read", "alpha.read"]
        .into_iter()
        .map(|name| echo(spec(name, &["a"], vec![Capability::Read]), calls.clone()).unwrap())
        .collect();
    let app = Application::with_extensions(vec![cfg_a, cfg_b], extensions).unwrap();
    let mut caller = Principal::local("a");
    caller.libraries.insert("b".into());
    let descriptors = app.extension_descriptors(&caller, "a").unwrap();
    assert_eq!(
        descriptors
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha.read", "zeta.read"]
    );
    assert!(app.extension_descriptors(&caller, "b").unwrap().is_empty());
    assert!(matches!(
        app.execute_extension(
            &caller,
            request("b", "alpha.read", json!({"text":"test"})),
            CancellationToken::new()
        )
        .await,
        Err(Error::Scope)
    ));
    assert!(matches!(
        app.extension_descriptors(&Principal::local("b"), "a"),
        Err(Error::Scope)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn context_narrows_caller_grants_and_cannot_override_selected_library() {
    let (_dir, cfg) = config();
    let mut library = Library::open(cfg.clone()).unwrap();
    library
        .write_source(
            "raw",
            "secret.md",
            b"---\nprivate: true\n---\nsecret".to_vec(),
            None,
        )
        .unwrap();
    drop(library);
    let extension = Extension::typed::<Input, Output, _, _>(
        spec("example.limited", &["a"], vec![Capability::Read]),
        |ctx, _| async move {
            assert!(ctx.execute("get", json!({"slug":"secret"})).await.is_err());
            assert!(matches!(
                ctx.execute(
                    "source_write",
                    json!({"kind":"raw","path":"bad.md","content":"forbidden"})
                )
                .await,
                Err(Error::Scope)
            ));
            assert!(matches!(
                ctx.execute("example.limited", json!({"text":"nested"}))
                    .await,
                Err(Error::Capability(_))
            ));
            let doctor = ctx.execute("doctor", json!({"library_id":"b"})).await?;
            Ok(Output { value: doctor })
        },
    )
    .unwrap();
    let app = Application::with_extensions(vec![cfg.clone()], vec![extension]).unwrap();
    let result = app
        .execute_extension(
            &Principal::local("a"),
            request("a", "example.limited", json!({"text":"ok"})),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["value"]["library_id"], "a");
    assert!(!cfg.raw.join("bad.md").exists());
}

#[tokio::test]
async fn cancellation_reaches_handler_and_does_not_cancel_parent_on_completion() {
    let (_dir, cfg) = config();
    let calls = Arc::new(AtomicUsize::new(0));
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let invoked = calls.clone();
    let extension = Extension::typed::<Input, Output, _, _>(
        spec("example.wait", &["a"], vec![Capability::Read]),
        move |ctx, input| {
            invoked.fetch_add(1, Ordering::SeqCst);
            started_tx.send(ctx.cancellation()).unwrap();
            async move {
                if input.text == "wait" {
                    std::future::pending::<()>().await;
                }
                Ok(Output {
                    value: json!("done"),
                })
            }
        },
    )
    .unwrap();
    let app = Application::with_extensions(vec![cfg], vec![extension]).unwrap();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        app.execute_extension(
            &Principal::local("a"),
            request("a", "example.wait", json!({"text":"wait"})),
            cancelled
        )
        .await,
        Err(Error::Model(llmbase_model::ModelError::Cancelled))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let parent = CancellationToken::new();
    let principal = Principal::local("a");
    let future = app.execute_extension(
        &principal,
        request("a", "example.wait", json!({"text":"wait"})),
        parent.clone(),
    );
    tokio::pin!(future);
    let child = tokio::select! {
        token = started_rx.recv() => token.unwrap(),
        _ = &mut future => panic!("handler unexpectedly finished"),
    };
    parent.cancel();
    assert!(matches!(
        future.await,
        Err(Error::Model(llmbase_model::ModelError::Cancelled))
    ));
    assert!(child.is_cancelled());
    let parent = CancellationToken::new();
    app.execute_extension(
        &Principal::local("a"),
        request("a", "example.wait", json!({"text":"done"})),
        parent.clone(),
    )
    .await
    .unwrap();
    assert!(!parent.is_cancelled());
    assert!(started_rx.recv().await.unwrap().is_cancelled());
}

#[tokio::test]
async fn extension_writes_use_durable_intents_and_recover_after_index_failure() {
    let (_dir, cfg) = config();
    drop(Library::open(cfg.clone()).unwrap());
    std::fs::create_dir_all(&cfg.raw).unwrap();
    std::fs::write(cfg.raw.join("broken.md"), "---\ntitle: [broken\n---\nbody").unwrap();
    let extension = Extension::typed::<Input, Output, _, _>(
        spec(
            "example.write",
            &["a"],
            vec![Capability::Read, Capability::Write, Capability::Private],
        ),
        |ctx, input| async move {
            let result = ctx
                .execute(
                    "source_write",
                    json!({"kind":"raw","path":"accepted.md","content":input.text}),
                )
                .await?;
            Ok(Output { value: result })
        },
    )
    .unwrap();
    let app = Application::with_extensions(vec![cfg.clone()], vec![extension]).unwrap();
    let content = "原文\r\n𠀀 e\u{301}。\n";
    assert!(
        app.execute_extension(
            &Principal::local("a"),
            request("a", "example.write", json!({"text":content})),
            CancellationToken::new()
        )
        .await
        .is_err()
    );
    assert_eq!(
        std::fs::read(cfg.raw.join("accepted.md")).unwrap(),
        content.as_bytes()
    );
    let db = rusqlite::Connection::open(cfg.database()).unwrap();
    let status: String = db
        .query_row("SELECT status FROM write_intents", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "pending");
    std::fs::write(cfg.raw.join("broken.md"), "repaired external fixture").unwrap();
    let recovered = Library::open(cfg.clone()).unwrap();
    let source = recovered.get("accepted", None, true).unwrap();
    assert_eq!(source.content, content);
    assert_eq!(source.revision, 1);
    assert_eq!(
        db.query_row("SELECT status FROM write_intents", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "applied"
    );
    drop(recovered);
    // A caller retry without the accepted revision must not overwrite or duplicate it.
    assert!(matches!(
        app.execute_extension(
            &Principal::local("a"),
            request(
                "a",
                "example.write",
                json!({"text":"unaccepted replacement"})
            ),
            CancellationToken::new()
        )
        .await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        std::fs::read(cfg.raw.join("accepted.md")).unwrap(),
        content.as_bytes()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM write_intents", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn handler_failure_retains_accepted_writes_and_jobs_are_rejected() {
    let (_dir, cfg) = config();
    let extension = Extension::typed::<Input, Output, _, _>(
        spec("example.write", &["a"], vec![Capability::Write]),
        |ctx, input| async move {
            ctx.execute(
                "source_write",
                json!({"kind":"raw","path":"accepted.md","content":input.text}),
            )
            .await?;
            Err(Error::Invalid("handler failed after accepted write".into()))
        },
    )
    .unwrap();
    let app = Application::with_extensions(vec![cfg.clone()], vec![extension]).unwrap();
    assert!(matches!(
        app.execute(
            &Principal::local("a"),
            request(
                "a",
                "job_enqueue",
                json!({"operation":"example.write","args":{"text":"source"}})
            )
        ),
        Err(Error::Capability(_))
    ));
    assert!(!cfg.database().exists());
    assert!(
        app.execute_extension(
            &Principal::local("a"),
            request("a", "example.write", json!({"text":"accepted source"})),
            CancellationToken::new()
        )
        .await
        .is_err()
    );
    let reopened = Library::open(cfg).unwrap();
    assert_eq!(
        reopened.get("accepted", None, false).unwrap().content,
        "accepted source"
    );
}

#[test]
fn invalid_enabled_language_profile_fails_at_registration() {
    let (_dir, mut cfg) = config();
    cfg.settings = json!({"languages":{"default":"missing","profiles":{}}});
    let extension = echo(
        spec("example.read", &["a"], vec![Capability::Read]),
        Arc::new(AtomicUsize::new(0)),
    )
    .unwrap();
    assert!(Application::with_extensions(vec![cfg], vec![extension]).is_err());
}
