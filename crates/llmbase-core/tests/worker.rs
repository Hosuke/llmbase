use llmbase_core::{
    Error, Library, LibraryConfig,
    application::{Application, Capability, Principal, Request},
    jobs::JobStatus,
    store::WriterSession,
    worker::WorkerModels,
};
use llmbase_model::CancellationToken;
use serde_json::{Value, json};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tempfile::TempDir;

fn setup() -> (TempDir, Application, Principal, LibraryConfig) {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("raw")).unwrap();
    fs::write(
        dir.path().join("raw/orchard.md"),
        "---\ntitle: Orchard\n---\n學而時習之。 Orchards grow fruit.\n",
    )
    .unwrap();
    let config = LibraryConfig::load(dir.path(), "worker-test").unwrap();
    let app = Application::new(vec![config.clone()]).unwrap();
    (dir, app, Principal::local("worker-test"), config)
}

fn enqueue(app: &Application, principal: &Principal, operation: &str, args: Value) -> Value {
    app.execute(
        principal,
        Request {
            library_id: "worker-test".into(),
            operation: "job_enqueue".into(),
            args: json!({"operation":operation,"args":args}),
        },
    )
    .unwrap()
}

#[tokio::test]
async fn worker_runs_real_index_and_persists_result_and_events() {
    let (_dir, app, principal, config) = setup();
    let queued = enqueue(&app, &principal, "index", json!({}));
    let job = app
        .run_next_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            "worker-a",
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.id, queued["id"]);
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.result.as_ref().unwrap()["added"], 1);
    let library = Library::open(config).unwrap();
    assert_eq!(library.job(&job.id).unwrap().status, JobStatus::Succeeded);
    assert_eq!(
        library
            .search("Orchard", 10, None, true)
            .unwrap()
            .results
            .len(),
        1
    );
    let events = library.job_events(&job.id).unwrap();
    assert_eq!(
        events
            .iter()
            .map(|v| v["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["queued", "start", "ok"]
    );
    assert!(
        app.run_next_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            "worker-a",
            CancellationToken::new()
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn targeted_worker_claims_only_requested_job_and_does_not_fallback() {
    let (_dir, app, principal, config) = setup();
    let untouched = enqueue(&app, &principal, "index", json!({}));
    let selected = enqueue(&app, &principal, "index", json!({}));
    let id = selected["id"].as_str().unwrap().to_owned();
    let job = app
        .run_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            llmbase_core::worker::WorkerSelection {
                worker_id: "targeted".into(),
                job_id: Some(id.clone()),
            },
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.id, id);
    let again = app
        .run_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            llmbase_core::worker::WorkerSelection {
                worker_id: "targeted".into(),
                job_id: Some(id),
            },
            CancellationToken::new(),
        )
        .await;
    assert!(matches!(again, Err(Error::Conflict(_))));
    assert_eq!(
        Library::open(config)
            .unwrap()
            .job(untouched["id"].as_str().unwrap())
            .unwrap()
            .status,
        JobStatus::Queued
    );
}

#[tokio::test]
async fn unauthorized_or_pre_cancelled_worker_does_not_claim() {
    let (_dir, app, principal, config) = setup();
    let queued = enqueue(&app, &principal, "index", json!({}));
    assert!(matches!(
        app.run_next_job(
            &Principal::public("worker-test"),
            "worker-test",
            WorkerModels::default(),
            "worker",
            CancellationToken::new()
        )
        .await,
        Err(Error::Scope)
    ));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        app.run_next_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            "worker",
            cancel
        )
        .await
        .is_err()
    );
    let library = Library::open(config).unwrap();
    let job = library.job(queued["id"].as_str().unwrap()).unwrap();
    assert_eq!(job.status, JobStatus::Queued);
    assert_eq!(job.attempts, 0);
}

#[tokio::test]
async fn errors_are_persisted_without_success_or_secret_echo() {
    let (_dir, app, principal, _config) = setup();
    enqueue(
        &app,
        &principal,
        "source_write",
        json!({"kind":"raw","path":"../outside","content":"hidden fixture payload"}),
    );
    let failed = app
        .run_next_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            "worker",
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert!(failed.result.is_none());
    assert!(!failed.error.unwrap().contains("hidden fixture"));
    enqueue(&app, &principal, "compile", json!({"source_id":"missing"}));
    let failed = app
        .run_next_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            "worker",
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(
        failed.error.as_deref(),
        Some("required operation capability is unavailable")
    );
}

#[tokio::test]
async fn worker_rechecks_operation_capability_and_skips_cancelled_jobs() {
    let (_dir, app, principal, config) = setup();
    let cancelled = enqueue(&app, &principal, "index", json!({}));
    Library::open(config.clone())
        .unwrap()
        .cancel_job(cancelled["id"].as_str().unwrap())
        .unwrap();
    enqueue(&app, &principal, "ask", json!({"question":"orchard"}));
    let mut worker = principal.clone();
    worker.capabilities.remove(&Capability::Model);
    let failed = app
        .run_next_job(
            &worker,
            "worker-test",
            WorkerModels::default(),
            "worker",
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.operation, "ask");
    assert_eq!(
        failed.error.as_deref(),
        Some("library or capability denied")
    );
    assert_eq!(
        Library::open(config)
            .unwrap()
            .job(cancelled["id"].as_str().unwrap())
            .unwrap()
            .attempts,
        0
    );
}

#[test]
fn inline_job_is_never_visible_to_a_general_queue_claim() {
    let (_dir, _app, _principal, config) = setup();
    let mut inline = Library::open(config.clone()).unwrap();
    let lease = inline
        .enqueue_running_job("compile", json!({"source_id":"a"}), 3, "inline", 60)
        .unwrap();
    let mut other = Library::open(config).unwrap();
    assert!(other.claim_job("other", 60).unwrap().is_none());
    assert_eq!(lease.job.status, JobStatus::Running);
    assert_eq!(lease.job.attempts, 1);
    inline
        .finish_job(&lease, JobStatus::Succeeded, Some(json!({"ok":true})), None)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_workers_cannot_execute_the_same_claim() {
    let (_dir, app, principal, config) = setup();
    let queued = enqueue(&app, &principal, "index", json!({}));
    let (first, second) = tokio::join!(
        app.run_next_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            "a",
            CancellationToken::new()
        ),
        app.run_next_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            "b",
            CancellationToken::new()
        ),
    );
    let mut completed = 0;
    for result in [first, second] {
        match result {
            Ok(Some(job)) => {
                assert_eq!(job.status, JobStatus::Succeeded);
                completed += 1;
            }
            Ok(None) | Err(Error::Busy) => {}
            Err(error) => panic!("unexpected worker failure: {error}"),
        }
    }
    assert_eq!(completed, 1);
    let library = Library::open(config).unwrap();
    assert_eq!(
        library
            .job(queued["id"].as_str().unwrap())
            .unwrap()
            .attempts,
        1
    );
}

/// Barrier plumbing for the post-claim/pre-operation open window: the hook
/// fires once when the claimed operation is about to open its library handle,
/// letting the test hold the recovery writer lock at exactly that moment.
struct OpenBarrier {
    fired: Arc<AtomicBool>,
    open_pending: mpsc::Receiver<()>,
    lock_held: mpsc::Sender<()>,
}
fn install_open_barrier(app: &mut Application) -> OpenBarrier {
    let fired = Arc::new(AtomicBool::new(false));
    let (open_tx, open_rx) = mpsc::channel::<()>();
    let (held_tx, held_rx) = mpsc::channel::<()>();
    let held_rx = Arc::new(Mutex::new(held_rx));
    let hook = {
        let fired = fired.clone();
        move || {
            if !fired.swap(true, Ordering::SeqCst) {
                let _ = open_tx.send(());
                // Never block a failing test binary indefinitely.
                let _ = held_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30));
            }
        }
    };
    app.set_operation_open_barrier(Some(Arc::new(hook)));
    OpenBarrier {
        fired,
        open_pending: open_rx,
        lock_held: held_tx,
    }
}

fn run_selected(
    app: &Application,
    principal: &Principal,
    job_id: String,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<Result<Option<llmbase_core::jobs::Job>, Error>> {
    let app = app.clone();
    let principal = principal.clone();
    tokio::spawn(async move {
        app.run_job(
            &principal,
            "worker-test",
            WorkerModels::default(),
            llmbase_core::worker::WorkerSelection {
                worker_id: "contended".into(),
                job_id: Some(job_id),
            },
            cancel,
        )
        .await
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claimed_repair_retries_transient_recovery_lock_contention_under_same_lease() {
    let (_dir, mut app, principal, config) = setup();
    let decoy = enqueue(&app, &principal, "doctor", json!({}));
    let selected = enqueue(
        &app,
        &principal,
        "maintenance_repair",
        json!({"mode":"clean"}),
    );
    let id = selected["id"].as_str().unwrap().to_owned();
    let barrier = install_open_barrier(&mut app);
    let worker = run_selected(&app, &principal, id.clone(), CancellationToken::new());
    barrier
        .open_pending
        .recv_timeout(Duration::from_secs(10))
        .expect("claimed operation never reached its library open");
    // Hold the recovery writer lock across the first operation-open attempts,
    // exactly like a concurrent status reader's brief Library::open.
    let session = WriterSession::acquire(&config).unwrap();
    barrier.lock_held.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    drop(session);
    let job = tokio::time::timeout(Duration::from_secs(15), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(barrier.fired.load(Ordering::SeqCst));
    assert_eq!(job.id, id);
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.attempts, 1, "no re-claim: the lease never changed");
    let library = Library::open(config).unwrap();
    let events = library.job_events(&job.id).unwrap();
    assert_eq!(
        events
            .iter()
            .map(|v| v["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["queued", "start", "ok"],
        "a single execution; the operation was never replayed"
    );
    assert_eq!(
        library.job(decoy["id"].as_str().unwrap()).unwrap().status,
        JobStatus::Queued
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claimed_repair_contention_exhaustion_fails_truthfully_without_replay() {
    let (_dir, mut app, principal, config) = setup();
    let decoy = enqueue(&app, &principal, "doctor", json!({}));
    let selected = enqueue(
        &app,
        &principal,
        "maintenance_repair",
        json!({"mode":"clean"}),
    );
    let id = selected["id"].as_str().unwrap().to_owned();
    let barrier = install_open_barrier(&mut app);
    let worker = run_selected(&app, &principal, id.clone(), CancellationToken::new());
    barrier
        .open_pending
        .recv_timeout(Duration::from_secs(10))
        .expect("claimed operation never reached its library open");
    // Outlast the bounded retry window (100 x 20ms); the failure must stay
    // truthful rather than flipping to success or replaying the operation.
    let session = WriterSession::acquire(&config).unwrap();
    barrier.lock_held.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(2600));
    drop(session);
    let job = tokio::time::timeout(Duration::from_secs(15), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(job.id, id);
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.attempts, 1);
    assert!(job.result.is_none());
    assert_eq!(
        job.error.as_deref(),
        Some("another writer is active; explicit retry is available")
    );
    let library = Library::open(config).unwrap();
    let events = library.job_events(&job.id).unwrap();
    assert_eq!(
        events
            .iter()
            .map(|v| v["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["queued", "start", "failed"]
    );
    assert_eq!(
        library.job(decoy["id"].as_str().unwrap()).unwrap().status,
        JobStatus::Queued
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_repair_open_contention_is_truthful_and_prompt() {
    let (_dir, mut app, principal, config) = setup();
    let selected = enqueue(
        &app,
        &principal,
        "maintenance_repair",
        json!({"mode":"clean"}),
    );
    let id = selected["id"].as_str().unwrap().to_owned();
    let barrier = install_open_barrier(&mut app);
    let cancel = CancellationToken::new();
    let worker = run_selected(&app, &principal, id.clone(), cancel.clone());
    barrier
        .open_pending
        .recv_timeout(Duration::from_secs(10))
        .expect("claimed operation never reached its library open");
    let session = WriterSession::acquire(&config).unwrap();
    barrier.lock_held.send(()).unwrap();
    // Let a few Busy retries happen, then cancel while contention persists.
    std::thread::sleep(Duration::from_millis(250));
    let cancelled_at = std::time::Instant::now();
    cancel.cancel();
    drop(session);
    let job = tokio::time::timeout(Duration::from_secs(15), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(job.id, id);
    assert_eq!(job.status, JobStatus::Cancelled);
    assert_eq!(
        job.error.as_deref(),
        Some("operation cancelled; committed effects are retained")
    );
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(2),
        "cancellation did not wait out the retry bound"
    );
    assert_eq!(
        Library::open(config).unwrap().job(&id).unwrap().status,
        JobStatus::Cancelled
    );
}
