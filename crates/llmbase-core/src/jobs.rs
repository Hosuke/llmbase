//! Persistent single-host job ledger with renewable, fenced leases.
use crate::{Error, Library, Result};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS jobs (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, operation TEXT NOT NULL, args TEXT NOT NULL,
 status TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL,
 available_at INTEGER NOT NULL, lease_token TEXT, lease_until INTEGER, worker_id TEXT,
 result TEXT, error TEXT,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
CREATE INDEX IF NOT EXISTS runnable_jobs ON jobs(library_id,status,available_at);
CREATE TABLE IF NOT EXISTS job_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, library_id TEXT NOT NULL, job_id TEXT NOT NULL,
 event TEXT NOT NULL, payload TEXT NOT NULL,
 ts TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Partial,
    Cancelled,
    Interrupted,
}
impl JobStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Partial => "partial",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

/// A worker's terminal outcome, persisted through the current lease's fence.
/// Completed outcomes require a non-null result; `{}` is an explicit no-op.
#[derive(Debug, Clone)]
pub enum JobOutcome {
    Completed {
        result: Value,
    },
    Partial {
        result: Option<Value>,
        error: Option<String>,
    },
    Failed {
        result: Option<Value>,
        error: Option<String>,
    },
    Cancelled {
        result: Option<Value>,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub library_id: String,
    pub id: String,
    pub operation: String,
    pub args: Value,
    pub status: JobStatus,
    pub attempts: u32,
    pub max_attempts: u32,
    pub available_at: u64,
    pub lease_until: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<String>,
}

/// Nonserializable ownership token. A job ID or recycled PID cannot complete another lease.
#[derive(Debug)]
pub struct JobLease {
    pub job: Job,
    token: String,
}

impl Library {
    fn jobs_schema(&self) -> Result<()> {
        self.db.execute_batch(SCHEMA)?;
        Ok(())
    }

    pub fn enqueue_job(&mut self, operation: &str, args: Value, max_attempts: u32) -> Result<Job> {
        if operation.is_empty()
            || operation.len() > 128
            || !args.is_object()
            || !(1..=20).contains(&max_attempts)
        {
            return Err(Error::Invalid(
                "invalid job operation, args or retry budget".into(),
            ));
        }
        reject_credentials(&args)?;
        self.jobs_schema()?;
        let id = Uuid::new_v4().to_string();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO jobs(id,library_id,operation,args,status,max_attempts,available_at) VALUES (?1,?2,?3,?4,'queued',?5,?6)",params![id,self.config.library_id,operation,serde_json::to_string(&args)?,max_attempts,now()?])?;
        tx.execute(
            "INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,'queued','{}')",
            params![self.config.library_id, id],
        )?;
        tx.commit()?;
        self.job(&id)
    }

    /// Start an inline operation atomically. A general queue worker must never
    /// observe an intermediate queued record and steal the inline operation.
    pub fn enqueue_running_job(
        &mut self,
        operation: &str,
        args: Value,
        max_attempts: u32,
        worker_id: &str,
        lease_seconds: u64,
    ) -> Result<JobLease> {
        if operation.is_empty()
            || operation.len() > 128
            || !args.is_object()
            || !(1..=20).contains(&max_attempts)
            || worker_id.is_empty()
            || worker_id.len() > 128
        {
            return Err(Error::Invalid("invalid running job parameters".into()));
        }
        validate_lease(lease_seconds)?;
        reject_credentials(&args)?;
        self.jobs_schema()?;
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().to_string();
        let now = now()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO jobs(id,library_id,operation,args,status,attempts,max_attempts,available_at,worker_id,lease_token,lease_until) VALUES (?1,?2,?3,?4,'running',1,?5,?6,?7,?8,?9)",
            params![id,self.config.library_id,operation,serde_json::to_string(&args)?,max_attempts,now,worker_id,token,now+lease_seconds],
        )?;
        tx.execute(
            "INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,'queued','{}')",
            params![self.config.library_id, id],
        )?;
        tx.execute(
            "INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,'start',?3)",
            params![
                self.config.library_id,
                id,
                json!({"worker_id":worker_id,"lease_until":now+lease_seconds}).to_string()
            ],
        )?;
        tx.commit()?;
        Ok(JobLease {
            job: self.job(&id)?,
            token,
        })
    }

    pub fn job(&self, id: &str) -> Result<Job> {
        self.jobs_schema()?;
        let raw=self.db.query_row("SELECT id,operation,args,status,attempts,max_attempts,available_at,lease_until,result,error FROM jobs WHERE library_id=?1 AND id=?2",params![self.config.library_id,id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,u32>(4)?,r.get::<_,u32>(5)?,r.get::<_,u64>(6)?,r.get::<_,Option<u64>>(7)?,r.get::<_,Option<String>>(8)?,r.get::<_,Option<String>>(9)?))).optional()?.ok_or_else(||Error::NotFound("job".into()))?;
        let (
            id,
            operation,
            args,
            status,
            attempts,
            max_attempts,
            available_at,
            lease_until,
            result,
            error,
        ) = raw;
        Ok(Job {
            library_id: self.config.library_id.clone(),
            id,
            operation,
            args: serde_json::from_str(&args)?,
            status: serde_json::from_value(json!(status))?,
            attempts,
            max_attempts,
            available_at,
            lease_until,
            result: result.map(|s| serde_json::from_str(&s)).transpose()?,
            error,
        })
    }

    pub fn jobs(&self) -> Result<Vec<Job>> {
        self.jobs_schema()?;
        let mut stmt = self
            .db
            .prepare("SELECT id FROM jobs WHERE library_id=?1 ORDER BY created_at,id")?;
        let ids = stmt
            .query_map([&self.config.library_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| self.job(id)).collect()
    }

    /// Expired leases become interrupted, never successful. Explicit retry controls backoff.
    pub fn claim_job(&mut self, worker_id: &str, lease_seconds: u64) -> Result<Option<JobLease>> {
        self.claim_job_inner(None, worker_id, lease_seconds)
    }

    /// Claim exactly the caller-created operation; never consume an unrelated queued job.
    pub fn claim_job_by_id(
        &mut self,
        id: &str,
        worker_id: &str,
        lease_seconds: u64,
    ) -> Result<JobLease> {
        self.claim_job_inner(Some(id), worker_id, lease_seconds)?
            .ok_or_else(|| Error::Conflict("requested job is not available to claim".into()))
    }

    fn claim_job_inner(
        &mut self,
        job_id: Option<&str>,
        worker_id: &str,
        lease_seconds: u64,
    ) -> Result<Option<JobLease>> {
        validate_lease(lease_seconds)?;
        if worker_id.is_empty() || worker_id.len() > 128 {
            return Err(Error::Invalid("worker ID required".into()));
        }
        self.jobs_schema()?;
        let now = now()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let expired = {
            let mut stmt = tx.prepare(
                "SELECT id FROM jobs WHERE library_id=?1 AND status='running' AND lease_until<=?2",
            )?;
            stmt.query_map(params![self.config.library_id, now], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for id in &expired {
            tx.execute("UPDATE jobs SET status='interrupted',lease_token=NULL,lease_until=NULL,error='lease expired',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND library_id=?2",params![id,self.config.library_id])?;
            tx.execute("INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,'interrupted','{\"by\":\"lease_expired\"}')",params![self.config.library_id,id])?;
        }
        let id:Option<String>=tx.query_row("SELECT id FROM jobs WHERE library_id=?1 AND status='queued' AND available_at<=?2 AND attempts<max_attempts AND (?3 IS NULL OR id=?3) ORDER BY available_at,created_at,id LIMIT 1",params![self.config.library_id,now,job_id],|r|r.get(0)).optional()?;
        let token = Uuid::new_v4().to_string();
        if let Some(id) = &id {
            tx.execute("UPDATE jobs SET status='running',attempts=attempts+1,worker_id=?3,lease_token=?4,lease_until=?5,error=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE library_id=?1 AND id=?2",params![self.config.library_id,id,worker_id,token,now+lease_seconds])?;
            tx.execute(
                "INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,'start',?3)",
                params![
                    self.config.library_id,
                    id,
                    json!({"worker_id":worker_id,"lease_until":now+lease_seconds}).to_string()
                ],
            )?;
        }
        tx.commit()?;
        id.map(|id| {
            Ok(JobLease {
                job: self.job(&id)?,
                token,
            })
        })
        .transpose()
    }

    pub fn heartbeat_job(&mut self, lease: &JobLease, lease_seconds: u64) -> Result<()> {
        validate_lease(lease_seconds)?;
        let now = now()?;
        let count=self.db.execute("UPDATE jobs SET lease_until=?4,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE library_id=?1 AND id=?2 AND lease_token=?3 AND status='running' AND lease_until>?5",params![self.config.library_id,lease.job.id,lease.token,now+lease_seconds,now])?;
        if count != 1 {
            return Err(Error::Conflict(
                "job lease expired, cancelled or replaced".into(),
            ));
        }
        Ok(())
    }

    pub fn finish_job_outcome(&mut self, lease: &JobLease, outcome: JobOutcome) -> Result<Job> {
        let (status, result, error) = match outcome {
            JobOutcome::Completed { result } => (JobStatus::Succeeded, Some(result), None),
            JobOutcome::Partial { result, error } => (JobStatus::Partial, result, error),
            JobOutcome::Failed { result, error } => (JobStatus::Failed, result, error),
            JobOutcome::Cancelled { result, error } => (JobStatus::Cancelled, result, error),
        };
        self.finish_job(lease, status, result, error.as_deref())
    }

    pub fn finish_job(
        &mut self,
        lease: &JobLease,
        status: JobStatus,
        result: Option<Value>,
        error: Option<&str>,
    ) -> Result<Job> {
        if matches!(
            status,
            JobStatus::Queued | JobStatus::Running | JobStatus::Interrupted
        ) {
            return Err(Error::Invalid("invalid terminal job status".into()));
        }
        if status == JobStatus::Succeeded && error.is_some() {
            return Err(Error::Invalid("successful job cannot have an error".into()));
        }
        if status == JobStatus::Succeeded && matches!(result.as_ref(), None | Some(Value::Null)) {
            return Err(Error::Invalid(
                "successful job requires an explicit non-null result".into(),
            ));
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count=tx.execute("UPDATE jobs SET status=?4,result=?5,error=?6,lease_token=NULL,lease_until=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE library_id=?1 AND id=?2 AND lease_token=?3 AND status='running' AND lease_until>?7",params![self.config.library_id,lease.job.id,lease.token,status.as_str(),result.as_ref().map(Value::to_string),error,now()?])?;
        if count != 1 {
            return Err(Error::Conflict(
                "job lease expired, cancelled or replaced".into(),
            ));
        }
        let event = if status == JobStatus::Succeeded {
            "ok"
        } else {
            status.as_str()
        };
        tx.execute(
            "INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,?3,?4)",
            params![
                self.config.library_id,
                lease.job.id,
                event,
                json!({"result":result,"err":error}).to_string()
            ],
        )?;
        tx.commit()?;
        self.job(&lease.job.id)
    }

    pub fn cancel_job(&mut self, id: &str) -> Result<Job> {
        self.jobs_schema()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count=tx.execute("UPDATE jobs SET status='cancelled',lease_token=NULL,lease_until=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE library_id=?1 AND id=?2 AND status IN ('queued','running')",params![self.config.library_id,id])?;
        if count != 1 {
            return Err(Error::Conflict("job is missing or already terminal".into()));
        }
        tx.execute("INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,'cancelled','{}')",params![self.config.library_id,id])?;
        tx.commit()?;
        self.job(id)
    }

    pub fn retry_job(&mut self, id: &str) -> Result<Job> {
        self.jobs_schema()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let attempts:u32=tx.query_row("SELECT attempts FROM jobs WHERE library_id=?1 AND id=?2 AND status IN ('failed','partial','interrupted') AND attempts<max_attempts",params![self.config.library_id,id],|r|r.get(0)).optional()?.ok_or_else(||Error::Conflict("job cannot retry or retry budget exhausted".into()))?;
        let backoff = 2u64.saturating_pow(attempts.saturating_sub(1)).min(300);
        tx.execute("UPDATE jobs SET status='queued',available_at=?3,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE library_id=?1 AND id=?2",params![self.config.library_id,id,now()?+backoff])?;
        tx.execute(
            "INSERT INTO job_events(library_id,job_id,event,payload) VALUES (?1,?2,'queued',?3)",
            params![
                self.config.library_id,
                id,
                json!({"retry":true,"backoff_seconds":backoff}).to_string()
            ],
        )?;
        tx.commit()?;
        self.job(id)
    }

    pub fn job_events(&self, id: &str) -> Result<Vec<Value>> {
        self.job(id)?;
        let mut stmt=self.db.prepare("SELECT sequence,event,payload,ts FROM job_events WHERE library_id=?1 AND job_id=?2 ORDER BY sequence")?;
        let records = stmt
            .query_map(params![self.config.library_id, id], |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        records.into_iter().map(|(sequence,event,payload,ts)|Ok(json!({"library_id":self.config.library_id,"job_id":id,"sequence":sequence,"event":event,"payload":serde_json::from_str::<Value>(&payload)?,"ts":ts}))).collect()
    }
}

fn validate_lease(seconds: u64) -> Result<()> {
    if !(1..=3600).contains(&seconds) {
        Err(Error::Invalid("lease must be 1..3600 seconds".into()))
    } else {
        Ok(())
    }
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| Error::Conflict("system clock precedes epoch".into()))
}
fn reject_credentials(value: &Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "api_key" | "authorization" | "secret" | "password" | "access_token"
                ) {
                    return Err(Error::Invalid(
                        "credentials must be supplied at execution time, never stored in jobs"
                            .into(),
                    ));
                }
                reject_credentials(v)?;
            }
        }
        Value::Array(values) => {
            for v in values {
                reject_credentials(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LibraryConfig;
    use tempfile::TempDir;

    #[test]
    fn successful_jobs_require_an_explicit_non_null_result() {
        let dir = TempDir::new().unwrap();
        let mut lib = Library::open(LibraryConfig::load(dir.path(), "results").unwrap()).unwrap();
        let lease = lib
            .enqueue_running_job("compile", json!({}), 1, "worker", 30)
            .unwrap();
        let events = lib.job_events(&lease.job.id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["event"], "queued");
        assert_eq!(events[1]["event"], "start");

        for result in [None, Some(Value::Null)] {
            assert!(matches!(
                lib.finish_job(&lease, JobStatus::Succeeded, result, None),
                Err(Error::Invalid(_))
            ));
            let running = lib.job(&lease.job.id).unwrap();
            assert_eq!(running.status, JobStatus::Running);
            assert_eq!(running.result, None);
            assert_eq!(running.error, None);
            assert_eq!(running.lease_until, lease.job.lease_until);
            assert_eq!(lib.job_events(&lease.job.id).unwrap(), events);
        }
        assert!(matches!(
            lib.finish_job_outcome(
                &lease,
                JobOutcome::Completed {
                    result: Value::Null,
                },
            ),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            lib.finish_job(
                &lease,
                JobStatus::Succeeded,
                Some(json!({})),
                Some("cannot succeed with an error"),
            ),
            Err(Error::Invalid(_))
        ));
        assert_eq!(lib.job(&lease.job.id).unwrap().status, JobStatus::Running);
        assert_eq!(lib.job_events(&lease.job.id).unwrap(), events);

        let completed = lib
            .finish_job_outcome(&lease, JobOutcome::Completed { result: json!({}) })
            .unwrap();
        assert_eq!(completed.status, JobStatus::Succeeded);
        assert_eq!(completed.result, Some(json!({})));
        assert_eq!(completed.error, None);
        assert_eq!(completed.lease_until, None);
        assert_eq!(lib.job(&lease.job.id).unwrap().result, Some(json!({})));
        let events = lib.job_events(&lease.job.id).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2]["event"], "ok");
        assert_eq!(events[2]["payload"], json!({"result":{},"err":null}));
    }

    #[test]
    fn partial_outcomes_preserve_results_across_reopen_and_retry() {
        let dir = TempDir::new().unwrap();
        let cfg = LibraryConfig::load(dir.path(), "partial-results").unwrap();
        let mut lib = Library::open(cfg.clone()).unwrap();
        let job = lib.enqueue_job("compile", json!({}), 2).unwrap();
        let lease = lib.claim_job_by_id(&job.id, "worker", 30).unwrap();
        let result = json!({
            "artifacts": [{"path":"wiki/concepts/retained.md","kind":"article"}],
            "completed": 1
        });
        let partial = lib
            .finish_job_outcome(
                &lease,
                JobOutcome::Partial {
                    result: Some(result.clone()),
                    error: Some("quota exhausted".into()),
                },
            )
            .unwrap();
        assert_eq!(partial.status, JobStatus::Partial);
        assert_eq!(partial.result, Some(result.clone()));
        assert_eq!(partial.error.as_deref(), Some("quota exhausted"));
        let events = lib.job_events(&job.id).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2]["event"], "partial");
        assert_eq!(events[2]["payload"]["result"], result);
        assert_eq!(events[2]["payload"]["err"], "quota exhausted");
        drop(lib);

        let mut lib = Library::open(cfg).unwrap();
        let reopened = lib.job(&job.id).unwrap();
        assert_eq!(reopened.library_id, job.library_id);
        assert_eq!(reopened.id, job.id);
        assert_eq!(reopened.status, JobStatus::Partial);
        assert_eq!(reopened.result, Some(result.clone()));
        assert_eq!(reopened.error.as_deref(), Some("quota exhausted"));
        assert_eq!(lib.job_events(&job.id).unwrap(), events);

        let queued = lib.retry_job(&job.id).unwrap();
        assert_eq!(queued.library_id, job.library_id);
        assert_eq!(queued.id, job.id);
        assert_eq!(queued.status, JobStatus::Queued);
        assert_eq!(queued.result, Some(result.clone()));
        lib.db
            .execute(
                "UPDATE jobs SET available_at=0 WHERE library_id=?1 AND id=?2",
                params![job.library_id, job.id],
            )
            .unwrap();
        let retry = lib.claim_job_by_id(&job.id, "retry-worker", 30).unwrap();
        assert_eq!(retry.job.library_id, job.library_id);
        assert_eq!(retry.job.id, job.id);
        assert_eq!(retry.job.status, JobStatus::Running);
        assert_eq!(retry.job.attempts, 2);
        assert_eq!(retry.job.result, Some(result));
    }

    #[test]
    fn jobs_survive_reopen_and_expired_owners_cannot_complete() {
        let dir = TempDir::new().unwrap();
        let cfg = LibraryConfig::load(dir.path(), "jobs").unwrap();
        let mut lib = Library::open(cfg.clone()).unwrap();
        let job = lib
            .enqueue_job("compile", json!({"source_id":"a"}), 2)
            .unwrap();
        let old = lib.claim_job("worker-pid-reused", 30).unwrap().unwrap();
        assert_eq!(old.job.id, job.id);
        assert!(lib.claim_job("other", 30).unwrap().is_none());
        lib.db.execute("UPDATE jobs SET lease_until=0", []).unwrap();
        drop(lib);
        let mut lib = Library::open(cfg).unwrap();
        assert!(lib.claim_job("restart", 30).unwrap().is_none());
        assert_eq!(lib.job(&job.id).unwrap().status, JobStatus::Interrupted);
        let events = lib.job_events(&job.id).unwrap();
        assert!(matches!(
            lib.finish_job_outcome(&old, JobOutcome::Completed { result: json!({}) }),
            Err(Error::Conflict(_))
        ));
        assert_eq!(lib.job_events(&job.id).unwrap(), events);
        let queued = lib.retry_job(&job.id).unwrap();
        assert!(queued.available_at >= now().unwrap());
        lib.db
            .execute("UPDATE jobs SET available_at=0", [])
            .unwrap();
        let new = lib.claim_job("worker-pid-reused", 30).unwrap().unwrap();
        assert_ne!(old.token, new.token);
        let events = lib.job_events(&job.id).unwrap();
        assert!(matches!(
            lib.finish_job_outcome(&old, JobOutcome::Completed { result: json!({}) }),
            Err(Error::Conflict(_))
        ));
        assert_eq!(lib.job(&job.id).unwrap().status, JobStatus::Running);
        assert_eq!(lib.job_events(&job.id).unwrap(), events);
        lib.heartbeat_job(&new, 60).unwrap();
        assert_eq!(
            lib.finish_job(&new, JobStatus::Partial, None, Some("quota exhausted"))
                .unwrap()
                .status,
            JobStatus::Partial
        );
        assert!(lib.retry_job(&job.id).is_err());
        assert_eq!(lib.job_events(&job.id).unwrap().len(), 6);
    }
    #[test]
    fn cancellation_fences_running_owner_and_credentials_are_not_persisted() {
        let dir = TempDir::new().unwrap();
        let mut lib = Library::open(LibraryConfig::load(dir.path(), "cancel").unwrap()).unwrap();
        assert!(
            lib.enqueue_job("ask", json!({"nested":{"api_key":"never-persist"}}), 1)
                .is_err()
        );
        let job = lib.enqueue_job("ask", json!({"question":"a"}), 1).unwrap();
        let lease = lib.claim_job("worker", 30).unwrap().unwrap();
        assert_eq!(
            lib.cancel_job(&job.id).unwrap().status,
            JobStatus::Cancelled
        );
        assert!(matches!(
            lib.heartbeat_job(&lease, 30),
            Err(Error::Conflict(_))
        ));
        let events = lib.job_events(&job.id).unwrap();
        assert!(matches!(
            lib.finish_job_outcome(&lease, JobOutcome::Completed { result: json!({}) }),
            Err(Error::Conflict(_))
        ));
        assert_eq!(lib.job(&job.id).unwrap().status, JobStatus::Cancelled);
        assert_eq!(lib.job_events(&job.id).unwrap(), events);
    }
}
