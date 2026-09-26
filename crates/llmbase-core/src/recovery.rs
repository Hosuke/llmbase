//! Durable file/SQLite write intents. Database commits the intent before touching a file.
//! Reopening the library completes already-authorized intents before exposing the core.
use crate::config::scoped_path;
use crate::document::{Document, hash};
use crate::store::WriterLock;
use crate::{Error, IndexReport, Library, Result};
use fs2::FileExt;
use rusqlite::params;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

struct Intent {
    id: String,
    kind: String,
    relative: String,
    operation: String,
    expected: Option<String>,
    content: Option<Vec<u8>>,
    destination: Option<String>,
}

/// Exact-byte journal intent for explicit-library compatibility writes. Unlike
/// source intents these bytes are opaque: never parsed, never indexed. `root`
/// names a configured root ("raw", "concepts", "outputs" or "meta") and
/// `relative` is the exact path beneath it; `before: None` records that the
/// destination was absent, distinct from an existing empty file.
pub(crate) struct JournalIntent {
    pub id: String,
    pub root: String,
    pub relative: String,
    pub before: Option<Vec<u8>>,
    pub after: Vec<u8>,
}

/// Revision on which an accepted derived write depends. Checked under the writer lock.
pub struct SourceGuard<'a> {
    pub source_id: &'a str,
    pub revision: u64,
    pub content_hash: &'a str,
}

impl Library {
    /// Retain an opaque original beside parsed text. The durable write-intent
    /// history contains its exact versions; originals never enter the text index.
    pub fn write_raw_asset(
        &mut self,
        relative: &str,
        bytes: Vec<u8>,
        expected: Option<&str>,
    ) -> Result<serde_json::Value> {
        if !relative.ends_with(".original") || bytes.len() > 32 * 1024 * 1024 {
            return Err(Error::Invalid(
                "opaque originals require .original and at most32 MiB".into(),
            ));
        }
        let path = scoped_path(&self.config.raw, relative)?;
        let _writer = WriterLock::acquire(&self.config)?;
        self.recover_locked()?;
        let content_hash = hash(&bytes);
        if fs::read(&path).ok().as_deref() == Some(bytes.as_slice()) {
            return Ok(
                serde_json::json!({"path":relative,"content_hash":content_hash,"unchanged":true}),
            );
        }
        check_expected(&path, expected)?;
        let intent = Intent {
            id: Uuid::new_v4().to_string(),
            kind: "raw".into(),
            relative: relative.into(),
            operation: "write_asset".into(),
            expected: expected.map(str::to_owned),
            content: Some(bytes),
            destination: None,
        };
        self.record_intent(&intent)?;
        self.recover_locked()?;
        Ok(
            serde_json::json!({"path":relative,"content_hash":content_hash,"unchanged":false,"intent_id":intent.id}),
        )
    }

    /// Create or compare-and-swap a file. None means it must not already exist.
    /// The bytes passed here are the accepted source; parsing never rewrites them.
    pub fn write_source(
        &mut self,
        kind: &str,
        relative: &str,
        bytes: Vec<u8>,
        expected: Option<&str>,
    ) -> Result<IndexReport> {
        self.write_source_checked(kind, relative, bytes, expected, None)
    }

    /// Apply reviewed bytes only while their input revision is still current on disk.
    /// Replaying the exact accepted result is idempotent, including receipt recovery.
    pub fn write_source_guarded(
        &mut self,
        kind: &str,
        relative: &str,
        bytes: Vec<u8>,
        expected: Option<&str>,
        guard: &SourceGuard<'_>,
    ) -> Result<IndexReport> {
        self.write_source_guarded_many(kind, relative, bytes, expected, std::slice::from_ref(guard))
    }

    /// As above, checking every current source used by an accepted derived write.
    pub fn write_source_guarded_many(
        &mut self,
        kind: &str,
        relative: &str,
        bytes: Vec<u8>,
        expected: Option<&str>,
        guards: &[SourceGuard<'_>],
    ) -> Result<IndexReport> {
        self.write_source_checked(kind, relative, bytes, expected, Some(guards))
    }

    fn write_source_checked(
        &mut self,
        kind: &str,
        relative: &str,
        bytes: Vec<u8>,
        expected: Option<&str>,
        guards: Option<&[SourceGuard<'_>]>,
    ) -> Result<IndexReport> {
        Document::parse(bytes.clone())?;
        let _lock = WriterLock::acquire(&self.config)?;
        self.recover_locked()?;
        for guard in guards.into_iter().flatten() {
            let source = self.get(guard.source_id, None, true)?;
            if source.deleted
                || source.revision != guard.revision
                || source.content_hash != guard.content_hash
            {
                return Err(Error::Conflict(
                    "input source changed after draft creation".into(),
                ));
            }
            let source_path = scoped_path(self.config.root_for(&source.kind)?, &source.rel_path)?;
            check_expected(&source_path, Some(guard.content_hash))?;
        }
        let path = scoped_path(self.config.root_for(kind)?, relative)?;
        if guards.is_some() && fs::read(&path).ok().as_deref() == Some(bytes.as_slice()) {
            return self.sync_locked();
        }
        check_expected(&path, expected)?;
        let intent = Intent {
            id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            relative: relative.into(),
            operation: "write".into(),
            expected: expected.map(str::to_owned),
            content: Some(bytes),
            destination: None,
        };
        self.record_intent(&intent)?;
        self.recover_locked()
    }

    pub fn delete_source(&mut self, source_id: &str, expected_hash: &str) -> Result<IndexReport> {
        let _lock = WriterLock::acquire(&self.config)?;
        self.recover_locked()?;
        let source = self.get(source_id, None, true)?;
        if source.deleted {
            return Err(Error::Conflict("source already deleted".into()));
        }
        if source.content_hash != expected_hash {
            return Err(Error::Conflict("source revision changed".into()));
        }
        let path = scoped_path(self.config.root_for(&source.kind)?, &source.rel_path)?;
        check_expected(&path, Some(expected_hash))?;
        let intent = Intent {
            id: Uuid::new_v4().to_string(),
            kind: source.kind,
            relative: source.rel_path,
            operation: "delete".into(),
            expected: Some(expected_hash.into()),
            content: None,
            destination: None,
        };
        self.record_intent(&intent)?;
        self.recover_locked()
    }

    pub fn rename_source(
        &mut self,
        source_id: &str,
        destination: &str,
        expected_hash: &str,
    ) -> Result<IndexReport> {
        let _lock = WriterLock::acquire(&self.config)?;
        self.recover_locked()?;
        let source = self.get(source_id, None, true)?;
        if source.deleted || source.content_hash != expected_hash {
            return Err(Error::Conflict("source revision changed".into()));
        }
        let root = self.config.root_for(&source.kind)?;
        let path = scoped_path(root, &source.rel_path)?;
        let dest = scoped_path(root, destination)?;
        check_expected(&path, Some(expected_hash))?;
        check_expected(&dest, None)?;
        let intent = Intent {
            id: Uuid::new_v4().to_string(),
            kind: source.kind,
            relative: source.rel_path,
            operation: "rename".into(),
            expected: Some(expected_hash.into()),
            content: Some(source.content.into_bytes()),
            destination: Some(destination.into()),
        };
        self.record_intent(&intent)?;
        self.recover_locked()
    }

    fn record_intent(&self, intent: &Intent) -> Result<()> {
        self.db.execute("INSERT INTO write_intents(id,library_id,kind,relative_path,operation,expected_hash,content,destination) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![intent.id,self.config.library_id,intent.kind,intent.relative,intent.operation,intent.expected,intent.content,intent.destination])?;
        Ok(())
    }

    /// Durably commit an exact-byte intent before its destination is replaced.
    /// Autocommit under synchronous=FULL makes the row survive a crash.
    pub(crate) fn record_journal(&self, intent: &JournalIntent) -> Result<()> {
        self.db.execute("INSERT INTO write_journal(id,library_id,root,relative_path,before_hash,before_bytes,after_hash,after_bytes) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![intent.id,self.config.library_id,intent.root,intent.relative,intent.before.as_deref().map(hash),intent.before,hash(&intent.after),intent.after])?;
        Ok(())
    }

    pub(crate) fn mark_journal_applied(&self, intent_id: &str) -> Result<()> {
        self.db.execute(
            "UPDATE write_journal SET status='applied' WHERE id=?1 AND library_id=?2",
            params![intent_id, self.config.library_id],
        )?;
        Ok(())
    }

    /// Replay pending exact-byte intents. The caller holds the shared writer
    /// lock. Stored rows are data, never trusted: scope, symlink and
    /// reserved-path rules are revalidated before any replay touches the
    /// filesystem, and each replay takes the same per-file lock as a live
    /// write. Applied rows keep both images for audit and backup.
    pub(crate) fn recover_journal_locked(&mut self) -> Result<()> {
        let intents = {
            let mut stmt = self.db.prepare("SELECT id,root,relative_path,before_bytes,after_bytes FROM write_journal WHERE library_id=?1 AND status='pending' ORDER BY created_at,rowid")?;
            stmt.query_map([&self.config.library_id], |r| {
                Ok(JournalIntent {
                    id: r.get(0)?,
                    root: r.get(1)?,
                    relative: r.get(2)?,
                    before: r.get(3)?,
                    after: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for intent in intents {
            let root = match intent.root.as_str() {
                "raw" => &self.config.raw,
                "concepts" => &self.config.concepts,
                "outputs" => &self.config.outputs,
                "meta" => &self.config.meta,
                _ => return Err(Error::Scope),
            };
            let destination = scoped_path(root, &intent.relative)?;
            if crate::compat_io::reserved_destination(&self.config, &destination)? {
                return Err(Error::Scope);
            }
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(crate::compat_io::file_lock_path(&destination)?)?;
            FileExt::try_lock_exclusive(&lock).map_err(|e| {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    Error::Busy
                } else {
                    e.into()
                }
            })?;
            let current = match fs::read(&destination) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            if current.as_deref() == Some(intent.after.as_slice()) {
                // Crash after install but before acknowledgement: the rename
                // may still lack its durability step, so fsync the file and
                // its directory before the applied status is committed.
                File::open(&destination)?.sync_all()?;
                if let Some(parent) = destination.parent() {
                    File::open(parent)?.sync_all()?;
                }
            } else if current == intent.before {
                install_journaled(&destination, &intent.after)?;
            } else {
                // An unrelated external edit wins. The pending row and both
                // captured images stay as conflict evidence; nothing is
                // discarded or acknowledged.
                return Err(Error::Conflict(format!(
                    "journaled write to {} changed outside the captured before-image",
                    intent.relative
                )));
            }
            self.mark_journal_applied(&intent.id)?;
        }
        Ok(())
    }

    pub(crate) fn recover_locked(&mut self) -> Result<IndexReport> {
        let intents = {
            let mut stmt=self.db.prepare("SELECT id,kind,relative_path,operation,expected_hash,content,destination FROM write_intents WHERE library_id=?1 AND status='pending' ORDER BY created_at,id")?;
            stmt.query_map([&self.config.library_id], |r| {
                Ok(Intent {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    relative: r.get(2)?,
                    operation: r.get(3)?,
                    expected: r.get(4)?,
                    content: r.get(5)?,
                    destination: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        if intents.is_empty() {
            return Ok(IndexReport::default());
        }
        for intent in &intents {
            let root = self.config.root_for(&intent.kind)?;
            let path = scoped_path(root, &intent.relative)?;
            match intent.operation.as_str() {
                "write" | "write_asset" => {
                    let bytes = intent
                        .content
                        .as_ref()
                        .ok_or_else(|| Error::Conflict("write intent missing content".into()))?;
                    install(&path, bytes, intent.expected.as_deref(), &intent.id)?;
                }
                "delete" => remove(&path, intent.expected.as_deref())?,
                "rename" => {
                    let dest = scoped_path(
                        root,
                        intent.destination.as_deref().ok_or_else(|| {
                            Error::Conflict("rename intent missing destination".into())
                        })?,
                    )?;
                    let bytes = intent
                        .content
                        .as_ref()
                        .ok_or_else(|| Error::Conflict("rename intent missing content".into()))?;
                    // Destination may already exist with exact accepted bytes after a crash.
                    install(&dest, bytes, None, &intent.id)?;
                    remove(&path, intent.expected.as_deref())?;
                }
                _ => return Err(Error::Conflict("unknown pending write operation".into())),
            }
        }
        let report = if intents
            .iter()
            .any(|intent| intent.operation != "write_asset")
        {
            self.sync_locked()?
        } else {
            IndexReport::default()
        };
        let tx = self.db.transaction()?;
        for intent in &intents {
            tx.execute(
                "UPDATE write_intents SET status='applied' WHERE id=?1 AND library_id=?2",
                params![intent.id, self.config.library_id],
            )?;
        }
        tx.commit()?;
        Ok(report)
    }
}

fn check_expected(path: &Path, expected: Option<&str>) -> Result<()> {
    match fs::read(path) {
        Ok(bytes) if expected == Some(hash(&bytes).as_str()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && expected.is_none() => Ok(()),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Err(Error::Conflict(
            "file changed outside the accepted revision; no overwrite performed".into(),
        )),
    }
}

fn install(path: &Path, bytes: &[u8], expected: Option<&str>, intent_id: &str) -> Result<()> {
    if let Ok(current) = fs::read(path)
        && current == bytes
    {
        return Ok(());
    }
    check_expected(path, expected)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("source path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".llmbase-{intent_id}-{}.stage", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// Stage/write/fsync/rename/directory-fsync for journaled opaque bytes. The
/// stage keeps the compatibility writer's Unix 0600 mode so a new file never
/// silently gets broader permissions than the standalone path would create.
fn install_journaled(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("journal path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".llmbase-{}.stage", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn remove(path: &Path, expected: Option<&str>) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
        _ => {}
    }
    check_expected(path, expected)?;
    fs::remove_file(path)?;
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LibraryConfig;
    use tempfile::TempDir;

    #[test]
    fn recovery_handles_before_file_and_after_file_crash_windows() {
        for after_file in [false, true] {
            let dir = TempDir::new().unwrap();
            let config = LibraryConfig::load(dir.path(), "recover").unwrap();
            let library = Library::open(config.clone()).unwrap();
            let bytes = b"immutable original source".to_vec();
            let intent = Intent {
                id: "test-intent".into(),
                kind: "raw".into(),
                relative: "a.md".into(),
                operation: "write".into(),
                expected: None,
                content: Some(bytes.clone()),
                destination: None,
            };
            library.record_intent(&intent).unwrap();
            if after_file {
                install(&config.raw.join("a.md"), &bytes, None, &intent.id).unwrap();
            }
            drop(library);
            let mut reopened = Library::open(config).unwrap();
            assert_eq!(
                reopened.get("a", None, false).unwrap().content,
                "immutable original source"
            );
            assert_eq!(reopened.sync().unwrap().unchanged, 1);
            let status: String = reopened
                .db
                .query_row("SELECT status FROM write_intents", [], |r| r.get(0))
                .unwrap();
            assert_eq!(status, "applied");
        }
    }

    #[test]
    fn accepted_writes_require_compare_and_swap_and_preserve_old_citations() {
        let dir = TempDir::new().unwrap();
        let mut library = Library::open(LibraryConfig::load(dir.path(), "write").unwrap()).unwrap();
        library
            .write_source("raw", "a.md", b"old accepted content".to_vec(), None)
            .unwrap();
        let original = library.get("a", None, false).unwrap();
        assert!(
            library
                .write_source("raw", "a.md", b"new content".to_vec(), None)
                .is_err()
        );
        library
            .write_source(
                "raw",
                "a.md",
                b"new content".to_vec(),
                Some(&original.content_hash),
            )
            .unwrap();
        let current = library.get("a", None, false).unwrap();
        assert_eq!(current.revision, 2);
        assert!(
            library
                .delete_source(&current.source_id, &original.content_hash)
                .is_err()
        );
        library
            .rename_source(&current.source_id, "renamed.md", &current.content_hash)
            .unwrap();
        library
            .delete_source(&current.source_id, &current.content_hash)
            .unwrap();
        assert!(
            library
                .get(&current.source_id, None, false)
                .unwrap()
                .deleted
        );
        assert_eq!(
            library
                .get(&current.source_id, Some(1), false)
                .unwrap()
                .content,
            "old accepted content"
        );
    }

    #[test]
    fn recovery_does_not_overwrite_an_unrelated_external_edit() {
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "conflict").unwrap();
        let library = Library::open(config.clone()).unwrap();
        let intent = Intent {
            id: "conflict".into(),
            kind: "raw".into(),
            relative: "a.md".into(),
            operation: "write".into(),
            expected: None,
            content: Some(b"accepted".to_vec()),
            destination: None,
        };
        library.record_intent(&intent).unwrap();
        fs::create_dir_all(&config.raw).unwrap();
        fs::write(config.raw.join("a.md"), "external edit").unwrap();
        drop(library);
        assert!(matches!(
            Library::open(config.clone()),
            Err(Error::Conflict(_))
        ));
        assert_eq!(
            fs::read_to_string(config.raw.join("a.md")).unwrap(),
            "external edit"
        );
    }

    #[test]
    fn journal_recovers_before_and_after_file_crash_windows_once() {
        for after_file in [false, true] {
            let dir = TempDir::new().unwrap();
            let config = LibraryConfig::load(dir.path(), "journal").unwrap();
            let library = Library::open(config.clone()).unwrap();
            let intent = JournalIntent {
                id: "crash-window".into(),
                root: "meta".into(),
                relative: "custom.json".into(),
                before: None,
                after: b"{\"v\":NaN}".to_vec(),
            };
            library.record_journal(&intent).unwrap();
            if after_file {
                // Crash after the rename but before the applied commit.
                fs::create_dir_all(&config.meta).unwrap();
                fs::write(config.meta.join("custom.json"), &intent.after).unwrap();
            }
            drop(library);
            // Recovery happens once; a repeated reopen changes nothing.
            for _ in 0..2 {
                let reopened = Library::open(config.clone()).unwrap();
                assert_eq!(
                    fs::read(config.meta.join("custom.json")).unwrap(),
                    b"{\"v\":NaN}"
                );
                let (status, count): (String, i64) = reopened
                    .db
                    .query_row(
                        "SELECT status,(SELECT count(*) FROM write_journal) FROM write_journal WHERE id='crash-window'",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .unwrap();
                assert_eq!(status, "applied");
                assert_eq!(count, 1);
                drop(reopened);
            }
        }
    }

    #[test]
    fn journaled_atomic_write_stores_exact_images_without_indexing() {
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "journal").unwrap();
        let library = Library::open(config.clone()).unwrap();
        let nonstandard = b"{\"v\":NaN}".as_slice();
        let binary_v1 = b"\xff\xfenot frontmatter".as_slice();
        let binary_v2 = b"\x00\x01\xffbinary\x02".as_slice();
        // First write: absent before-image; nonstandard JSON stays exact.
        let receipt_first = crate::compat_io::atomic_write(
            &config.meta.join("custom.json"),
            nonstandard,
            Some(&config),
        )
        .unwrap();
        assert_eq!(receipt_first["journal"], true);
        assert!(
            receipt_first["intent_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        // Invalid UTF-8 in a .md destination: exact bytes, never parsed.
        fs::create_dir_all(&config.concepts).unwrap();
        let receipt_blob = crate::compat_io::atomic_write(
            &config.concepts.join("blob.md"),
            binary_v1,
            Some(&config),
        )
        .unwrap();
        // Nonempty overwrite of the binary payload: the before-image is the
        // previous exact on-disk bytes, not an absent marker.
        let receipt_overwrite = crate::compat_io::atomic_write(
            &config.concepts.join("blob.md"),
            binary_v2,
            Some(&config),
        )
        .unwrap();
        // An existing empty file is a distinct before-image from an absent one.
        let receipt_empty =
            crate::compat_io::atomic_write(&config.meta.join("empty.bin"), b"", Some(&config))
                .unwrap();
        let receipt_filled = crate::compat_io::atomic_write(
            &config.meta.join("empty.bin"),
            b"filled",
            Some(&config),
        )
        .unwrap();
        // Every write committed its own durable intent.
        let ids: std::collections::HashSet<String> = [
            &receipt_first,
            &receipt_blob,
            &receipt_overwrite,
            &receipt_empty,
            &receipt_filled,
        ]
        .iter()
        .map(|receipt| receipt["intent_id"].as_str().unwrap().to_owned())
        .collect();
        assert_eq!(ids.len(), 5);
        #[derive(Debug, PartialEq)]
        struct JournalRow {
            before_hash: Option<String>,
            before_bytes: Option<Vec<u8>>,
            after_hash: String,
            after_bytes: Vec<u8>,
            status: String,
        }
        let rows: Vec<JournalRow> = {
            let mut stmt = library
                .db
                .prepare("SELECT before_hash,before_bytes,after_hash,after_bytes,status FROM write_journal ORDER BY rowid")
                .unwrap();
            stmt.query_map([], |r| {
                Ok(JournalRow {
                    before_hash: r.get(0)?,
                    before_bytes: r.get(1)?,
                    after_hash: r.get(2)?,
                    after_bytes: r.get(3)?,
                    status: r.get(4)?,
                })
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
        };
        assert_eq!(rows.len(), 5);
        assert_eq!(
            rows[0],
            JournalRow {
                before_hash: None,
                before_bytes: None,
                after_hash: hash(nonstandard),
                after_bytes: nonstandard.to_vec(),
                status: "applied".into()
            }
        );
        assert_eq!(
            rows[1],
            JournalRow {
                before_hash: None,
                before_bytes: None,
                after_hash: hash(binary_v1),
                after_bytes: binary_v1.to_vec(),
                status: "applied".into()
            }
        );
        assert_eq!(
            rows[2],
            JournalRow {
                before_hash: Some(hash(binary_v1)),
                before_bytes: Some(binary_v1.to_vec()),
                after_hash: hash(binary_v2),
                after_bytes: binary_v2.to_vec(),
                status: "applied".into()
            }
        );
        assert_eq!(
            rows[3],
            JournalRow {
                before_hash: None,
                before_bytes: None,
                after_hash: hash(b""),
                after_bytes: Vec::new(),
                status: "applied".into()
            }
        );
        assert_eq!(
            rows[4],
            JournalRow {
                before_hash: Some(hash(b"")),
                before_bytes: Some(Vec::new()),
                after_hash: hash(b"filled"),
                after_bytes: b"filled".to_vec(),
                status: "applied".into()
            }
        );
        // Final on-disk bytes are exactly the last accepted images.
        assert_eq!(
            fs::read(config.meta.join("custom.json")).unwrap(),
            nonstandard
        );
        assert_eq!(
            fs::read(config.concepts.join("blob.md")).unwrap(),
            binary_v2
        );
        assert_eq!(fs::read(config.meta.join("empty.bin")).unwrap(), b"filled");
        // Opaque writes never created source rows, not even for .md names.
        let sources: i64 = library
            .db
            .query_row("SELECT count(*) FROM sources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sources, 0);
    }

    #[test]
    fn journal_recovery_preserves_external_edit_and_keeps_evidence() {
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "journal").unwrap();
        let library = Library::open(config.clone()).unwrap();
        library
            .record_journal(&JournalIntent {
                id: "conflict".into(),
                root: "meta".into(),
                relative: "custom.json".into(),
                before: Some(b"v1".to_vec()),
                after: b"v2".to_vec(),
            })
            .unwrap();
        fs::create_dir_all(&config.meta).unwrap();
        fs::write(config.meta.join("custom.json"), b"external edit").unwrap();
        drop(library);
        assert!(matches!(
            Library::open(config.clone()),
            Err(Error::Conflict(_))
        ));
        assert_eq!(
            fs::read(config.meta.join("custom.json")).unwrap(),
            b"external edit"
        );
        let db = rusqlite::Connection::open(config.database()).unwrap();
        let status: String = db
            .query_row(
                "SELECT status FROM write_journal WHERE id='conflict'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "pending");
    }

    #[test]
    fn journal_busy_leaves_bytes_and_intent_count_unchanged() {
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "journal").unwrap();
        let library = Library::open(config.clone()).unwrap();
        crate::compat_io::atomic_write(&config.meta.join("custom.json"), b"v1", Some(&config))
            .unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(crate::compat_io::file_lock_path(&config.meta.join("custom.json")).unwrap())
            .unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
        assert!(matches!(
            crate::compat_io::atomic_write(&config.meta.join("custom.json"), b"v2", Some(&config)),
            Err(Error::Busy)
        ));
        assert_eq!(fs::read(config.meta.join("custom.json")).unwrap(), b"v1");
        let (pending, total): (i64, i64) = library
            .db
            .query_row(
                "SELECT count(*) FILTER (WHERE status='pending'),count(*) FROM write_journal",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((pending, total), (0, 1));
    }

    #[test]
    fn journal_replay_fails_safely_for_foreign_escaping_and_reserved_records() {
        // A record scoped to another library is never replayed by this one.
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "mine").unwrap();
        let library = Library::open(config.clone()).unwrap();
        library
            .db
            .execute(
                "INSERT INTO write_journal(id,library_id,root,relative_path,before_hash,before_bytes,after_hash,after_bytes) VALUES ('foreign','other','meta','x.json',NULL,NULL,?1,?2)",
                params![hash(b"x"), b"x".to_vec()],
            )
            .unwrap();
        drop(library);
        let reopened = Library::open(config.clone()).unwrap();
        assert!(!config.meta.join("x.json").exists());
        let status: String = reopened
            .db
            .query_row(
                "SELECT status FROM write_journal WHERE id='foreign'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "pending");
        drop(reopened);
        // Traversal out of the configured root aborts open; nothing installs.
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "escape").unwrap();
        let library = Library::open(config.clone()).unwrap();
        library
            .record_journal(&JournalIntent {
                id: "escape".into(),
                root: "meta".into(),
                relative: "../escape.txt".into(),
                before: None,
                after: b"x".to_vec(),
            })
            .unwrap();
        drop(library);
        assert!(Library::open(config.clone()).is_err());
        assert!(!dir.path().join("escape.txt").exists());
        // A stored reserved destination is refused during replay.
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "reserved").unwrap();
        let library = Library::open(config.clone()).unwrap();
        library
            .record_journal(&JournalIntent {
                id: "reserved".into(),
                root: "meta".into(),
                relative: "llmbase.sqlite3".into(),
                before: None,
                after: b"x".to_vec(),
            })
            .unwrap();
        drop(library);
        assert!(matches!(Library::open(config.clone()), Err(Error::Scope)));
    }

    #[test]
    fn journal_table_is_additive_for_existing_version_one_databases() {
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "legacy").unwrap();
        let mut library = Library::open(config.clone()).unwrap();
        library
            .write_source("raw", "a.md", b"accepted original bytes".to_vec(), None)
            .unwrap();
        let accepted = library.get("a", None, false).unwrap();
        drop(library);
        // Recreate the pre-task schema in this disposable DB: remove ONLY the
        // newly introduced journal table, leaving every prior row untouched.
        {
            let db = rusqlite::Connection::open(config.database()).unwrap();
            db.execute_batch("DROP TABLE write_journal").unwrap();
        }
        let reopened = Library::open(config.clone()).unwrap();
        let version: i64 = reopened
            .db
            .query_row("SELECT schema_version FROM library", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
        // The accepted source, revision and original bytes survived unchanged.
        let current = reopened.get("a", None, false).unwrap();
        assert_eq!(current.source_id, accepted.source_id);
        assert_eq!(current.revision, accepted.revision);
        assert_eq!(current.content_hash, accepted.content_hash);
        assert_eq!(current.content, "accepted original bytes");
        // Open re-created the additive table, and it serves an ordinary write.
        let table: i64 = reopened
            .db
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='write_journal'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table, 1);
        let receipt = crate::compat_io::atomic_write(
            &config.meta.join("custom.json"),
            b"{\"v\":NaN}",
            Some(&config),
        )
        .unwrap();
        assert_eq!(receipt["journal"], true);
        assert_eq!(
            fs::read(config.meta.join("custom.json")).unwrap(),
            b"{\"v\":NaN}"
        );
    }

    #[test]
    fn journal_recovery_respects_the_per_file_lock() {
        let dir = TempDir::new().unwrap();
        let config = LibraryConfig::load(dir.path(), "journal").unwrap();
        let library = Library::open(config.clone()).unwrap();
        let destination = config.meta.join("custom.json");
        crate::compat_io::atomic_write(&destination, b"v1", Some(&config)).unwrap();
        library
            .record_journal(&JournalIntent {
                id: "fenced".into(),
                root: "meta".into(),
                relative: "custom.json".into(),
                before: Some(b"v1".to_vec()),
                after: b"v2".to_vec(),
            })
            .unwrap();
        drop(library);
        // Holding the destination's per-file lock fences recovery: open refuses
        // Busy, the bytes stay and the same row stays pending.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(crate::compat_io::file_lock_path(&destination).unwrap())
            .unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
        assert!(matches!(Library::open(config.clone()), Err(Error::Busy)));
        assert_eq!(fs::read(&destination).unwrap(), b"v1");
        {
            let db = rusqlite::Connection::open(config.database()).unwrap();
            let status: String = db
                .query_row(
                    "SELECT status FROM write_journal WHERE id='fenced'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(status, "pending");
        }
        FileExt::unlock(&lock).unwrap();
        drop(lock);
        // Released: recovery applies the same intent once; a repeated reopen
        // changes nothing and records no duplicate history.
        for _ in 0..2 {
            let reopened = Library::open(config.clone()).unwrap();
            assert_eq!(fs::read(&destination).unwrap(), b"v2");
            let (status, count): (String, i64) = reopened
                .db
                .query_row(
                    "SELECT status,(SELECT count(*) FROM write_journal) FROM write_journal WHERE id='fenced'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(status, "applied");
            assert_eq!(count, 2);
            drop(reopened);
        }
    }
}
