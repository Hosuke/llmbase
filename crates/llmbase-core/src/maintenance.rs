//! Deterministic maintenance, read exports, and recoverable legacy projections.
use crate::{
    Error, Library, Result, Source, config::scoped_path, document::hash, profile::LanguageProfile,
    recovery::SourceGuard, store::WriterLock,
};
use ferrous_opencc::{OpenCC, config::BuiltinConfig};
use regex::Regex;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};
use uuid::Uuid;

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const PROJECTION_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS maintenance_projection_batches (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, status TEXT NOT NULL,
 artifacts TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')));
"#;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LintOptions {
    pub allow_cjk_slugs: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FixOptions {
    pub normalize_tags: bool,
    pub rebuild_metadata: bool,
}
impl Default for FixOptions {
    fn default() -> Self {
        Self {
            normalize_tags: true,
            rebuild_metadata: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Artifact {
    file: String,
    content: String,
    expected: Option<String>,
    #[serde(default)]
    guards: Vec<ProjectionGuard>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ProjectionGuard {
    source_id: String,
    revision: u64,
    content_hash: String,
}

pub(crate) struct GeneratedMetadata<'a> {
    pub file: &'a str,
    pub previous: Option<&'a [u8]>,
    pub next: &'a Value,
}

struct Snapshot {
    sources: Vec<Source>,
    aliases: BTreeMap<String, BTreeSet<usize>>,
    legacy_aliases: BTreeMap<String, String>,
    outgoing: BTreeMap<usize, BTreeSet<usize>>,
    backlinks: BTreeMap<usize, BTreeSet<usize>>,
    unresolved: Vec<(usize, String)>,
}

impl Library {
    pub fn export_article(
        &self,
        slug: &str,
        profile: &LanguageProfile,
        include_private: bool,
    ) -> Result<Value> {
        profile.validate()?;
        let _lock = WriterLock::acquire(&self.config)?;
        let snapshot = Snapshot::load(self, include_private)?;
        let index = snapshot
            .resolve(self, slug)?
            .ok_or_else(|| Error::NotFound("article".into()))?;
        Ok(snapshot.export_article(index, profile))
    }

    pub fn export_tag(&self, tag: &str, include_private: bool) -> Result<Value> {
        if tag.len() > 1024 {
            return Err(Error::Invalid("tag too long".into()));
        }
        let _lock = WriterLock::acquire(&self.config)?;
        let snapshot = Snapshot::load(self, include_private)?;
        let articles: Vec<_> = snapshot
            .sources
            .iter()
            .filter(|source| {
                tags(source)
                    .iter()
                    .any(|t| t.to_lowercase() == tag.to_lowercase())
            })
            .map(index_entry)
            .collect();
        Ok(json!({"tag":tag,"count":articles.len(),"articles":articles}))
    }

    pub fn export_graph(&self, slug: &str, depth: usize, include_private: bool) -> Result<Value> {
        if depth > 4 {
            return Err(Error::Invalid("export graph depth must be 0..4".into()));
        }
        let _lock = WriterLock::acquire(&self.config)?;
        let snapshot = Snapshot::load(self, include_private)?;
        let root = snapshot
            .resolve(self, slug)?
            .ok_or_else(|| Error::NotFound("graph root".into()))?;
        let mut queue = VecDeque::from([(root, 0)]);
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        let mut truncated = false;
        while let Some((index, level)) = queue.pop_front() {
            if seen.contains(&index) {
                continue;
            }
            if seen.len() >= 1000 {
                truncated = true;
                break;
            }
            seen.insert(index);
            let source = &snapshot.sources[index];
            nodes.push(json!({"slug":source.slug,"title":source.title,"summary":source.metadata.get("summary").cloned().unwrap_or(json!("")),"tags":tags(source).into_iter().filter(|t|!t.starts_with("category:")).collect::<Vec<_>>(),"depth":level}));
            if level < depth {
                let neighbors: BTreeSet<_> = snapshot
                    .outgoing
                    .get(&index)
                    .into_iter()
                    .flatten()
                    .chain(snapshot.backlinks.get(&index).into_iter().flatten())
                    .copied()
                    .collect();
                if neighbors.len() > 200 {
                    truncated = true;
                }
                for neighbor in neighbors.into_iter().take(200) {
                    if !seen.contains(&neighbor) {
                        queue.push_back((neighbor, level + 1));
                    }
                }
            }
        }
        Ok(
            json!({"root":snapshot.sources[root].slug,"depth":depth,"count":nodes.len(),"nodes":nodes,"truncated":truncated}),
        )
    }

    /// Project all currently public articles. Owner credentials never make
    /// unscoped legacy/static files include private source content.
    pub fn rebuild_metadata(&mut self) -> Result<Value> {
        self.rebuild_metadata_inner().map(|(report, _)| report)
    }

    /// Legacy Python rebuild_index returns the real projected entry array.
    pub fn rebuild_metadata_compat(&mut self) -> Result<Value> {
        self.rebuild_metadata_inner()
            .map(|(_, entries)| json!(entries))
    }

    /// Preserve the legacy exact-key response while filtering current visibility.
    pub fn legacy_backlinks(&self, slug: &str, include_private: bool) -> Result<Value> {
        if !scoped_path(&self.config.meta, "backlinks.json")?.is_file() {
            return Ok(
                json!({"slug":slug,"cited_by":[],"note":"run `llmbase compile index` first"}),
            );
        }
        let snapshot = Snapshot::load(self, include_private)?;
        let backlinks = snapshot.legacy_backlinks(self)?;
        Ok(json!({"slug":slug,"cited_by":backlinks.get(slug).cloned().unwrap_or_default()}))
    }

    /// The legacy _build_backlinks helper updates only its own projection.
    pub fn rebuild_backlinks_compat(&mut self) -> Result<Value> {
        self.sync()?;
        let _lock = WriterLock::acquire(&self.config)?;
        self.recover_projections_locked()?;
        let snapshot = Snapshot::load(self, false)?;
        let backlinks = snapshot.legacy_backlinks(self)?;
        let path = scoped_path(&self.config.meta, "backlinks.json")?;
        let artifact = Artifact {
            file: "backlinks.json".into(),
            content: serde_json::to_string_pretty(&backlinks)?,
            expected: read_bounded(&path, MAX_BYTES)?.map(|bytes| hash(&bytes)),
            guards: Vec::new(),
        };
        self.db.execute_batch(PROJECTION_SCHEMA)?;
        self.db.execute("INSERT INTO maintenance_projection_batches(id,library_id,status,artifacts) VALUES (?1,?2,'pending',?3)",params![Uuid::new_v4().to_string(),self.config.library_id,serde_json::to_string(&vec![artifact])?])?;
        self.recover_projections_locked()?;
        Ok(json!(backlinks))
    }

    /// Private compatibility state, never served as a public projection.
    pub(crate) fn write_compile_sources_compat(&mut self, keys: &[String]) -> Result<()> {
        let _lock = WriterLock::acquire(&self.config)?;
        self.recover_projections_locked()?;
        let path = scoped_path(&self.config.meta, "compiled_sources.json")?;
        let content = serde_json::to_string_pretty(keys)?;
        if content.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid("compiled source log exceeds 64 MiB".into()));
        }
        let artifact = Artifact {
            file: "compiled_sources.json".into(),
            content,
            expected: read_bounded(&path, MAX_BYTES)?.map(|bytes| hash(&bytes)),
            guards: Vec::new(),
        };
        self.db.execute_batch(PROJECTION_SCHEMA)?;
        self.db.execute("INSERT INTO maintenance_projection_batches(id,library_id,status,artifacts) VALUES (?1,?2,'pending',?3)",params![Uuid::new_v4().to_string(),self.config.library_id,serde_json::to_string(&vec![artifact])?])?;
        self.recover_projections_locked()?;
        Ok(())
    }

    /// Caller holds the shared writer lock. Trail content is accepted user data;
    /// preserve both its pre-edit bytes and each new revision in the same SQLite
    /// transaction as the durable filesystem intent.
    pub(crate) fn write_trails_compat_locked(
        &mut self,
        previous: Option<&[u8]>,
        next: &Value,
    ) -> Result<()> {
        let content = serde_json::to_string_pretty(next)?;
        if content.len() > 4 * 1024 * 1024 {
            return Err(Error::Invalid("trails exceed 4 MiB".into()));
        }
        let artifact = Artifact {
            file: "trails.json".into(),
            content,
            expected: previous.map(hash),
            guards: Vec::new(),
        };
        let path = scoped_path(&self.config.meta, "trails.json")?;
        if read_bounded(&path, 4 * 1024 * 1024)?.as_deref() != previous {
            return Err(Error::Conflict(
                "trails changed before journal persistence".into(),
            ));
        }
        self.db.execute_batch(PROJECTION_SCHEMA)?;
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS maintenance_trail_revisions (library_id TEXT NOT NULL,revision INTEGER NOT NULL,original BLOB NOT NULL,hash TEXT NOT NULL,PRIMARY KEY(library_id,revision))")?;
        let mut revision: u64 = self.db.query_row(
            "SELECT COALESCE(max(revision),0) FROM maintenance_trail_revisions WHERE library_id=?1",
            [&self.config.library_id],
            |r| r.get(0),
        )?;
        let tx = self.db.transaction()?;
        if let Some(previous) = previous {
            revision += 1;
            tx.execute(
                "INSERT INTO maintenance_trail_revisions VALUES (?1,?2,?3,?4)",
                params![self.config.library_id, revision, previous, hash(previous)],
            )?;
        }
        revision += 1;
        tx.execute(
            "INSERT INTO maintenance_trail_revisions VALUES (?1,?2,?3,?4)",
            params![
                self.config.library_id,
                revision,
                artifact.content.as_bytes(),
                hash(artifact.content.as_bytes())
            ],
        )?;
        tx.execute("INSERT INTO maintenance_projection_batches(id,library_id,status,artifacts) VALUES (?1,?2,'pending',?3)",params![Uuid::new_v4().to_string(),self.config.library_id,serde_json::to_string(&vec![artifact])?])?;
        tx.commit()?;
        self.recover_projections_locked()?;
        Ok(())
    }

    pub(crate) fn maintenance_article_aliases(
        &self,
        include_private: bool,
    ) -> Result<BTreeMap<String, String>> {
        let _lock = WriterLock::acquire(&self.config)?;
        Ok(Snapshot::load(self, include_private)?.legacy_aliases)
    }

    pub(crate) fn maintenance_resolve_article(
        &self,
        name: &str,
        include_private: bool,
    ) -> Result<Option<String>> {
        let _lock = WriterLock::acquire(&self.config)?;
        let snapshot = Snapshot::load(self, include_private)?;
        Ok(snapshot
            .resolve(self, name)?
            .map(|index| snapshot.sources[index].slug.clone()))
    }

    fn rebuild_metadata_inner(&mut self) -> Result<(Value, Vec<Value>)> {
        let indexed = self.sync()?;
        let _lock = WriterLock::acquire(&self.config)?;
        self.recover_projections_locked()?;
        let snapshot = Snapshot::load(self, false)?;
        let entries: Vec<_> = snapshot.sources.iter().map(index_entry).collect();
        let updated: String =
            self.db
                .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
                    r.get(0)
                })?;
        let mut markdown = format!(
            "---\ntitle: Wiki Index\nupdated: {updated}\n---\n\n# Knowledge Base Index\n\n"
        );
        let mut groups: BTreeMap<String, Vec<&Source>> = BTreeMap::new();
        for source in &snapshot.sources {
            let values = tags(source);
            if values.is_empty() {
                groups
                    .entry("uncategorized".into())
                    .or_default()
                    .push(source);
            } else {
                for tag in values {
                    groups.entry(tag).or_default().push(source);
                }
            }
        }
        for (tag, articles) in groups {
            markdown.push_str(&format!("\n## {}\n\n", title_case(&tag)));
            for source in articles {
                markdown.push_str(&format!(
                    "- [[{}|{}]] — {}\n",
                    source.slug,
                    source.title,
                    source
                        .metadata
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                ));
            }
        }
        markdown.push_str(&format!("\n\n---\n*{} articles indexed*\n", entries.len()));
        let backlinks = snapshot.legacy_backlinks(self)?;
        let generation = Uuid::new_v4().to_string();
        let mut artifacts = Vec::new();
        for (file, content) in [
            ("index.json", serde_json::to_string_pretty(&entries)?),
            ("_index.md", markdown),
            (
                "aliases.json",
                serde_json::to_string_pretty(&snapshot.legacy_aliases)?,
            ),
            ("backlinks.json", serde_json::to_string_pretty(&backlinks)?),
        ] {
            let path = scoped_path(&self.config.meta, file)?;
            let expected = read_bounded(&path, MAX_BYTES)?.map(|bytes| hash(&bytes));
            artifacts.push(Artifact {
                file: file.into(),
                content,
                expected,
                guards: Vec::new(),
            });
        }
        let manifest = json!({"version":1,"generation":generation,"visibility":"public","article_count":entries.len(),"files":artifacts.iter().map(|a|(&a.file,hash(a.content.as_bytes()))).collect::<BTreeMap<_,_>>()});
        let manifest_path = scoped_path(&self.config.meta, "projection-generation.json")?;
        artifacts.push(Artifact {
            file: "projection-generation.json".into(),
            content: serde_json::to_string_pretty(&manifest)?,
            expected: read_bounded(&manifest_path, MAX_BYTES)?.map(|b| hash(&b)),
            guards: Vec::new(),
        });
        self.db.execute_batch(PROJECTION_SCHEMA)?;
        self.db.execute("INSERT INTO maintenance_projection_batches(id,library_id,status,artifacts) VALUES (?1,?2,'pending',?3)",params![generation,self.config.library_id,serde_json::to_string(&artifacts)?])?;
        self.recover_projections_locked()?;
        let indexed_total: u64 = self.db.query_row(
            "SELECT count(*) FROM sources WHERE library_id=?1 AND kind='compiled' AND deleted=0",
            [&self.config.library_id],
            |r| r.get(0),
        )?;
        Ok((
            json!({"article_count":entries.len(),"indexed_article_count":indexed_total,"projection_visibility":"public","generation":generation,"index_report":indexed,"files":["index.json","_index.md","aliases.json","backlinks.json"]}),
            entries,
        ))
    }

    /// Called by Library::open while its existing writer lock is held.
    /// Also safe to replay before a new projection generation is authorized.
    pub(crate) fn recover_projections_locked(&mut self) -> Result<usize> {
        let exists=self.db.query_row("SELECT 1 FROM sqlite_master WHERE type='table' AND name='maintenance_projection_batches'",[],|_|Ok(())).optional()?.is_some();
        if !exists {
            return Ok(0);
        }
        let pending = {
            let mut stmt=self.db.prepare("SELECT id,artifacts FROM maintenance_projection_batches WHERE library_id=?1 AND status='pending' ORDER BY created_at,id")?;
            stmt.query_map([&self.config.library_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for (id, payload) in &pending {
            let artifacts: Vec<Artifact> = serde_json::from_str(payload)?;
            let standalone = artifacts.len() == 1
                && matches!(
                    artifacts[0].file.as_str(),
                    "backlinks.json" | "compiled_sources.json" | "trails.json"
                );
            let generated_batch = (1..=2).contains(&artifacts.len())
                && artifacts.iter().all(|a| generated_file(&a.file))
                && artifacts
                    .iter()
                    .map(|a| &a.file)
                    .collect::<HashSet<_>>()
                    .len()
                    == artifacts.len();
            let full_batch = artifacts.len() == 5
                && artifacts.last().map(|a| a.file.as_str()) == Some("projection-generation.json")
                && artifacts
                    .iter()
                    .map(|a| a.file.as_str())
                    .collect::<HashSet<_>>()
                    .len()
                    == 5
                && artifacts.iter().all(|a| {
                    matches!(
                        a.file.as_str(),
                        "index.json"
                            | "_index.md"
                            | "aliases.json"
                            | "backlinks.json"
                            | "projection-generation.json"
                    )
                });
            if !(standalone || full_batch || generated_batch) {
                return Err(Error::Conflict("invalid projection batch".into()));
            }
            for artifact in &artifacts {
                if (!generated_file(&artifact.file)
                    && ![
                        "index.json",
                        "_index.md",
                        "aliases.json",
                        "backlinks.json",
                        "projection-generation.json",
                        "compiled_sources.json",
                        "trails.json",
                    ]
                    .contains(&artifact.file.as_str()))
                    || artifact.content.len() as u64 > MAX_BYTES
                {
                    return Err(Error::Conflict("invalid projection artifact".into()));
                }
                for guard in &artifact.guards {
                    let current = self.get(&guard.source_id, None, true)?;
                    let source_path =
                        scoped_path(self.config.root_for(&current.kind)?, &current.rel_path)?;
                    if current.deleted
                        || current.revision != guard.revision
                        || current.content_hash != guard.content_hash
                        || read_bounded(&source_path, MAX_BYTES)?
                            .as_ref()
                            .map(|bytes| hash(bytes))
                            != Some(guard.content_hash.clone())
                    {
                        return Err(Error::Conflict(
                            "generated metadata source changed before publication".into(),
                        ));
                    }
                }
                let path = scoped_path(&self.config.meta, &artifact.file)?;
                install_projection(&path, artifact, id)?;
            }
            self.db.execute("UPDATE maintenance_projection_batches SET status='complete' WHERE library_id=?1 AND id=?2",params![self.config.library_id,id])?;
        }
        Ok(pending.len())
    }

    /// Fixed generated metadata families only. Caller holds the writer lock and
    /// revalidates scope/locked human data before submitting this durable intent.
    pub(crate) fn write_generated_metadata_locked(
        &mut self,
        file: &str,
        previous: Option<&[u8]>,
        next: &Value,
        sources: &[Source],
    ) -> Result<()> {
        self.write_generated_metadata_batch_locked(
            &[GeneratedMetadata {
                file,
                previous,
                next,
            }],
            sources,
        )
    }

    pub(crate) fn write_generated_metadata_batch_locked(
        &mut self,
        batch: &[GeneratedMetadata<'_>],
        sources: &[Source],
    ) -> Result<()> {
        if !(1..=2).contains(&batch.len())
            || batch.iter().map(|a| a.file).collect::<HashSet<_>>().len() != batch.len()
        {
            return Err(Error::Invalid(
                "generated batch requires one or two distinct files".into(),
            ));
        }
        let mut artifacts = Vec::new();
        for item in batch {
            if !generated_file(item.file) {
                return Err(Error::Invalid("invalid generated metadata file".into()));
            }
            let content = serde_json::to_string_pretty(item.next)?;
            if content.len() > 4 * 1024 * 1024 {
                return Err(Error::Invalid("generated metadata exceeds 4 MiB".into()));
            }
            let path = scoped_path(&self.config.meta, item.file)?;
            if read_bounded(&path, 4 * 1024 * 1024)?.as_deref() != item.previous {
                return Err(Error::Conflict("metadata changed during generation".into()));
            }
            artifacts.push(Artifact {
                file: item.file.into(),
                content,
                expected: item.previous.map(hash),
                guards: sources
                    .iter()
                    .map(|s| ProjectionGuard {
                        source_id: s.source_id.clone(),
                        revision: s.revision,
                        content_hash: s.content_hash.clone(),
                    })
                    .collect(),
            });
        }
        self.db.execute_batch(PROJECTION_SCHEMA)?;
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS maintenance_generated_revisions(library_id TEXT NOT NULL,file TEXT NOT NULL,revision INTEGER NOT NULL,original BLOB NOT NULL,hash TEXT NOT NULL,PRIMARY KEY(library_id,file,revision))")?;
        let tx = self.db.transaction()?;
        for (item, artifact) in batch.iter().zip(&artifacts) {
            let file = item.file;
            let mut revision:u64=tx.query_row("SELECT COALESCE(max(revision),0) FROM maintenance_generated_revisions WHERE library_id=?1 AND file=?2",params![self.config.library_id,file],|r|r.get(0))?;
            if let Some(previous) = item.previous {
                revision += 1;
                tx.execute(
                    "INSERT INTO maintenance_generated_revisions VALUES (?1,?2,?3,?4,?5)",
                    params![
                        self.config.library_id,
                        file,
                        revision,
                        previous,
                        hash(previous)
                    ],
                )?;
            }
            revision += 1;
            tx.execute(
                "INSERT INTO maintenance_generated_revisions VALUES (?1,?2,?3,?4,?5)",
                params![
                    self.config.library_id,
                    file,
                    revision,
                    artifact.content.as_bytes(),
                    hash(artifact.content.as_bytes())
                ],
            )?;
        }
        tx.execute("INSERT INTO maintenance_projection_batches(id,library_id,status,artifacts) VALUES (?1,?2,'pending',?3)",params![Uuid::new_v4().to_string(),self.config.library_id,serde_json::to_string(&artifacts)?])?;
        tx.commit()?;
        self.recover_projections_locked()?;
        Ok(())
    }

    pub fn lint(&self, options: LintOptions, include_private: bool) -> Result<Value> {
        let taxonomy = self.taxonomy("en", include_private)?;
        let _lock = WriterLock::acquire(&self.config)?;
        let snapshot = Snapshot::load(self, include_private)?;
        if snapshot.sources.len() > 2000 {
            return Err(Error::Capability(
                "pairwise duplicate lint is bounded to 2000 visible articles".into(),
            ));
        }
        let mut checks: BTreeMap<&str, Vec<String>> = [
            "structural",
            "broken_links",
            "orphans",
            "missing_metadata",
            "dirty_tags",
            "duplicates",
            "stubs",
            "uncategorized",
        ]
        .into_iter()
        .map(|name| (name, Vec::new()))
        .collect();
        if !self.config.meta.join("_index.md").is_file() {
            checks
                .get_mut("structural")
                .unwrap()
                .push("Missing master index file (_index.md)".into());
        }
        if !self.config.meta.join("index.json").is_file() {
            checks
                .get_mut("structural")
                .unwrap()
                .push("Missing JSON index (index.json)".into());
        }
        if snapshot.sources.is_empty() {
            checks
                .get_mut("structural")
                .unwrap()
                .push("No articles in the wiki".into());
        }
        for (index, target) in &snapshot.unresolved {
            checks.get_mut("broken_links").unwrap().push(format!(
                "Broken link in {}: [[{target}]]",
                snapshot.sources[*index].slug
            ));
        }
        if !self.config.meta.join("backlinks.json").is_file() {
            checks
                .get_mut("orphans")
                .unwrap()
                .push("Backlinks map not built yet".into());
        } else {
            for (index, source) in snapshot.sources.iter().enumerate() {
                if snapshot
                    .backlinks
                    .get(&index)
                    .is_none_or(BTreeSet::is_empty)
                {
                    checks.get_mut("orphans").unwrap().push(format!(
                        "Orphan article (no incoming links): {}",
                        source.slug
                    ));
                }
            }
        }
        for source in &snapshot.sources {
            for field in ["title", "summary", "tags"] {
                if source.metadata.get(field).is_none_or(empty_value) {
                    checks
                        .get_mut("missing_metadata")
                        .unwrap()
                        .push(format!("Missing {field}: {}", source.slug));
                }
            }
            if let Some(values) = source.metadata.get("tags").and_then(Value::as_array) {
                let dirty: Vec<_> = values
                    .iter()
                    .filter(|tag| dirty_tag(tag))
                    .map(|tag| {
                        tag.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| tag.to_string())
                    })
                    .collect();
                if !dirty.is_empty() {
                    checks
                        .get_mut("dirty_tags")
                        .unwrap()
                        .push(format!("Dirty tags in {}: {dirty:?}", source.slug));
                }
            }
            let title = source
                .metadata
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&source.slug);
            let summary = source
                .metadata
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("");
            let content = source.content[source.body_start..].trim();
            let stub = if title.contains("English Title / 中文标题")
                || summary.contains("One-line summary in English")
            {
                Some(format!("Unfilled template: {}", source.slug))
            } else if title.chars().all(|c| matches!(c, '.' | '/' | ' ')) {
                Some(format!("Empty/garbage title: {}", source.slug))
            } else if [
                "has not been fully written",
                "has not been written yet",
                "尚未完成撰写",
            ]
            .iter()
            .any(|s| content.contains(s))
            {
                Some(format!("Placeholder stub: {}", source.slug))
            } else if [
                "the user says",
                "the user wants",
                "the user asks",
                "the user is",
            ]
            .iter()
            .any(|s| summary.to_lowercase().contains(s))
            {
                Some(format!("LLM prompt leak: {}", source.slug))
            } else if !options.allow_cjk_slugs
                && !source.slug.is_empty()
                && source.slug.chars().all(is_cjk)
            {
                Some(format!("CJK slug (should be pinyin): {}", source.slug))
            } else if content.chars().count() < 50
                && !source.metadata.get("stub").is_some_and(|v| !empty_value(v))
            {
                Some(format!(
                    "Near-empty article: {} ({} chars)",
                    source.slug,
                    content.chars().count()
                ))
            } else {
                None
            };
            if let Some(issue) = stub {
                checks.get_mut("stubs").unwrap().push(issue);
            }
        }
        for (i, a) in snapshot.sources.iter().enumerate() {
            for b in &snapshot.sources[i + 1..] {
                if duplicate_candidate(self, a, b) {
                    checks
                        .get_mut("duplicates")
                        .unwrap()
                        .push(format!("Likely duplicate: {} <-> {}", a.slug, b.slug));
                }
            }
        }
        collect_uncategorized(
            &taxonomy["categories"],
            checks.get_mut("uncategorized").unwrap(),
        );
        let total = checks.values().map(Vec::len).sum::<usize>();
        let mut result = serde_json::to_value(checks)?;
        result["total_issues"] = json!(total);
        Ok(result)
    }

    /// Safe deterministic subset: format/deduplicate existing string tags only.
    /// Content generation, deletion, and semantic merging remain separate work.
    pub fn fix_deterministic(
        &mut self,
        options: FixOptions,
        include_private: bool,
    ) -> Result<Value> {
        let snapshot = {
            let _lock = WriterLock::acquire(&self.config)?;
            Snapshot::load(self, include_private).map_err(|error| match error {
                Error::NotFound(_) => Error::Conflict(
                    "source visibility changed before deterministic maintenance".into(),
                ),
                error => error,
            })?
        };
        let mut fixes = Vec::new();
        let mut failures = Vec::new();
        if options.normalize_tags {
            for source in &snapshot.sources {
                let Some(old) = source.metadata.get("tags").and_then(Value::as_array) else {
                    continue;
                };
                let mut seen = HashSet::new();
                let mut cleaned = Vec::new();
                for tag in old {
                    if let Some(text) = tag.as_str() {
                        let tag = text.trim().to_lowercase();
                        if !tag.is_empty() && seen.insert(tag.clone()) {
                            cleaned.push(json!(tag));
                        }
                    } else {
                        cleaned.push(tag.clone());
                    }
                }
                if &cleaned == old {
                    continue;
                }
                let mut metadata = source.metadata.clone();
                metadata["tags"] = json!(cleaned);
                let bytes = format!(
                    "---\n{}---\n{}",
                    crate::document::yaml_metadata(&metadata)?,
                    &source.content[source.body_start..]
                )
                .into_bytes();
                let guard = SourceGuard {
                    source_id: &source.source_id,
                    revision: source.revision,
                    content_hash: &source.content_hash,
                };
                match self.write_source_guarded("compiled",&source.rel_path,bytes,Some(&source.content_hash),&guard){
                Ok(_)=>fixes.push(format!("Normalized tag formatting for: {}",source.slug)),
                Err(_)=>failures.push(json!({"source_id":source.source_id,"message":"source changed or journaled write failed; original bytes were not overwritten without matching revision"})),
            }
            }
        }
        let mut projection = None;
        if options.rebuild_metadata {
            match self.rebuild_metadata() {
                Ok(report) => projection = Some(report),
                Err(_) => failures.push(json!({"message":"metadata projection rebuild failed"})),
            }
        }
        let status = if failures.is_empty() {
            "succeeded"
        } else if fixes.is_empty() {
            return Err(Error::Conflict(
                "deterministic maintenance failed before applying tag edits".into(),
            ));
        } else {
            "partial"
        };
        Ok(
            json!({"status":status,"fix_count":fixes.len(),"fixes":fixes,"failures":failures,"projection":projection,"scope":"deterministic_tag_formatting"}),
        )
    }

    pub fn xici(&self, lang: &str, include_private: bool) -> Result<Value> {
        if lang.is_empty()
            || lang.len() > 64
            || !lang
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(Error::Invalid("invalid introduction language code".into()));
        }
        let _lock = WriterLock::acquire(&self.config)?;
        let path = scoped_path(&self.config.meta, &format!("xici-{lang}.json"))?;
        let Some(bytes) = read_bounded(&path, 4 * 1024 * 1024)? else {
            return Ok(
                json!({"text":"","themes":[],"lang":lang,"generated_at":null,"article_count":0,"origin":"not_generated"}),
            );
        };
        let mut cached: Value = serde_json::from_slice(&bytes)?;
        if !cached.is_object()
            || !cached.get("text").is_some_and(Value::is_string)
            || !cached.get("themes").is_some_and(Value::is_array)
        {
            return Err(Error::Invalid("invalid introduction cache".into()));
        }
        if !include_private {
            if !crate::visibility::is_public(&cached) {
                return Err(Error::NotFound("public introduction".into()));
            }
            if let Some(ids) = cached.get("source_ids").and_then(Value::as_array) {
                if ids.is_empty() {
                    return Err(Error::Capability(
                        "cached introduction has no verifiable public provenance".into(),
                    ));
                }
                let unique: HashSet<_> = ids.iter().filter_map(Value::as_str).collect();
                if unique.len() != ids.len()
                    || cached.get("article_count").and_then(Value::as_u64) != Some(ids.len() as u64)
                {
                    return Err(Error::Conflict(
                        "introduction provenance and article count disagree".into(),
                    ));
                }
                for id in ids {
                    let source = self.get(
                        id.as_str().ok_or_else(|| {
                            Error::Invalid("invalid introduction source identity".into())
                        })?,
                        None,
                        false,
                    )?;
                    if source.deleted {
                        return Err(Error::Conflict(
                            "introduction references a deleted source".into(),
                        ));
                    }
                }
            } else {
                // Legacy caches lack per-field evidence. A library that has
                // contained private compiled material cannot publish that text.
                let hidden:u64=self.db.query_row("SELECT count(*) FROM revisions r JOIN sources s ON s.library_id=r.library_id AND s.id=r.source_id WHERE r.library_id=?1 AND s.kind='compiled' AND ((json_type(r.metadata,'$.private') IS NOT NULL AND json_type(r.metadata,'$.private') NOT IN ('null','false')) OR (json_type(r.metadata,'$.visibility') IS NOT NULL AND (json_type(r.metadata,'$.visibility')!='text' OR json_extract(r.metadata,'$.visibility')!='public')))",[&self.config.library_id],|r|r.get(0))?;
                if hidden > 0 {
                    return Err(Error::Capability("legacy introduction cache lacks provenance for a library with private history".into()));
                }
            }
        }
        cached["origin"] = json!("legacy_cache");
        Ok(cached)
    }
}

impl Snapshot {
    fn legacy_backlinks(&self, library: &Library) -> Result<BTreeMap<String, Vec<String>>> {
        let mut backlinks: BTreeMap<String, Vec<String>> = self
            .backlinks
            .iter()
            .map(|(target, incoming)| {
                (
                    self.sources[*target].slug.clone(),
                    incoming
                        .iter()
                        .map(|i| self.sources[*i].slug.clone())
                        .collect(),
                )
            })
            .collect();
        // Preserve genuinely unknown target keys for legacy lint/stub callers;
        // do not expose known private targets or silently resolve ambiguities.
        if !self.unresolved.is_empty() {
            let complete = Self::load(library, true)?;
            for (incoming, raw_target) in &self.unresolved {
                if !matches!(complete.resolve(library, raw_target), Ok(None)) {
                    continue;
                }
                let values = backlinks
                    .entry(raw_target.to_lowercase().replace(' ', "-"))
                    .or_default();
                let incoming = self.sources[*incoming].slug.clone();
                if !values.contains(&incoming) {
                    values.push(incoming);
                }
            }
        }
        Ok(backlinks)
    }

    fn load(library: &Library, include_private: bool) -> Result<Self> {
        let (count,bytes):(u64,u64)=library.db.query_row("SELECT count(*),COALESCE(sum(length(r.original)),0) FROM sources s JOIN revisions r ON r.library_id=s.library_id AND r.source_id=s.id AND r.revision=s.revision WHERE s.library_id=?1 AND s.kind='compiled' AND s.deleted=0 AND (?2 OR s.visibility='public')",params![library.config.library_id,include_private],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if count > 20_000 || bytes > MAX_BYTES {
            return Err(Error::Capability(
                "maintenance read exceeds 20000 visible articles or64 MiB".into(),
            ));
        }
        let sources = library.list(Some("compiled"), include_private)?;
        let mut aliases: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut legacy_aliases = BTreeMap::new();
        let mut by_slug: HashMap<String, Vec<usize>> = HashMap::new();
        let traditional = OpenCC::from_config(BuiltinConfig::S2t)
            .map_err(|_| Error::Capability("CJK alias dictionary unavailable".into()))?;
        for (index, source) in sources.iter().enumerate() {
            by_slug.entry(source.slug.clone()).or_default().push(index);
            let mut names = vec![source.slug.clone(), source.title.clone()];
            names.extend(source.title.split('/').map(str::trim).map(str::to_owned));
            for key in ["aliases", "merged_from"] {
                if let Some(items) = source.metadata.get(key).and_then(Value::as_array) {
                    names.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            }
            for name in names {
                for name in [
                    name.trim().to_lowercase(),
                    library.normalizer.normalize(name.trim()),
                    traditional.convert(name.trim()).to_lowercase(),
                ] {
                    if name.is_empty() {
                        continue;
                    }
                    aliases
                        .entry(library.normalizer.normalize(&name))
                        .or_default()
                        .insert(index);
                    legacy_aliases
                        .entry(name)
                        .or_insert_with(|| source.slug.clone());
                }
            }
            aliases
                .entry(source.source_id.clone())
                .or_default()
                .insert(index);
        }
        let path = scoped_path(&library.config.meta, "aliases.json")?;
        if let Some(bytes) = read_bounded(&path, 4 * 1024 * 1024)? {
            let existing: BTreeMap<String, String> = serde_json::from_slice(&bytes)?;
            for (name, target) in existing {
                if let Some(indices) = by_slug.get(&target).filter(|indices| indices.len() == 1) {
                    // Existing aliases can be human overrides, not disposable cache.
                    aliases.insert(
                        library.normalizer.normalize(name.trim()),
                        BTreeSet::from([indices[0]]),
                    );
                    legacy_aliases.insert(name, target);
                }
            }
        }
        let mut snapshot = Self {
            sources,
            aliases,
            legacy_aliases,
            outgoing: BTreeMap::new(),
            backlinks: BTreeMap::new(),
            unresolved: Vec::new(),
        };
        let links = link_pattern()?;
        for (index, source) in snapshot.sources.iter().enumerate() {
            for capture in links.captures_iter(&source.content[source.body_start..]) {
                let target = capture[1].trim();
                match snapshot.resolve(library, target) {
                    Ok(Some(target)) => {
                        snapshot.outgoing.entry(index).or_default().insert(target);
                        snapshot.backlinks.entry(target).or_default().insert(index);
                    }
                    _ => snapshot.unresolved.push((index, target.into())),
                }
            }
        }
        Ok(snapshot)
    }
    fn resolve(&self, library: &Library, name: &str) -> Result<Option<usize>> {
        let key = library.normalizer.normalize(name.trim());
        for key in [&key, &key.replace(' ', "-"), &key.replace(' ', "")] {
            if let Some(indices) = self.aliases.get(key) {
                return if indices.len() == 1 {
                    Ok(indices.first().copied())
                } else {
                    Err(Error::Conflict("ambiguous article alias".into()))
                };
            }
        }
        Ok(None)
    }
    fn export_article(&self, index: usize, profile: &LanguageProfile) -> Value {
        let source = &self.sources[index];
        let sections = profile.split(&source.content[source.body_start..]);
        let mut content = BTreeMap::new();
        for section in &profile.sections {
            if let Some(text) = sections
                .get(&section.key)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let key = match section.key.as_str() {
                    "中文" => "zh",
                    "日本語" => "ja",
                    key if key.eq_ignore_ascii_case("english") => "english",
                    key => key,
                };
                content.insert(key.to_owned(), text.to_owned());
            }
        }
        let my_tags: BTreeSet<_> = tags(source)
            .into_iter()
            .filter(|t| !t.starts_with("category:"))
            .map(|t| t.to_lowercase())
            .collect();
        let mut related = Vec::new();
        for (other_index, other) in self.sources.iter().enumerate() {
            if other_index == index {
                continue;
            }
            let shared: BTreeSet<_> = tags(other)
                .into_iter()
                .filter(|t| !t.starts_with("category:"))
                .map(|t| t.to_lowercase())
                .filter(|t| my_tags.contains(t))
                .collect();
            if !shared.is_empty() {
                related.push(json!({"slug":other.slug,"title":other.title,"shared_tags":shared}));
            }
        }
        related.sort_by(|a, b| {
            b["shared_tags"]
                .as_array()
                .unwrap()
                .len()
                .cmp(&a["shared_tags"].as_array().unwrap().len())
                .then(a["slug"].as_str().cmp(&b["slug"].as_str()))
        });
        related.truncate(10);
        let short = |index: &usize| json!({"slug":self.sources[*index].slug,"title":self.sources[*index].title});
        json!({"slug":source.slug,"title":source.title,"summary":source.metadata.get("summary").cloned().unwrap_or(json!("")),"tags":tags(source).into_iter().filter(|t|!t.starts_with("category:")).collect::<Vec<_>>(),"sources":source.metadata.get("sources").cloned().unwrap_or(json!([])),"created":source.metadata.get("created").cloned().unwrap_or(json!("")),"updated":source.metadata.get("updated").cloned().unwrap_or(json!("")),"content":content,"backlinks":self.backlinks.get(&index).into_iter().flatten().map(short).collect::<Vec<_>>(),"outgoing_links":self.outgoing.get(&index).into_iter().flatten().map(short).collect::<Vec<_>>(),"related_by_tags":related})
    }
}

fn read_bounded(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    match fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(meta) => {
            if !meta.is_file() || meta.len() > limit {
                return Err(Error::Invalid(
                    "metadata file exceeds size bound or is not a file".into(),
                ));
            }
            Ok(Some(fs::read(path)?))
        }
    }
}
fn generated_file(file: &str) -> bool {
    matches!(
        file,
        "taxonomy.json"
            | "taxonomy-private.json"
            | "health.json"
            | "health-private.json"
            | "last_fix.json"
            | "last_fix-private.json"
            | "entities.json"
            | "entities.private.json"
    ) || file
        .strip_prefix("xici-")
        .and_then(|name| name.strip_suffix(".json"))
        .is_some_and(|lang| {
            !lang.is_empty()
                && lang.len() <= 80
                && lang
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        })
}
fn install_projection(path: &Path, artifact: &Artifact, generation: &str) -> Result<()> {
    let current = read_bounded(path, MAX_BYTES)?;
    if current.as_deref() == Some(artifact.content.as_bytes()) {
        return Ok(());
    }
    if current.as_ref().map(|bytes| hash(bytes)) != artifact.expected {
        return Err(Error::Conflict(
            "projection changed after journaled batch; no overwrite performed".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("projection path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let staged = parent.join(format!(".projection-{generation}-{}.stage", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)?;
    file.write_all(artifact.content.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(staged, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn index_entry(source: &Source) -> Value {
    json!({"slug":source.slug,"title":source.title,"summary":source.metadata.get("summary").cloned().unwrap_or(json!("")),"tags":source.metadata.get("tags").cloned().unwrap_or(json!([])),"sources":source.metadata.get("sources").cloned().unwrap_or(json!([]))})
}
fn tags(source: &Source) -> Vec<String> {
    source
        .metadata
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
fn title_case(value: &str) -> String {
    let mut boundary = true;
    let mut result = String::new();
    for c in value.chars() {
        if boundary {
            result.extend(c.to_uppercase());
        } else {
            result.extend(c.to_lowercase());
        }
        boundary = !c.is_alphabetic();
    }
    result
}
fn link_pattern() -> Result<Regex> {
    Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]")
        .map_err(|_| Error::Invalid("wiki-link parser".into()))
}
fn empty_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(v) => !*v,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Number(n) => n.as_i64() == Some(0),
    }
}
pub(crate) fn dirty_tag(value: &Value) -> bool {
    value.as_str().is_none_or(|tag| {
        tag.chars().count() > 40
            || tag.split_whitespace().count() > 4
            || [
                "we need",
                "the user",
                "output",
                "list ",
                "tags:",
                "tag1",
                "tag2",
                "interpret",
                "based on",
            ]
            .iter()
            .any(|p| tag.to_lowercase().contains(p))
    })
}
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x9fff)
}
pub(crate) fn duplicate_candidate(library: &Library, a: &Source, b: &Source) -> bool {
    let mut score = 0;
    let cjk_slug = a.slug.chars().any(is_cjk) || b.slug.chars().any(is_cjk);
    if (cjk_slug && (a.slug.contains(&b.slug) || b.slug.contains(&a.slug)))
        || (!cjk_slug
            && ((a.slug.chars().count() >= 4 && b.slug.contains(&a.slug))
                || (b.slug.chars().count() >= 4 && a.slug.contains(&b.slug))))
    {
        score += 2;
    }
    let a_tags: HashSet<_> = tags(a).into_iter().map(|t| t.to_lowercase()).collect();
    let b_tags: HashSet<_> = tags(b).into_iter().map(|t| t.to_lowercase()).collect();
    let union = a_tags.union(&b_tags).count();
    if union > 0 && a_tags.intersection(&b_tags).count() as f64 / union as f64 >= 0.6 {
        score += 2;
    }
    let names = |source: &Source| {
        source
            .title
            .split('/')
            .chain(std::iter::once(source.slug.as_str()))
            .map(|s| {
                library
                    .normalizer
                    .normalize(s)
                    .chars()
                    .filter(|c| is_cjk(*c))
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    };
    let a_names = names(a);
    let b_names = names(b);
    if a_names.iter().any(|a| {
        b_names.iter().any(|b| {
            a == b || {
                let (short, long) = if a.chars().count() <= b.chars().count() {
                    (a, b)
                } else {
                    (b, a)
                };
                short.chars().count() > 1
                    && long.contains(short)
                    && short.chars().count() as f64 / long.chars().count() as f64 >= 0.67
            }
        })
    }) {
        score += 3;
    }
    score >= 2
}
fn collect_uncategorized(nodes: &Value, issues: &mut Vec<String>) {
    if let Some(nodes) = nodes.as_array() {
        for node in nodes {
            if node["id"] == "other"
                && let Some(articles) = node["articles"].as_array()
            {
                for article in articles {
                    if let Some(slug) = article["slug"].as_str() {
                        issues.push(format!("Uncategorized article: {slug}"));
                    }
                }
            }
            collect_uncategorized(&node["children"], issues);
        }
    }
}
