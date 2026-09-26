//! Native source -> persisted reviewable draft -> explicit recoverable CAS apply.
use std::collections::{HashMap, HashSet};

use llmbase_model::{CancellationToken, ChatRequest, Client, ModelError};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    Error, Library, Result, Source,
    document::hash,
    jobs::{JobLease, JobStatus},
    profile::CompileProfile,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS compile_drafts (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, job_id TEXT NOT NULL,
 source_id TEXT NOT NULL, source_revision INTEGER NOT NULL, source_hash TEXT NOT NULL,
 cache_key TEXT NOT NULL, target_path TEXT NOT NULL, base_hash TEXT, content TEXT NOT NULL,
 content_hash TEXT NOT NULL, profile_json TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('draft','applied')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
CREATE TABLE IF NOT EXISTS compile_receipts (
 library_id TEXT NOT NULL, cache_key TEXT NOT NULL, job_id TEXT NOT NULL,
 report TEXT, PRIMARY KEY(library_id,cache_key));
CREATE TABLE IF NOT EXISTS compile_draft_guards (
 library_id TEXT NOT NULL, draft_id TEXT NOT NULL, guards_json TEXT NOT NULL,
 PRIMARY KEY(library_id,draft_id));
"#;

fn metadata_visibility_rank(metadata: &Value) -> u8 {
    if metadata.get("private") == Some(&json!(true)) {
        return 2;
    }
    match metadata
        .get("visibility")
        .and_then(Value::as_str)
        .unwrap_or("public")
    {
        "public" => 0,
        "internal" => 1,
        _ => 2,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompileReport {
    pub job_id: String,
    pub draft_ids: Vec<String>,
    pub status: JobStatus,
    pub cached: bool,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompileDraft {
    pub id: String,
    pub library_id: String,
    pub job_id: String,
    pub source_id: String,
    pub source_revision: u64,
    pub source_hash: String,
    pub target_path: String,
    pub base_hash: Option<String>,
    /// Accepted-candidate complete UTF-8 file, including preserved frontmatter.
    pub content: String,
    pub content_hash: String,
    pub profile: CompileProfile,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DraftSourceGuard {
    pub source_id: String,
    pub revision: u64,
    pub content_hash: String,
}

#[derive(Deserialize)]
pub(crate) struct Article {
    slug: String,
    title: String,
    summary: String,
    tags: Vec<String>,
    content: String,
    #[serde(default)]
    aliases: Vec<String>,
}

impl Library {
    pub(crate) fn compile_schema(&self) -> Result<()> {
        self.db.execute_batch(SCHEMA)?;
        Ok(())
    }

    pub async fn compile_source(
        &mut self,
        client: &Client,
        source_id: &str,
        profile: &CompileProfile,
        cancel: CancellationToken,
    ) -> Result<CompileReport> {
        profile.validate()?;
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled.into());
        }
        let source = self.get(source_id, None, true)?;
        if source.deleted || source.kind != "raw" {
            return Err(Error::Invalid(
                "compile_source requires a current raw source".into(),
            ));
        }
        self.compile_schema()?;
        let model = profile.model.as_deref().unwrap_or(client.model());
        let cache_key = hash(&serde_json::to_vec(
            &json!({"library_id":self.config.library_id,"source_id":source.source_id,"revision":source.revision,"source_hash":source.content_hash,
            "profile":profile,"provider":client.endpoint_identity(),"model":model,"context_policy":"visibility-v3"}),
        )?);
        let previous: Option<String> = self.db.query_row("SELECT report FROM compile_receipts WHERE library_id=?1 AND cache_key=?2 AND report IS NOT NULL",params![self.config.library_id,cache_key],|r|r.get(0)).optional()?;
        if let Some(previous) = previous {
            let mut report: CompileReport = serde_json::from_str(&previous)?;
            // A receipt only counts when all persisted review artifacts exist.
            for id in &report.draft_ids {
                self.draft_guards(&self.draft(id)?)?;
            }
            if report.status == JobStatus::Succeeded && !report.draft_ids.is_empty() {
                report.cached = true;
                return Ok(report);
            }
        }
        let lease = self.enqueue_running_job("compile",json!({"source_id":source.source_id,"source_hash":source.content_hash,"profile_hash":hash(&serde_json::to_vec(profile)?),"cache_key":cache_key}),3,&format!("compile-{}", Uuid::new_v4()),60)?;
        let job = lease.job.clone();
        self.db.execute("INSERT INTO compile_receipts(library_id,cache_key,job_id,report) VALUES (?1,?2,?3,NULL) ON CONFLICT(library_id,cache_key) DO UPDATE SET job_id=excluded.job_id,report=NULL",params![self.config.library_id,cache_key,job.id])?;
        let result = self
            .compile_to_drafts(client, &source, profile, &cache_key, &lease, &cancel)
            .await;
        match result {
            Ok(report) => {
                self.finish_job(
                    &lease,
                    report.status.clone(),
                    Some(serde_json::to_value(&report)?),
                    if report.errors.is_empty() {
                        None
                    } else {
                        Some("one or more article drafts failed validation")
                    },
                )?;
                self.db.execute(
                    "UPDATE compile_receipts SET report=?3 WHERE library_id=?1 AND cache_key=?2",
                    params![
                        self.config.library_id,
                        cache_key,
                        serde_json::to_string(&report)?
                    ],
                )?;
                Ok(report)
            }
            Err(error) => {
                let drafts = {
                    let mut stmt = self.db.prepare(
                        "SELECT id FROM compile_drafts WHERE library_id=?1 AND job_id=?2",
                    )?;
                    stmt.query_map(params![self.config.library_id, job.id], |r| {
                        r.get::<_, String>(0)
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?
                };
                let status = if !drafts.is_empty() {
                    JobStatus::Partial
                } else if matches!(error, Error::Model(ModelError::Cancelled)) {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Failed
                };
                // Failure is visible in the durable ledger, never an empty success.
                let _ = self.finish_job(
                    &lease,
                    status,
                    Some(json!({"draft_ids":drafts})),
                    Some(&safe_compile_error(&error)),
                );
                Err(error)
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // Explicit durable source/profile/job inputs to one compile run.
    async fn compile_to_drafts(
        &mut self,
        client: &Client,
        source: &Source,
        profile: &CompileProfile,
        cache_key: &str,
        lease: &JobLease,
        cancel: &CancellationToken,
    ) -> Result<CompileReport> {
        let source_rank = metadata_visibility_rank(&source.metadata);
        let existing = self
            .list(Some("compiled"), source_rank > 0)?
            .into_iter()
            .filter(|candidate| metadata_visibility_rank(&candidate.metadata) <= source_rank)
            .collect::<Vec<_>>();
        let descriptions = existing
            .iter()
            .take(100)
            .map(|s| json!({"slug":s.slug,"title":s.title,"summary":s.metadata.get("summary")}))
            .collect::<Vec<_>>();
        let prompt = render_prompt(
            &profile.user_prompt,
            &HashMap::from([
                ("title", source.title.clone()),
                ("content", source.content[source.body_start..].to_owned()),
                ("metadata", serde_json::to_string(&source.metadata)?),
                ("existing", serde_json::to_string(&descriptions)?),
                (
                    "language_profile",
                    serde_json::to_string(&profile.language)?,
                ),
                (
                    "article_format",
                    profile
                        .language
                        .sections
                        .iter()
                        .map(|s| {
                            if s.header.is_empty() {
                                format!("{} bare body", s.key)
                            } else {
                                s.header.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
            ]),
        );
        let mut request = ChatRequest::new(prompt).with_system(profile.system_prompt.clone());
        request.max_tokens = profile.max_tokens;
        request.model = profile.model.clone();
        let generation = client.complete(request, cancel.clone());
        tokio::pin!(generation);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(15));
        let response = loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(ModelError::Cancelled.into()),
                _ = heartbeat.tick() => { self.heartbeat_job(lease,60)?; },
                response = &mut generation => break response?,
            }
        };
        if response.truncated {
            return Err(Error::Invalid(
                "compilation model output is truncated".into(),
            ));
        }
        if response.content.trim().is_empty() {
            return Err(Error::Invalid(
                "compilation model returned empty content".into(),
            ));
        }
        let output: Value = serde_json::from_str(&response.content)
            .map_err(|_| Error::Invalid("compilation model returned invalid JSON".into()))?;
        let articles = output
            .get("articles")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty() && a.len() <= 100)
            .ok_or_else(|| {
                Error::Invalid("compilation requires 1..100 structured articles".into())
            })?;
        let mut report = CompileReport {
            job_id: lease.job.id.clone(),
            draft_ids: Vec::new(),
            status: JobStatus::Succeeded,
            cached: false,
            errors: Vec::new(),
        };
        let mut targets = HashSet::new();
        for (index, raw) in articles.iter().enumerate() {
            if cancel.is_cancelled() {
                if report.draft_ids.is_empty() {
                    return Err(ModelError::Cancelled.into());
                }
                report
                    .errors
                    .push("compilation cancelled after creating some drafts".into());
                break;
            }
            self.heartbeat_job(lease, 60)?;
            let prepared = serde_json::from_value::<Article>(raw.clone())
                .map_err(|_| Error::Invalid("article schema is invalid".into()))
                .and_then(|article| {
                    self.prepare_draft(
                        source,
                        &article,
                        profile,
                        &existing,
                        cache_key,
                        &lease.job.id,
                        index,
                    )
                });
            match prepared {
                Ok(draft) => {
                    if !targets.insert(draft.target_path.clone()) {
                        report.errors.push(format!(
                            "article {index}: duplicate target in one model batch"
                        ));
                        continue;
                    }
                    // The target's reviewed base hash covers its own mutation.
                    // Every other description sent to the model remains sealed.
                    let guards = existing
                        .iter()
                        .take(100)
                        .filter(|candidate| candidate.rel_path != draft.target_path)
                        .map(|candidate| DraftSourceGuard {
                            source_id: candidate.source_id.clone(),
                            revision: candidate.revision,
                            content_hash: candidate.content_hash.clone(),
                        })
                        .collect::<Vec<_>>();
                    self.persist_guarded_compile_draft(&draft, cache_key, &guards)?;
                    report.draft_ids.push(draft.id);
                }
                Err(error) => report
                    .errors
                    .push(format!("article {index}: {}", safe_compile_error(&error))),
            }
        }
        if report.draft_ids.is_empty() {
            return Err(Error::Invalid(
                "compilation produced no valid article drafts".into(),
            ));
        }
        if !report.errors.is_empty() {
            report.status = JobStatus::Partial;
        }
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_draft(
        &self,
        source: &Source,
        article: &Article,
        profile: &CompileProfile,
        existing: &[Source],
        cache_key: &str,
        job_id: &str,
        index: usize,
    ) -> Result<CompileDraft> {
        validate_slug(&article.slug)?;
        if article.title.trim().is_empty()
            || article.title.len() > 4096
            || article.summary.len() > 65536
            || article.content.trim().is_empty()
            || article.content.len() > 8 * 1024 * 1024
            || article.tags.len() > 256
            || article.aliases.len() > 256
            || article
                .tags
                .iter()
                .chain(&article.aliases)
                .any(|s| s.is_empty() || s.len() > 1024)
        {
            return Err(Error::Invalid(
                "article content or metadata is empty or exceeds limits".into(),
            ));
        }
        let sections = profile.language.split(&article.content);
        if !profile
            .language
            .sections
            .iter()
            .any(|s| sections.get(&s.key).is_some_and(|v| !v.trim().is_empty()))
        {
            return Err(Error::Invalid(
                "article has no content in configured language sections".into(),
            ));
        }
        let target = resolve_target(self, existing, article)?;
        let mut metadata = target
            .map(|s| s.metadata.clone())
            .unwrap_or_else(|| json!({}));
        let body = target
            .map(|s| {
                profile
                    .language
                    .merge(&s.content[s.body_start..], &article.content)
            })
            .unwrap_or_else(|| article.content.trim().to_owned());
        if target.is_none() {
            metadata["title"] = json!(article.title);
            metadata["summary"] = json!(article.summary);
        }
        merge_strings(&mut metadata, "tags", &article.tags)?;
        let mut aliases = article.aliases.clone();
        if let Some(old) = target {
            if old.slug != article.slug {
                aliases.push(article.slug.clone());
            }
            if old.title != article.title {
                aliases.push(article.title.clone());
            }
        }
        merge_strings(&mut metadata, "aliases", &aliases)?;
        let mut support = source.metadata.clone();
        support["library_id"] = json!(self.config.library_id);
        support["source_id"] = json!(source.source_id);
        support["revision"] = json!(source.revision);
        support["content_hash"] = json!(source.content_hash);
        support["title"] = json!(source.title);
        let sources = metadata
            .as_object_mut()
            .ok_or_else(|| Error::Invalid("target metadata is not a mapping".into()))?
            .entry("sources")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| Error::Invalid("target sources metadata is not an array".into()))?;
        if !sources.contains(&support) {
            sources.push(support);
        }
        metadata["compile_profile_revision"] = json!(profile.revision);
        metadata["compile_context_policy"] = json!("visibility-v3");
        let now: String =
            self.db
                .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
                    r.get(0)
                })?;
        if metadata.get("created").is_none() {
            metadata["created"] = json!(now);
        }
        metadata["updated"] = json!(now);
        // A private/internal source must never make its content public by merging.
        let source_visibility = if source.metadata.get("private") == Some(&Value::Bool(true)) {
            "private"
        } else {
            source
                .metadata
                .get("visibility")
                .and_then(Value::as_str)
                .unwrap_or("public")
        };
        let target_visibility = if metadata.get("private") == Some(&Value::Bool(true)) {
            "private"
        } else {
            metadata
                .get("visibility")
                .and_then(Value::as_str)
                .unwrap_or("public")
        };
        let rank = |value: &str| match value {
            "private" => 2,
            "internal" => 1,
            _ => 0,
        };
        let visibility = if rank(source_visibility) > rank(target_visibility) {
            source_visibility
        } else {
            target_visibility
        }
        .to_owned();
        metadata["visibility"] = json!(visibility);
        let yaml = crate::document::yaml_metadata(&metadata)?;
        let content = format!("---\n{yaml}---\n{body}\n");
        Ok(CompileDraft {
            id: hash(format!("{}:{cache_key}:{index}", self.config.library_id).as_bytes()),
            library_id: self.config.library_id.clone(),
            job_id: job_id.into(),
            source_id: source.source_id.clone(),
            source_revision: source.revision,
            source_hash: source.content_hash.clone(),
            target_path: target
                .map(|s| s.rel_path.clone())
                .unwrap_or_else(|| format!("{}.md", article.slug)),
            base_hash: target.map(|s| s.content_hash.clone()),
            content_hash: hash(content.as_bytes()),
            content,
            profile: profile.clone(),
            status: "draft".into(),
        })
    }

    pub fn draft(&self, id: &str) -> Result<CompileDraft> {
        self.compile_schema()?;
        let row = self.db.query_row("SELECT id,job_id,source_id,source_revision,source_hash,target_path,base_hash,content,content_hash,profile_json,status FROM compile_drafts WHERE library_id=?1 AND id=?2",params![self.config.library_id,id],|r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,u64>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?,r.get::<_,String>(9)?,r.get::<_,String>(10)?))).optional()?.ok_or_else(|| Error::NotFound("compile draft".into()))?;
        let (
            id,
            job_id,
            source_id,
            source_revision,
            source_hash,
            target_path,
            base_hash,
            content,
            content_hash,
            profile,
            status,
        ) = row;
        if hash(content.as_bytes()) != content_hash {
            return Err(Error::Conflict("stored compile draft hash mismatch".into()));
        }
        Ok(CompileDraft {
            id,
            library_id: self.config.library_id.clone(),
            job_id,
            source_id,
            source_revision,
            source_hash,
            target_path,
            base_hash,
            content,
            content_hash,
            profile: serde_json::from_str(&profile)?,
            status,
        })
    }

    /// Persist a compile/promotion draft and its sealed extra guard set atomically.
    /// A present empty set is distinct from a missing/incomplete receipt.
    pub(crate) fn persist_guarded_compile_draft(
        &self,
        draft: &CompileDraft,
        cache_key: &str,
        guards: &[DraftSourceGuard],
    ) -> Result<()> {
        self.compile_schema()?;
        if draft.library_id != self.config.library_id
            || hash(draft.content.as_bytes()) != draft.content_hash
            || guards.len() > 100
        {
            return Err(Error::Conflict(
                "invalid guarded draft scope or content".into(),
            ));
        }
        let guards_json = serde_json::to_string(guards)?;
        let tx = self.db.unchecked_transaction()?;
        insert_draft(&tx, draft, cache_key)?;
        tx.execute(
            "INSERT OR IGNORE INTO compile_draft_guards(library_id,draft_id,guards_json) VALUES (?1,?2,?3)",
            params![self.config.library_id, draft.id, guards_json],
        )?;
        let persisted: String = tx.query_row(
            "SELECT guards_json FROM compile_draft_guards WHERE library_id=?1 AND draft_id=?2",
            params![self.config.library_id, draft.id],
            |r| r.get(0),
        )?;
        if persisted != guards_json {
            return Err(Error::Conflict(
                "promotion draft evidence guards changed".into(),
            ));
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn draft_guards(&self, draft: &CompileDraft) -> Result<Vec<DraftSourceGuard>> {
        self.compile_schema()?;
        let encoded: Option<String> = self
            .db
            .query_row(
                "SELECT guards_json FROM compile_draft_guards WHERE library_id=?1 AND draft_id=?2",
                params![self.config.library_id, draft.id],
                |r| r.get(0),
            )
            .optional()?;
        let source = self.get(&draft.source_id, None, true)?;
        let Some(encoded) = encoded else {
            let document = crate::document::Document::parse(draft.content.as_bytes().to_vec())?;
            if source.kind == "note"
                || document.metadata["compile_context_policy"] == "visibility-v3"
            {
                return Err(Error::Conflict(
                    "draft is missing sealed evidence guards".into(),
                ));
            }
            return Ok(Vec::new());
        };
        if encoded.len() > 65536 {
            return Err(Error::Conflict("promotion guard set exceeds limits".into()));
        }
        let guards: Vec<DraftSourceGuard> = serde_json::from_str(&encoded)?;
        if guards.len() > 100
            || guards
                .iter()
                .any(|guard| guard.source_id.is_empty() || guard.content_hash.len() != 64)
        {
            return Err(Error::Conflict(
                "invalid promotion evidence guard set".into(),
            ));
        }
        Ok(guards)
    }

    /// Apply exactly the reviewed bytes using the shared write-intent/CAS path.
    /// A repeat after a crash or accepted application is idempotent.
    pub fn apply_draft(&mut self, id: &str, expected: Option<&str>) -> Result<CompileDraft> {
        let draft = self.draft(id)?;
        if expected != draft.base_hash.as_deref() {
            return Err(Error::Conflict(
                "draft expected hash differs from reviewed base".into(),
            ));
        }
        let target_expected = if draft.status == "applied" {
            Some(draft.content_hash.as_str())
        } else {
            expected
        };
        let extra_guards = self.draft_guards(&draft)?;
        let mut guards = vec![crate::recovery::SourceGuard {
            source_id: &draft.source_id,
            revision: draft.source_revision,
            content_hash: &draft.source_hash,
        }];
        guards.extend(
            extra_guards
                .iter()
                .map(|guard| crate::recovery::SourceGuard {
                    source_id: &guard.source_id,
                    revision: guard.revision,
                    content_hash: &guard.content_hash,
                }),
        );
        self.write_source_guarded_many(
            "compiled",
            &draft.target_path,
            draft.content.as_bytes().to_vec(),
            target_expected,
            &guards,
        )?;
        self.db.execute(
            "UPDATE compile_drafts SET status='applied' WHERE library_id=?1 AND id=?2",
            params![self.config.library_id, id],
        )?;
        self.draft(id)
    }
}

fn insert_draft(db: &rusqlite::Connection, draft: &CompileDraft, cache_key: &str) -> Result<()> {
    db.execute("INSERT OR IGNORE INTO compile_drafts(id,library_id,job_id,source_id,source_revision,source_hash,cache_key,target_path,base_hash,content,content_hash,profile_json,status) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'draft')",
        params![draft.id,draft.library_id,draft.job_id,draft.source_id,draft.source_revision,draft.source_hash,cache_key,draft.target_path,draft.base_hash,draft.content,draft.content_hash,serde_json::to_string(&draft.profile)?])?;
    Ok(())
}

pub(crate) fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty()
        || slug.len() > 240
        || slug.starts_with('.')
        || slug.ends_with('.')
        || slug.ends_with(' ')
        || slug
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
        || slug == "."
        || slug == ".."
    {
        return Err(Error::Invalid(
            "article slug must be a safe filename component".into(),
        ));
    }
    Ok(())
}

fn resolve_target<'a>(
    library: &Library,
    existing: &'a [Source],
    article: &Article,
) -> Result<Option<&'a Source>> {
    let exact = existing
        .iter()
        .filter(|s| s.slug == article.slug)
        .collect::<Vec<_>>();
    if exact.len() > 1 {
        return Err(Error::Conflict("ambiguous existing article slug".into()));
    }
    if let Some(source) = exact.first() {
        return Ok(Some(source));
    }
    let candidates = std::iter::once(article.slug.as_str())
        .chain(std::iter::once(article.title.as_str()))
        .chain(article.title.split('/').map(str::trim))
        .chain(article.aliases.iter().map(String::as_str))
        .map(|s| library.normalizer.normalize(s))
        .collect::<HashSet<_>>();
    let matches = existing
        .iter()
        .filter(|source| {
            std::iter::once(source.slug.as_str())
                .chain(std::iter::once(source.title.as_str()))
                .chain(source.title.split('/').map(str::trim))
                .chain(
                    source
                        .metadata
                        .get("aliases")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                )
                .any(|name| candidates.contains(&library.normalizer.normalize(name)))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [source] => Ok(Some(source)),
        _ => Err(Error::Conflict("ambiguous existing article alias".into())),
    }
}

fn merge_strings(metadata: &mut Value, key: &str, values: &[String]) -> Result<()> {
    let array = metadata
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("metadata must be an object".into()))?
        .entry(key)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| Error::Invalid("article list metadata has incompatible shape".into()))?;
    for value in values {
        if !array.iter().any(|old| old.as_str() == Some(value)) {
            array.push(json!(value));
        }
    }
    Ok(())
}

fn render_prompt(template: &str, values: &HashMap<&str, String>) -> String {
    let mut output = String::new();
    let mut rest = template;
    while !rest.is_empty() {
        let next = values
            .iter()
            .filter_map(|(key, value)| {
                let marker = format!("{{{key}}}");
                rest.find(&marker).map(|position| (position, marker, value))
            })
            .min_by_key(|(position, _, _)| *position);
        if let Some((position, marker, value)) = next {
            output.push_str(&rest[..position]);
            output.push_str(value);
            rest = &rest[position + marker.len()..];
        } else {
            output.push_str(rest);
            break;
        }
    }
    output
}

fn safe_compile_error(error: &Error) -> String {
    match error {
        Error::Model(model) => model.to_string(),
        Error::Invalid(_) => "invalid compile output or profile".into(),
        Error::Conflict(_) => "compile target or source conflict".into(),
        _ => "compilation storage or execution failure".into(),
    }
}
