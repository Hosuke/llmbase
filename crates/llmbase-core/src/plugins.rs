//! Operator-configured process adapters for external formats/connectors.
//! The native core retains accepted writes, revisions, jobs and scope checks.
use crate::{
    Error, Library, LibraryConfig, Result,
    application::{Application, Principal, Request},
    jobs::JobStatus,
    store::WriterSession,
};
use llmbase_model::{CancellationToken, ModelError};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

const MAX_INPUT: usize = 16 * 1024 * 1024;
fn timeout_default() -> u64 {
    60
}
fn limit_default() -> usize {
    16 * 1024 * 1024
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Adapter {
    command: Vec<String>,
    #[serde(default = "timeout_default")]
    timeout_seconds: u64,
    #[serde(default = "limit_default")]
    max_output_bytes: usize,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterRequest {
    pub adapter: String,
    pub input: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    protocol_version: u8,
    documents: Vec<AcceptedDocument>,
    #[serde(default)]
    assets: Vec<OriginalAsset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalAsset {
    path: String,
    content_bytes: Vec<u8>,
    #[serde(default)]
    expected_hash: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedDocument {
    path: String,
    content: String,
    expected_hash: Option<String>,
}

impl Application {
    pub async fn ingest_with_adapter(
        &self,
        principal: &Principal,
        library_id: &str,
        request: AdapterRequest,
        cancel: CancellationToken,
    ) -> Result<Value> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "plugin_ingest".into(),
                args: json!({}),
            },
        )?;
        let config = self.library_config(library_id)?;
        let _guard = cancel.clone().drop_guard();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let adapter = adapter(&config, &request.adapter)?;
            let mut library = Library::open(config)?;
            runtime.block_on(library.ingest_adapter(request, adapter, cancel))
        })
        .await
        .map_err(|_| Error::Capability("process adapter worker failed".into()))?
    }
}

fn adapter(config: &LibraryConfig, name: &str) -> Result<Adapter> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
    {
        return Err(Error::Invalid("invalid adapter name".into()));
    }
    let value = config
        .settings
        .get("plugins")
        .and_then(|v| v.get("process_adapters"))
        .and_then(|v| v.get(name))
        .cloned()
        .ok_or_else(|| Error::Capability("process adapter is not configured".into()))?;
    let adapter: Adapter = serde_json::from_value(value)?;
    if adapter.command.is_empty()
        || adapter.command.len() > 32
        || !Path::new(&adapter.command[0]).is_absolute()
        || !Path::new(&adapter.command[0]).is_file()
        || adapter
            .command
            .iter()
            .any(|s| s.len() > 16_384 || s.contains('\0'))
        || !(1..=600).contains(&adapter.timeout_seconds)
        || !(1024..=64 * 1024 * 1024).contains(&adapter.max_output_bytes)
    {
        return Err(Error::Invalid(
            "invalid configured process adapter bounds or executable".into(),
        ));
    }
    Ok(adapter)
}

impl Library {
    async fn ingest_adapter(
        &mut self,
        request: AdapterRequest,
        adapter: Adapter,
        cancel: CancellationToken,
    ) -> Result<Value> {
        let input = serde_json::to_vec(
            &json!({"protocol_version":1,"library_id":self.config.library_id,"input":request.input}),
        )?;
        if input.len() > MAX_INPUT {
            return Err(Error::Invalid("adapter input exceeds 16 MiB".into()));
        }
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled.into());
        }
        // Do not persist potentially sensitive external inputs or credentials.
        let lease = self.enqueue_running_job(
            "plugin_ingest",
            json!({"adapter":request.adapter}),
            1,
            "native-adapter",
            60,
        )?;
        let run_cancel = cancel.child_token();
        let config = self.config.clone();
        let outcome = {
            let work = run_process(&config, &adapter, input, run_cancel.clone());
            tokio::pin!(work);
            let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    result=&mut work=>break result,
                    _=heartbeat.tick()=>{
                        if self.job(&lease.job.id)?.status==JobStatus::Cancelled {run_cancel.cancel();}
                        if !run_cancel.is_cancelled(){self.heartbeat_job(&lease,60)?;}
                    }
                }
            }
        };
        if self.job(&lease.job.id)?.status == JobStatus::Cancelled {
            return Err(ModelError::Cancelled.into());
        }
        let result = outcome.and_then(|bytes| self.accept_adapter_reply(&bytes, &cancel));
        match result {
            Ok(mut value) => {
                let status = if value["failed"].as_array().is_some_and(|v| !v.is_empty()) {
                    JobStatus::Partial
                } else {
                    JobStatus::Succeeded
                };
                value["job_id"] = json!(lease.job.id);
                value["status"] = serde_json::to_value(&status)?;
                self.finish_job(&lease, status, Some(value.clone()), None)?;
                Ok(value)
            }
            Err(error) => {
                let status = if matches!(error, Error::Model(ModelError::Cancelled)) {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Failed
                };
                self.finish_job(
                    &lease,
                    status,
                    None,
                    Some("external adapter failed; no provider stderr or input is persisted"),
                )?;
                Err(error)
            }
        }
    }

    fn accept_adapter_reply(&mut self, bytes: &[u8], cancel: &CancellationToken) -> Result<Value> {
        let reply: Reply = serde_json::from_slice(bytes)
            .map_err(|_| Error::Invalid("adapter reply does not match protocol v1".into()))?;
        if reply.protocol_version != 1 || reply.documents.is_empty() || reply.documents.len() > 64 {
            return Err(Error::Invalid(
                "adapter must return protocol v1 and 1..64 documents".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        if reply.assets.len() > 64 {
            return Err(Error::Invalid("too many original assets".into()));
        }
        for asset in &reply.assets {
            crate::config::scoped_path(&self.config.raw, &asset.path)?;
            if !asset.path.ends_with(".original")
                || asset.content_bytes.len() > 32 * 1024 * 1024
                || !seen.insert(&asset.path)
            {
                return Err(Error::Invalid("invalid original asset".into()));
            }
        }
        for doc in &reply.documents {
            crate::config::scoped_path(&self.config.raw, &doc.path)?;
            if !seen.insert(&doc.path) {
                return Err(Error::Invalid("duplicate adapter document path".into()));
            }
            if !matches!(
                Path::new(&doc.path).extension().and_then(|s| s.to_str()),
                Some("md" | "markdown" | "txt")
            ) {
                return Err(Error::Invalid(
                    "adapter text documents require md, markdown or txt extension".into(),
                ));
            }
            crate::document::Document::parse(doc.content.as_bytes().to_vec())?;
        }
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled.into());
        }
        let _writer = WriterSession::acquire(&self.config)?;
        let mut accepted = Vec::new();
        let mut failed = Vec::new();
        let mut originals = Vec::new();
        for asset in reply.assets {
            if cancel.is_cancelled() {
                failed.push(json!({"path":asset.path,"code":"cancelled"}));
                break;
            }
            match self.write_raw_asset(
                &asset.path,
                asset.content_bytes,
                asset.expected_hash.as_deref(),
            ) {
                Ok(value) => originals.push(value),
                Err(_) => {
                    failed.push(json!({"path":asset.path,"code":"original_write_failed"}));
                    break;
                }
            }
        }
        if !failed.is_empty() {
            if originals.is_empty() {
                return Err(Error::Conflict(
                    "original asset could not be accepted".into(),
                ));
            }
            return Ok(
                json!({"library_id":self.config.library_id,"accepted":accepted,"original_assets":originals,"failed":failed}),
            );
        }
        for doc in reply.documents {
            if cancel.is_cancelled() {
                failed.push(json!({"path":doc.path,"code":"cancelled"}));
                continue;
            }
            let content_hash = crate::document::hash(doc.content.as_bytes());
            match self.write_source_guarded_many("raw",&doc.path,doc.content.into_bytes(),doc.expected_hash.as_deref(),&[]) {
                Ok(report)=>accepted.push(json!({"path":doc.path,"content_hash":content_hash,"index_report":report})),
                Err(error)=>failed.push(json!({"path":doc.path,"code":match error {Error::Conflict(_)=>"conflict",Error::Busy=>"busy",_=>"write_failed"}})),
            }
        }
        if accepted.is_empty() && originals.is_empty() {
            return Err(Error::Conflict("no adapter documents were accepted".into()));
        }
        Ok(
            json!({"library_id":self.config.library_id,"accepted":accepted,"original_assets":originals,"failed":failed}),
        )
    }
}

async fn read_bounded(stream: impl AsyncRead + Unpin, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "adapter output limit exceeded",
        ));
    }
    Ok(bytes)
}

async fn run_process(
    config: &LibraryConfig,
    adapter: &Adapter,
    input: Vec<u8>,
    cancel: CancellationToken,
) -> Result<Vec<u8>> {
    let mut child = Command::new(&adapter.command[0])
        .args(&adapter.command[1..])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .current_dir(&config.base)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Error::Capability("configured adapter could not start".into()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Capability("adapter stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Capability("adapter stdout unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Capability("adapter stderr unavailable".into()))?;
    let io = async {
        let (_, out, _, status) = tokio::try_join!(
            async move {
                stdin.write_all(&input).await?;
                stdin.shutdown().await?;
                drop(stdin);
                Ok::<(), std::io::Error>(())
            },
            read_bounded(stdout, adapter.max_output_bytes),
            read_bounded(stderr, 64 * 1024),
            child.wait()
        )?;
        if !status.success() {
            return Err(std::io::Error::other("adapter exited unsuccessfully"));
        }
        Ok::<_, std::io::Error>(out)
    };
    let result = tokio::select! {
        _=cancel.cancelled()=>Err(Error::Model(ModelError::Cancelled)),
        result=tokio::time::timeout(Duration::from_secs(adapter.timeout_seconds),io)=>match result {
            Ok(Ok(bytes))=>Ok(bytes),Ok(Err(_))=>Err(Error::Capability("adapter failed or exceeded output bounds".into())),
            Err(_)=>Err(Error::Capability("adapter timed out".into())),
        }
    };
    if result.is_err() {
        let _ = child.kill().await;
    }
    result
}
