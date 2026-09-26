//! A finite native worker: one durable claim, shared application dispatch, and
//! nonce-fenced completion. Process supervisors decide when to run it again.
use crate::{
    Error, Library, Result,
    application::{Application, Principal, Request},
    ask::AskModels,
    jobs::{Job, JobStatus},
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
            if cancel.is_cancelled() { return Err(ModelError::Cancelled.into()); }
            let mut ledger = Library::open(config)?;
            let lease = match selection.job_id {
                Some(id) => Some(ledger.claim_job_by_id(&id, &selection.worker_id, 60)?),
                None => ledger.claim_job(&selection.worker_id, 60)?,
            };
            let Some(lease) = lease else { return Ok(None) };
            let request = Request {
                library_id: lease.job.library_id.clone(),
                operation: lease.job.operation.clone(), args: lease.job.args.clone(),
            };
            let work_cancel = CancellationToken::new();
            let _work_guard = work_cancel.clone().drop_guard();
            runtime.block_on(async {
                let work = execute_job(application, principal, request, models, work_cancel.clone());
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
                if current.status == JobStatus::Cancelled { return Ok(Some(current)); }
                let (status, value, error) = match outcome {
                    Ok(value) if cancelled => (JobStatus::Partial, Some(value), Some("cancelled after operation accepted; inspect result for committed effects")),
                    Ok(value) => {
                        let status = match value.get("status").and_then(Value::as_str) {
                            Some("partial") => JobStatus::Partial,
                            Some("failed") => JobStatus::Failed,
                            Some("cancelled") => JobStatus::Cancelled,
                            _ => JobStatus::Succeeded,
                        };
                        (status, Some(value), None)
                    }
                    Err(Error::Model(ModelError::Cancelled)) => (JobStatus::Cancelled, None, Some("operation cancelled; committed effects are retained")),
                    Err(error) => (JobStatus::Failed, None, Some(safe_error(&error))),
                };
                Ok(Some(ledger.finish_job(&lease, status, value, error)?))
            })
        }).await.map_err(|_| Error::Capability("job worker failed".into()))?
    }
}

async fn execute_job(
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
