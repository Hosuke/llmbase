//! A finite native worker: one durable claim, shared application dispatch, and
//! nonce-fenced completion. Process supervisors decide when to run it again.
use crate::{
    Error, Library, Result,
    application::{Application, Principal, Request},
    ask::AskModels,
    jobs::{Job, JobOutcome, JobStatus},
};
use llmbase_model::{CancellationToken, Client, ModelError};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct WorkerModels {
    pub generation: Option<Client>,
    pub embedding: Option<Client>,
}

#[derive(Clone, Debug)]
pub struct WorkerSelection {
    pub worker_id: String,
    pub job_id: Option<String>,
}

impl Application {
    pub async fn run_next_job(
        &self,
        principal: &Principal,
        library_id: &str,
        models: WorkerModels,
        worker_id: &str,
        cancel: CancellationToken,
    ) -> Result<Option<Job>> {
        self.run_job(
            principal,
            library_id,
            models,
            WorkerSelection {
                worker_id: worker_id.into(),
                job_id: None,
            },
            cancel,
        )
        .await
    }

    pub async fn run_job(
        &self,
        principal: &Principal,
        library_id: &str,
        models: WorkerModels,
        selection: WorkerSelection,
        cancel: CancellationToken,
    ) -> Result<Option<Job>> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "job_run_one".into(),
                args: json!({}),
            },
        )?;
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled.into());
        }
        let config = self.library_config(library_id)?;
        let application = self.clone();
        let principal = principal.clone();
        if selection
            .job_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
        {
            return Err(Error::Invalid("invalid job selection".into()));
        }
        let runtime = tokio::runtime::Handle::current();
        let _cancel_on_drop = cancel.clone().drop_guard();
        tokio::task::spawn_blocking(move || {
            if cancel.is_cancelled() {
                return Err(ModelError::Cancelled.into());
            }
            let mut ledger = Library::open(config)?;
            let lease = match selection.job_id {
                Some(id) => Some(ledger.claim_job_by_id(&id, &selection.worker_id, 60)?),
                None => ledger.claim_job(&selection.worker_id, 60)?,
            };
            let Some(lease) = lease else { return Ok(None) };
            let request = Request {
                library_id: lease.job.library_id.clone(),
                operation: lease.job.operation.clone(),
                args: lease.job.args.clone(),
            };
            let work_cancel = CancellationToken::new();
            let _work_guard = work_cancel.clone().drop_guard();
            runtime.block_on(async {
                let work =
                    execute_job(application, principal, request, models, work_cancel.clone());
                tokio::pin!(work);
                let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
                heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let (outcome, cancelled) = loop {
                    tokio::select! {
                        outcome = &mut work => break (outcome, false),
                        _ = cancel.cancelled() => {
                            work_cancel.cancel();
                            break ((&mut work).await, true);
                        }
                        _ = heartbeat.tick() => {
                            let current = ledger.job(&lease.job.id)?;
                            if current.status == JobStatus::Cancelled {
                                work_cancel.cancel();
                                // Accepted filesystem writes may complete; cancellation
                                // never claims to roll back their committed side effects.
                                let _ = (&mut work).await;
                                return Ok(Some(current));
                            }
                            ledger.heartbeat_job(&lease, 60)?;
                        }
                    }
                };
                let current = ledger.job(&lease.job.id)?;
                if current.status == JobStatus::Cancelled {
                    return Ok(Some(current));
                }
                // A ready result and cancellation may arrive in the same select.
                // Inspect the token as well as the selected branch before accepting
                // completion; previously committed effects remain in the result.
                let outcome = respect_cancellation(outcome, cancelled || cancel.is_cancelled());
                Ok(Some(ledger.finish_job_outcome(&lease, outcome)?))
            })
        })
        .await
        .map_err(|_| Error::Capability("job worker failed".into()))?
    }
}

async fn execute_job(
    application: Application,
    principal: Principal,
    request: Request,
    models: WorkerModels,
    cancel: CancellationToken,
) -> JobOutcome {
    let operation = request.operation.clone();
    let result = execute_job_value(application, principal, request, models, cancel).await;
    handler_outcome(&operation, result)
}

async fn execute_job_value(
    application: Application,
    principal: Principal,
    request: Request,
    models: WorkerModels,
    cancel: CancellationToken,
) -> Result<Value> {
    application.authorize(&principal, &request)?;
    if request.operation.starts_with("job_") {
        return Err(Error::Invalid(
            "nested job management is not allowed".into(),
        ));
    }
    if cancel.is_cancelled() {
        return Err(ModelError::Cancelled.into());
    }
    let required_generation = || {
        models
            .generation
            .clone()
            .ok_or_else(|| Error::Capability("generation model is not configured".into()))
    };
    match request.operation.as_str() {
        "ingest_url" => {
            let url = request
                .args
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Invalid("url is required".into()))?;
            application
                .ingest_url(&principal, &request.library_id, url, cancel)
                .await
        }
        "maintenance_repair" => {
            let no_model = matches!(
                request.args.get("mode").and_then(Value::as_str),
                Some("clean" | "heal_slugs")
            );
            let client = if no_model {
                None
            } else {
                Some(required_generation()?)
            };
            application
                .repair(
                    &principal,
                    &request.library_id,
                    client,
                    request.args,
                    cancel,
                )
                .await
        }
        "legacy_query" | "legacy_compile" => {
            application
                .legacy_workflow(
                    &principal,
                    &request.library_id,
                    required_generation()?,
                    &request.operation,
                    request.args,
                    cancel,
                )
                .await
        }
        "taxonomy_generate" | "xici_generate" | "entities_generate" => {
            application
                .generate_maintenance(
                    &principal,
                    &request.library_id,
                    required_generation()?,
                    &request.operation,
                    request.args,
                    cancel,
                )
                .await
        }
        "plugin_ingest" => {
            application
                .ingest_with_adapter(
                    &principal,
                    &request.library_id,
                    serde_json::from_value(request.args)?,
                    cancel,
                )
                .await
        }
        "compile" => Ok(serde_json::to_value(
            application
                .compile_source(
                    &principal,
                    &request.library_id,
                    required_generation()?,
                    serde_json::from_value(request.args)?,
                    cancel,
                )
                .await?,
        )?),
        "answer_promote" => Ok(serde_json::to_value(
            application
                .promote_answer(
                    &principal,
                    &request.library_id,
                    required_generation()?,
                    serde_json::from_value(request.args)?,
                    cancel,
                )
                .await?,
        )?),
        "vector_build" => Ok(serde_json::to_value(
            application
                .build_vectors(
                    &principal,
                    &request.library_id,
                    models.embedding.ok_or_else(|| {
                        Error::Capability("embedding model is not configured".into())
                    })?,
                    serde_json::from_value(request.args)?,
                    cancel,
                )
                .await?,
        )?),
        "hybrid_search" => Ok(serde_json::to_value(
            application
                .search_hybrid(
                    &principal,
                    &request.library_id,
                    models.embedding,
                    serde_json::from_value(request.args)?,
                    cancel,
                )
                .await?,
        )?),
        "ask" => {
            let generation = required_generation()?;
            let mut args = request.args;
            if args.get("library_id").is_some() {
                return Err(Error::Invalid(
                    "library_id belongs in the operation envelope".into(),
                ));
            }
            args["library_id"] = json!(request.library_id);
            Ok(serde_json::to_value(
                application
                    .ask(
                        &principal,
                        AskModels {
                            generation,
                            embedding: models.embedding,
                        },
                        serde_json::from_value(args)?,
                        cancel,
                    )
                    .await?,
            )?)
        }
        _ => tokio::task::spawn_blocking(move || application.execute(&principal, request))
            .await
            .map_err(|_| Error::Capability("job application worker failed".into()))?,
    }
}

/// Interpret only the status-bearing operation contracts. A source or draft may
/// have a domain-specific `status` field which is not a job outcome.
fn handler_outcome(operation: &str, result: Result<Value>) -> JobOutcome {
    let value = match result {
        Ok(value) => value,
        Err(Error::Model(ModelError::Cancelled)) => {
            return JobOutcome::Cancelled {
                result: None,
                error: Some("operation cancelled; committed effects are retained".into()),
            };
        }
        Err(error) => {
            return JobOutcome::Failed {
                result: None,
                error: Some(safe_error(&error).into()),
            };
        }
    };
    let invalid = || JobOutcome::Failed {
        result: None,
        error: Some("handler returned a missing or invalid outcome".into()),
    };
    if value.is_null() {
        return invalid();
    }
    if !matches!(
        operation,
        "compile"
            | "legacy_compile"
            | "plugin_ingest"
            | "maintenance_repair"
            | "taxonomy_generate"
            | "xici_generate"
            | "entities_generate"
            | "lint_fix"
            | "web_write_wiki_clean"
    ) {
        // These built-ins return explicit domain results (including the legacy
        // query string), not terminal status reports. Custom handlers will need
        // their own typed registration contract; no unknown registry exists here.
        return JobOutcome::Completed { result: value };
    }
    match value.get("status").and_then(Value::as_str) {
        Some("succeeded") => JobOutcome::Completed { result: value },
        Some("ok") if operation == "web_write_wiki_clean" => {
            JobOutcome::Completed { result: value }
        }
        Some("no_changes") if matches!(operation, "legacy_compile" | "entities_generate") => {
            JobOutcome::Completed { result: value }
        }
        Some("disabled") if operation == "entities_generate" => {
            JobOutcome::Completed { result: value }
        }
        Some("partial") => JobOutcome::Partial {
            result: Some(value),
            error: Some("operation partially completed; inspect retained result".into()),
        },
        Some("failed") => JobOutcome::Failed {
            result: Some(value),
            error: Some("operation reported failure; inspect retained result".into()),
        },
        Some("cancelled") => JobOutcome::Cancelled {
            result: Some(value),
            error: Some("operation cancelled; committed effects are retained".into()),
        },
        _ => invalid(),
    }
}

fn respect_cancellation(outcome: JobOutcome, cancelled: bool) -> JobOutcome {
    match outcome {
        JobOutcome::Completed { result } if cancelled => JobOutcome::Partial {
            result: Some(result),
            error: Some(
                "cancelled after operation accepted; inspect result for committed effects".into(),
            ),
        },
        other => other,
    }
}

fn safe_error(error: &Error) -> &'static str {
    match error {
        Error::Scope => "library or capability denied",
        Error::Invalid(_) | Error::Json(_) | Error::Yaml(_) => {
            "invalid operation arguments or data"
        }
        Error::NotFound(_) => "resource not found",
        Error::Conflict(_) => "resource conflicts with current state",
        Error::Busy => "another writer is active; explicit retry is available",
        Error::Capability(_) => "required operation capability is unavailable",
        Error::Io(_) | Error::Sql(_) => "storage operation failed",
        Error::Model(_) => "model operation failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LibraryConfig;

    #[test]
    fn missing_or_malformed_handler_results_persist_failure_not_success() {
        for value in [
            Value::Null,
            json!({}),
            json!({"status":null}),
            json!({"status":1}),
            json!({"status":"error"}),
            json!({"status":"queued"}),
            json!("succeeded"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let config = LibraryConfig::load(dir.path(), "outcome").unwrap();
            let mut library = Library::open(config.clone()).unwrap();
            let job = library.enqueue_job("compile", json!({}), 1).unwrap();
            let lease = library.claim_job("fixture", 30).unwrap().unwrap();
            library
                .finish_job_outcome(&lease, handler_outcome("compile", Ok(value)))
                .unwrap();
            drop(library);
            let library = Library::open(config).unwrap();
            let saved = library.job(&job.id).unwrap();
            assert_eq!(saved.status, JobStatus::Failed);
            assert!(saved.result.is_none());
            assert_eq!(
                saved.error.as_deref(),
                Some("handler returned a missing or invalid outcome")
            );
            let events = library.job_events(&job.id).unwrap();
            assert_eq!(events.last().unwrap()["event"], "failed");
            assert!(!events.iter().any(|event| event["event"] == "ok"));
        }
        assert!(matches!(
            handler_outcome("index", Ok(Value::Null)),
            JobOutcome::Failed { .. }
        ));
    }

    #[test]
    fn domain_statuses_and_explicit_noops_are_not_misclassified() {
        for (operation, value) in [
            ("draft_get", json!({"status":"draft"})),
            ("legacy_query", json!("文言之答。")),
            ("index", json!({"added":0})),
            ("legacy_compile", json!({"status":"no_changes"})),
            ("entities_generate", json!({"status":"disabled"})),
            ("entities_generate", json!({"status":"no_changes"})),
            ("lint_fix", json!({"status":"succeeded","fix_count":0})),
            ("web_write_wiki_clean", json!({"status":"ok","removed":0})),
        ] {
            assert!(matches!(
                handler_outcome(operation, Ok(value)),
                JobOutcome::Completed { .. }
            ));
        }
        assert!(matches!(
            handler_outcome("compile", Ok(json!({"status":"disabled"}))),
            JobOutcome::Failed { .. }
        ));
    }

    #[test]
    fn deterministic_partial_reports_preserve_failure_and_accepted_work() {
        for (operation, value) in [
            (
                "lint_fix",
                json!({"status":"partial","fixes":["accepted"],"failures":[{"message":"failed"}]}),
            ),
            (
                "web_write_wiki_clean",
                json!({"status":"partial","slugs":["removed"],"failures":[{"message":"failed"}]}),
            ),
        ] {
            match handler_outcome(operation, Ok(value.clone())) {
                JobOutcome::Partial { result, error } => {
                    assert_eq!(result, Some(value));
                    assert!(error.is_some());
                }
                other => panic!("{operation} falsely completed: {other:?}"),
            }
        }
    }

    #[test]
    fn late_completion_after_cancellation_preserves_result_without_success() {
        let value = json!({"draft_ids":["accepted"],"status":"succeeded"});
        match respect_cancellation(handler_outcome("compile", Ok(value.clone())), true) {
            JobOutcome::Partial { result, error } => {
                assert_eq!(result, Some(value));
                assert!(
                    error
                        .unwrap()
                        .contains("cancelled after operation accepted")
                );
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert!(matches!(
            respect_cancellation(handler_outcome("compile", Err(Error::Busy)), true),
            JobOutcome::Failed { .. }
        ));
    }
}
