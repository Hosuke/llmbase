use crate::document::{CHUNKER_VERSION, Document, PARSER_VERSION, hash};
use crate::text::Normalizer;
use crate::{
    Error, Evidence, IndexReport, LibraryConfig, ResolvedEvidence, Result, SearchHit, SearchResult,
    Source,
};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Duration;
use uuid::Uuid;
use walkdir::WalkDir;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS library (id TEXT PRIMARY KEY, schema_version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS sources (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, kind TEXT NOT NULL, rel_path TEXT NOT NULL,
 slug TEXT NOT NULL, revision INTEGER NOT NULL, hash TEXT NOT NULL, fingerprint TEXT NOT NULL,
 title TEXT NOT NULL, visibility TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
 UNIQUE(library_id,kind,rel_path));
CREATE TABLE IF NOT EXISTS revisions (
 library_id TEXT NOT NULL, source_id TEXT NOT NULL, revision INTEGER NOT NULL,
 hash TEXT NOT NULL, original BLOB NOT NULL, metadata TEXT NOT NULL, body_start INTEGER NOT NULL,
 rel_path TEXT NOT NULL, captured_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 PRIMARY KEY(library_id,source_id,revision));
CREATE TABLE IF NOT EXISTS passages (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, source_id TEXT NOT NULL, revision INTEGER NOT NULL,
 start INTEGER NOT NULL, end INTEGER NOT NULL, quote TEXT NOT NULL, source_url TEXT, locator TEXT,
 active INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS active_passages ON passages(library_id,source_id,active);
CREATE VIRTUAL TABLE IF NOT EXISTS passages_fts USING fts5(passage_id UNINDEXED, title, body, tokenize='unicode61 remove_diacritics 0');
CREATE TABLE IF NOT EXISTS aliases (
 library_id TEXT NOT NULL, alias TEXT NOT NULL, source_id TEXT NOT NULL,
 PRIMARY KEY(library_id,alias,source_id));
CREATE TABLE IF NOT EXISTS tombstones (
 library_id TEXT NOT NULL, source_id TEXT NOT NULL, revision INTEGER NOT NULL,
 deleted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 PRIMARY KEY(library_id,source_id,revision));
CREATE TABLE IF NOT EXISTS write_intents (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, kind TEXT NOT NULL, relative_path TEXT NOT NULL,
 operation TEXT NOT NULL, expected_hash TEXT, content BLOB, destination TEXT,
 status TEXT NOT NULL DEFAULT 'pending',
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
-- Additive exact-byte journal for explicit-library compatibility writes.
-- before_bytes NULL means the destination was absent; an empty BLOB means an
-- existing empty file. Version-1 databases gain this table on open, untouched.
CREATE TABLE IF NOT EXISTS write_journal (
 id TEXT PRIMARY KEY, library_id TEXT NOT NULL, root TEXT NOT NULL, relative_path TEXT NOT NULL,
 before_hash TEXT, before_bytes BLOB, after_hash TEXT NOT NULL, after_bytes BLOB NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending',
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
"#;

pub struct Library {
    pub config: LibraryConfig,
    pub(crate) db: Connection,
    pub(crate) normalizer: Normalizer,
}

/// Whether a path relative to a configured content root is indexable text.
/// `.original` subtrees are opaque retained artifacts, never parsed sources.
pub fn is_source_path(relative: &Path) -> bool {
    relative
        .components()
        .all(|component| matches!(component, Component::Normal(name) if name != ".original"))
        && matches!(
            relative
                .extension()
                .and_then(|extension| extension.to_str()),
            Some("md" | "txt" | "markdown")
        )
}

struct KernelWriter {
    file: File,
    owner_pid: u32,
}
impl Drop for KernelWriter {
    fn drop(&mut self) {
        // An inherited descriptor in a forked child must not unlock its parent's lease.
        if self.owner_pid == std::process::id() {
            let _ = FileExt::unlock(&self.file);
        }
    }
}
thread_local! {
    static WRITER_SESSIONS: RefCell<HashMap<PathBuf, Weak<KernelWriter>>> = RefCell::new(HashMap::new());
}

/// Read adapters may borrow the current synchronous callback's exclusive lease.
pub fn writer_session_active(config: &LibraryConfig) -> bool {
    let Ok(meta) = config.meta.canonicalize() else {
        return false;
    };
    WRITER_SESSIONS.with(|sessions| {
        sessions
            .borrow()
            .get(&meta.join("llmbase.writer.lock"))
            .and_then(Weak::upgrade)
            .is_some_and(|lease| lease.owner_pid == std::process::id())
    })
}

pub(crate) struct WriterLock(Rc<KernelWriter>);
impl WriterLock {
    pub(crate) fn acquire(config: &LibraryConfig) -> Result<Self> {
        let path = config.meta.canonicalize()?.join("llmbase.writer.lock");
        if let Some(lease) =
            WRITER_SESSIONS.with(|sessions| sessions.borrow().get(&path).and_then(Weak::upgrade))
        {
            if lease.owner_pid != std::process::id() {
                return Err(Error::Busy);
            }
            return Ok(Self(lease));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock_exclusive().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                Error::Busy
            } else {
                e.into()
            }
        })?;
        Ok(Self(Rc::new(KernelWriter {
            file,
            owner_pid: std::process::id(),
        })))
    }
}

/// Lexical, same-thread boundary for a synchronous Python extension callback.
/// Nested native storage calls share this lease; unrelated threads/processes do not.
/// This deliberately is !Send and must never be held over an async task yield.
pub struct WriterSession {
    _lock: WriterLock,
}
impl WriterSession {
    pub fn acquire(config: &LibraryConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.meta)?;
        let lock = WriterLock::acquire(config)?;
        let path = config.meta.canonicalize()?.join("llmbase.writer.lock");
        WRITER_SESSIONS.with(|sessions| {
            sessions.borrow_mut().insert(path, Rc::downgrade(&lock.0));
        });
        Ok(Self { _lock: lock })
    }
}

struct Scanned {
    kind: String,
    relative: String,
    slug: String,
    doc: Document,
}
#[derive(Clone)]
struct Existing {
    id: String,
    kind: String,
    path: String,
    revision: u64,
    hash: String,
    fingerprint: String,
    deleted: bool,
}

impl Library {
    pub fn open(config: LibraryConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.meta)?;
        let _lock = WriterLock::acquire(&config)?;
        let mut db = Connection::open(config.database())?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        let bound: Option<(String, i64)> = tx
            .query_row("SELECT id,schema_version FROM library", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        match bound {
            Some((id, _)) if id != config.library_id => return Err(Error::Scope),
            Some((_, version)) if version != 1 => {
                return Err(Error::Capability("unsupported database schema".into()));
            }
            None => {
                tx.execute("INSERT INTO library VALUES (?1,1)", [&config.library_id])?;
            }
            _ => {}
        }
        tx.commit()?;
        let normalizer = Normalizer::new()?;
        let mut library = Self {
            config,
            db,
            normalizer,
        };
        library.recover_locked()?;
        library.recover_journal_locked()?;
        library.recover_projections_locked()?;
        Ok(library)
    }

    /// Read files once under the cooperative writer lock, then atomically publish the whole scan.
    /// A parse error aborts before any revision, deletion or index change is committed.
    pub fn sync(&mut self) -> Result<IndexReport> {
        let _lock = WriterLock::acquire(&self.config)?;
        self.recover_locked()?;
        self.recover_projections_locked()?;
        self.sync_locked()
    }

    pub(crate) fn sync_locked(&mut self) -> Result<IndexReport> {
        let mut scanned = Vec::new();
        for (kind, root) in self.config.roots() {
            if !root.exists() {
                continue;
            }
            for entry in WalkDir::new(root)
                .follow_links(false)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|entry| entry.depth() == 0 || entry.file_name() != ".original")
            {
                let entry =
                    entry.map_err(|e| Error::Invalid(format!("cannot scan source tree: {e}")))?;
                if entry.file_type().is_symlink() {
                    return Err(Error::Invalid("source tree contains a symlink".into()));
                }
                if !entry.file_type().is_file() {
                    continue;
                }
                if !is_source_path(entry.path().strip_prefix(root).map_err(|_| Error::Scope)?) {
                    continue;
                }
                if entry
                    .metadata()
                    .map_err(|e| Error::Invalid(e.to_string()))?
                    .len()
                    > 64 * 1024 * 1024
                {
                    return Err(Error::Invalid(
                        "source exceeds 64 MiB text parser limit".into(),
                    ));
                }
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| Error::Scope)?
                    .to_str()
                    .ok_or_else(|| Error::Invalid("non-UTF-8 filename".into()))?
                    .to_owned();
                let slug = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Error::Invalid("invalid source filename".into()))?
                    .to_owned();
                let doc = Document::parse(std::fs::read(entry.path())?)?;
                scanned.push(Scanned {
                    kind: kind.as_str().into(),
                    relative,
                    slug,
                    doc,
                });
            }
        }
        let scope = &self.config.library_id;
        let fingerprint = format!(
            "{PARSER_VERSION}:{CHUNKER_VERSION}:{}:opencc-t2s-0.3.1:nfkc-1",
            self.config.chunk_chars
        );
        let existing = {
            let mut stmt = self.db.prepare("SELECT id,kind,rel_path,revision,hash,fingerprint,deleted FROM sources WHERE library_id=?1")?;
            stmt.query_map([scope], |r| {
                Ok(Existing {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    path: r.get(2)?,
                    revision: r.get(3)?,
                    hash: r.get(4)?,
                    fingerprint: r.get(5)?,
                    deleted: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let by_path: HashMap<_, _> = existing
            .iter()
            .map(|e| ((e.kind.clone(), e.path.clone()), e))
            .collect();
        let present: HashSet<_> = scanned
            .iter()
            .map(|s| (s.kind.clone(), s.relative.clone()))
            .collect();
        let mut used = HashSet::new();
        let mut report = IndexReport::default();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for scanned in scanned {
            let Scanned {
                kind,
                relative,
                slug,
                doc,
            } = scanned;
            let previous = by_path
                .get(&(kind.clone(), relative.clone()))
                .copied()
                .or_else(|| {
                    let matches: Vec<_> = existing
                        .iter()
                        .filter(|e| {
                            !e.deleted
                                && e.kind == kind
                                && e.hash == doc.hash
                                && !present.contains(&(e.kind.clone(), e.path.clone()))
                                && !used.contains(&e.id)
                        })
                        .collect();
                    if matches.len() == 1 {
                        Some(matches[0])
                    } else {
                        None
                    }
                });
            let id = previous
                .map(|e| e.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            used.insert(id.clone());
            let moved = previous.is_some_and(|e| e.path != relative);
            if previous.is_some_and(|e| {
                !e.deleted && e.hash == doc.hash && e.fingerprint == fingerprint && !moved
            }) {
                report.unchanged += 1;
                continue;
            }
            let changed = previous.is_none_or(|e| e.hash != doc.hash || moved || e.deleted);
            let revision = previous
                .map(|e| e.revision + u64::from(changed))
                .unwrap_or(1);
            if previous.is_none() {
                report.added += 1;
            } else if moved {
                report.moved += 1;
            } else {
                report.updated += 1;
            }
            let title = doc.title(&slug);
            let visibility = crate::visibility::visibility(&doc.metadata);
            tx.execute("INSERT INTO sources(id,library_id,kind,rel_path,slug,revision,hash,fingerprint,title,visibility,deleted) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0) ON CONFLICT(id) DO UPDATE SET rel_path=excluded.rel_path,slug=excluded.slug,revision=excluded.revision,hash=excluded.hash,fingerprint=excluded.fingerprint,title=excluded.title,visibility=excluded.visibility,deleted=0", params![id,scope,kind,relative,slug,revision,doc.hash,fingerprint,title,visibility])?;
            tx.execute("INSERT OR IGNORE INTO revisions(library_id,source_id,revision,hash,original,metadata,body_start,rel_path) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![scope,id,revision,doc.hash,doc.original.as_bytes(),serde_json::to_string(&doc.metadata)?,doc.body_start,relative])?;
            tx.execute("DELETE FROM passages_fts WHERE passage_id IN (SELECT id FROM passages WHERE library_id=?1 AND source_id=?2 AND active=1)",params![scope,id])?;
            tx.execute(
                "UPDATE passages SET active=0 WHERE library_id=?1 AND source_id=?2",
                params![scope, id],
            )?;
            tx.execute(
                "DELETE FROM aliases WHERE library_id=?1 AND source_id=?2",
                params![scope, id],
            )?;
            let mut aliases = vec![slug.clone(), title.clone()];
            aliases.extend(title.split('/').map(str::trim).map(str::to_owned));
            for key in ["aliases", "merged_from"] {
                if let Some(values) = doc.metadata.get(key).and_then(|v| v.as_array()) {
                    aliases.extend(values.iter().filter_map(|v| v.as_str()).map(str::to_owned));
                }
            }
            for alias in &aliases {
                tx.execute(
                    "INSERT OR IGNORE INTO aliases VALUES (?1,?2,?3)",
                    params![scope, self.normalizer.normalize(alias.trim()), id],
                )?;
            }
            let title_rep =
                self.normalizer
                    .representation(&format!("{} {}", title, aliases.join(" ")));
            for (start, end) in doc.chunks(self.config.chunk_chars)? {
                let passage_id =
                    hash(format!("{scope}:{id}:{revision}:{fingerprint}:{start}:{end}").as_bytes());
                let quote = &doc.original[start..end];
                let locator = doc.original[doc.body_start..start]
                    .lines()
                    .rev()
                    .find(|l| l.starts_with('#'))
                    .map(str::to_owned);
                tx.execute("INSERT INTO passages(id,library_id,source_id,revision,start,end,quote,source_url,locator,active) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,1) ON CONFLICT(id) DO UPDATE SET active=1",params![passage_id,scope,id,revision,start,end,quote,doc.source_url(),locator])?;
                tx.execute(
                    "INSERT INTO passages_fts(passage_id,title,body) VALUES (?1,?2,?3)",
                    params![passage_id, title_rep, self.normalizer.representation(quote)],
                )?;
                report.passages_written += 1;
            }
        }
        for old in &existing {
            if !old.deleted && !used.contains(&old.id) {
                tx.execute(
                    "UPDATE sources SET deleted=1 WHERE library_id=?1 AND id=?2",
                    params![scope, old.id],
                )?;
                tx.execute("DELETE FROM passages_fts WHERE passage_id IN (SELECT id FROM passages WHERE library_id=?1 AND source_id=?2 AND active=1)",params![scope,old.id])?;
                tx.execute(
                    "UPDATE passages SET active=0 WHERE library_id=?1 AND source_id=?2",
                    params![scope, old.id],
                )?;
                tx.execute(
                    "DELETE FROM aliases WHERE library_id=?1 AND source_id=?2",
                    params![scope, old.id],
                )?;
                tx.execute("INSERT OR IGNORE INTO tombstones(library_id,source_id,revision) VALUES (?1,?2,?3)",params![scope,old.id,old.revision])?;
                report.deleted += 1;
            }
        }
        tx.commit()?;
        Ok(report)
    }

    pub fn list(&self, kind: Option<&str>, include_private: bool) -> Result<Vec<Source>> {
        let mut stmt = self.db.prepare("SELECT id FROM sources WHERE library_id=?1 AND deleted=0 AND (?2 IS NULL OR kind=?2) AND (?3 OR visibility='public') ORDER BY kind,rel_path")?;
        let ids = stmt
            .query_map(
                params![self.config.library_id, kind, include_private],
                |r| r.get::<_, String>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter()
            .map(|id| self.get(id, None, include_private))
            .collect()
    }

    pub fn get(
        &self,
        id_or_alias: &str,
        revision: Option<u64>,
        include_private: bool,
    ) -> Result<Source> {
        let direct: Option<String> = self.db.query_row("SELECT id FROM sources WHERE library_id=?1 AND id=?2 AND (?3 OR visibility='public')",params![self.config.library_id,id_or_alias,include_private],|r| r.get(0)).optional()?;
        let id = if let Some(id) = direct {
            id
        } else {
            let mut stmt = self.db.prepare("SELECT DISTINCT s.id FROM aliases a JOIN sources s ON s.id=a.source_id WHERE a.library_id=?1 AND a.alias=?2 AND s.deleted=0 AND (?3 OR s.visibility='public') ORDER BY s.id")?;
            let ids = stmt
                .query_map(
                    params![
                        self.config.library_id,
                        self.normalizer.normalize(id_or_alias.trim()),
                        include_private
                    ],
                    |r| r.get::<_, String>(0),
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            match ids.as_slice() {
                [id] => id.clone(),
                [] => return Err(Error::NotFound(id_or_alias.into())),
                _ => return Err(Error::Conflict("ambiguous alias; specify source_id".into())),
            }
        };
        if !include_private {
            self.validate_public_source_snapshot(&id)?;
        }
        let row = self.db.query_row("SELECT s.id,r.revision,r.hash,s.kind,r.rel_path,s.slug,s.title,r.metadata,r.original,r.body_start,s.deleted FROM sources s JOIN revisions r ON r.source_id=s.id AND r.library_id=s.library_id AND r.revision=COALESCE(?3,s.revision) WHERE s.library_id=?1 AND s.id=?2 AND (?4 OR s.visibility='public')",params![self.config.library_id,id,revision,include_private], |r| Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?,r.get::<_,Vec<u8>>(8)?,r.get::<_,usize>(9)?,r.get::<_,bool>(10)?))).optional()?.ok_or_else(|| Error::NotFound(id_or_alias.into()))?;
        let (
            source_id,
            revision,
            content_hash,
            kind,
            rel_path,
            _current_slug,
            _current_title,
            metadata,
            original,
            body_start,
            deleted,
        ) = row;
        let content = String::from_utf8(original)
            .map_err(|_| Error::Conflict("stored revision is corrupt".into()))?;
        let metadata: serde_json::Value = serde_json::from_str(&metadata)?;
        if !include_private && !crate::visibility::is_public(&metadata) {
            return Err(Error::NotFound(id_or_alias.into()));
        }
        // Historical reads must retain the historical filename and title, even
        // when the source was renamed or a later public revision was accepted.
        let slug = std::path::Path::new(&rel_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::Conflict("stored revision path is corrupt".into()))?
            .to_owned();
        let title = metadata
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&slug)
            .to_owned();
        Ok(Source {
            library_id: self.config.library_id.clone(),
            source_id,
            revision,
            content_hash,
            kind,
            rel_path,
            slug,
            title,
            metadata,
            content,
            body_start,
            deleted,
        })
    }

    pub fn search(
        &self,
        query: &str,
        top_k: usize,
        kind: Option<&str>,
        include_private: bool,
    ) -> Result<SearchResult> {
        if query.len() > 16_384 {
            return Err(Error::Invalid("query exceeds 16 KiB".into()));
        }
        if let Some(k) = kind {
            self.config.root_for(k)?;
        }
        let mut response = SearchResult {
            library_id: self.config.library_id.clone(),
            mode: "lexical".into(),
            degradation: Some("embedding index is not configured".into()),
            results: Vec::new(),
        };
        let fts_query = self.normalizer.fts_query(query);
        if fts_query.is_empty() || top_k == 0 {
            return Ok(response);
        }
        let mut stmt=self.db.prepare("SELECT p.id,s.id,s.slug,s.title,s.kind,p.revision,r.hash,p.start,p.end,p.quote,p.source_url,p.locator,bm25(passages_fts,0.0,4.0,1.0) AS score FROM passages_fts JOIN passages p ON p.id=passages_fts.passage_id JOIN sources s ON s.id=p.source_id JOIN revisions r ON r.library_id=p.library_id AND r.source_id=p.source_id AND r.revision=p.revision WHERE passages_fts MATCH ?1 AND p.library_id=?2 AND p.active=1 AND s.deleted=0 AND (?3 IS NULL OR s.kind=?3) AND (?4 OR s.visibility='public') ORDER BY score,p.id LIMIT ?5")?;
        response.results = stmt
            .query_map(
                params![
                    fts_query,
                    self.config.library_id,
                    kind,
                    include_private,
                    top_k.min(100)
                ],
                |r| {
                    let quote: String = r.get(9)?;
                    Ok(SearchHit {
                        source_id: r.get(1)?,
                        slug: r.get(2)?,
                        title: r.get(3)?,
                        source_kind: r.get(4)?,
                        score: -r.get::<_, f64>(12)?,
                        snippet: quote.chars().take(240).collect(),
                        evidence: Evidence {
                            library_id: self.config.library_id.clone(),
                            source_id: r.get(1)?,
                            revision: r.get(5)?,
                            content_hash: r.get(6)?,
                            passage_id: r.get(0)?,
                            start: r.get(7)?,
                            end: r.get(8)?,
                            offset_unit: "utf8_byte".into(),
                            exact_quote: quote,
                            source_url: r.get(10)?,
                            locator: r.get(11)?,
                        },
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !include_private {
            let ids: HashSet<_> = response
                .results
                .iter()
                .map(|hit| hit.source_id.as_str())
                .collect();
            for id in ids {
                self.validate_public_source_snapshot(id)?;
            }
        }
        Ok(response)
    }

    /// An external, unsynced visibility edit must not make another transport
    /// serve the previous public bytes. Deleted, recorded revisions remain
    /// resolvable according to their persisted visibility and tombstone.
    pub(crate) fn validate_public_source_snapshot(&self, source_id: &str) -> Result<()> {
        let current: Option<(String,String,bool,String)> = self.db.query_row(
            "SELECT kind,rel_path,deleted,visibility FROM sources WHERE library_id=?1 AND id=?2",
            params![self.config.library_id,source_id],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).optional()?;
        let (kind, path, deleted, visibility) =
            current.ok_or_else(|| Error::NotFound("source".into()))?;
        if visibility != "public" || !is_source_path(Path::new(&path)) {
            return Err(Error::NotFound("source".into()));
        }
        if deleted {
            return Ok(());
        }
        let path = crate::config::scoped_path(self.config.root_for(&kind)?, &path)?;
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes))
            .map_err(|_| {
                Error::Conflict("source changed since indexing; sync before reading".into())
            })?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(Error::Conflict("source exceeds public read limit".into()));
        }
        // Parse even unchanged bytes: older indexes may have interpreted YAML
        // privacy scalars differently. A matching hash cannot grant authority.
        let live = Document::parse(bytes)
            .map_err(|_| Error::Conflict("source metadata cannot be validated".into()))?;
        if !crate::visibility::is_public(&live.metadata) {
            return Err(Error::NotFound("source".into()));
        }
        Ok(())
    }

    pub fn resolve_evidence(
        &self,
        evidence: &Evidence,
        include_private: bool,
    ) -> Result<ResolvedEvidence> {
        if evidence.library_id != self.config.library_id {
            return Err(Error::Scope);
        }
        if evidence.offset_unit != "utf8_byte" {
            return Err(Error::Invalid("unsupported offset unit".into()));
        }
        let source = self.get(
            &evidence.source_id,
            Some(evidence.revision),
            include_private,
        )?;
        if evidence.start >= evidence.end
            || source.content_hash != evidence.content_hash
            || source.content.get(evidence.start..evidence.end)
                != Some(evidence.exact_quote.as_str())
        {
            return Err(Error::Conflict(
                "citation does not match original revision".into(),
            ));
        }
        let passage: Option<(usize,usize)> = self.db.query_row("SELECT start,end FROM passages WHERE library_id=?1 AND source_id=?2 AND revision=?3 AND id=?4",params![self.config.library_id,evidence.source_id,evidence.revision,evidence.passage_id],|r| Ok((r.get(0)?,r.get(1)?))).optional()?;
        if passage != Some((evidence.start, evidence.end)) {
            return Err(Error::NotFound("citation passage".into()));
        }
        let current = self.get(&evidence.source_id, None, include_private)?;
        Ok(ResolvedEvidence {
            evidence: evidence.clone(),
            stale: current.revision != evidence.revision || current.deleted,
            deleted: current.deleted,
            current_revision: current.revision,
        })
    }
}
