//! Copy-first local administration. These filesystem paths are not remote operation arguments.
use crate::document::{Document, hash};
use crate::{Error, Library, LibraryConfig, Result};
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags, backup::Backup};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;
use walkdir::WalkDir;

const RECEIPT: &str = "migration-receipt.json";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanReport {
    pub library_id: String,
    pub files: Vec<FileRecord>,
    pub total_bytes: u64,
    pub unknown_metadata_keys: Vec<String>,
    pub parse_errors: Vec<String>,
    pub identity_conflicts: Vec<String>,
    pub native_database: bool,
    #[serde(default)]
    pub database_files: Vec<FileRecord>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MigrationReceipt {
    pub version: u32,
    pub library_id: String,
    pub source: String,
    pub source_manifest: ScanReport,
    pub state: String,
    pub destination_files: Vec<FileRecord>,
}

/// Does not create a database, lock, directory, or receipt in the original library.
pub fn inspect(config: &LibraryConfig) -> Result<ScanReport> {
    let mapped = mapped_files(config)?;
    let mut files = Vec::new();
    let mut unknown = BTreeSet::new();
    let mut parse_errors = Vec::new();
    let mut slugs = BTreeSet::new();
    let mut conflicts = Vec::new();
    for (relative, path) in mapped {
        let bytes = fs::read(&path)?;
        files.push(FileRecord {
            path: relative.clone(),
            bytes: bytes.len() as u64,
            sha256: hash(&bytes),
        });
        if (relative.starts_with("raw/")
            || relative.starts_with("wiki/concepts/")
            || relative.starts_with("wiki/outputs/"))
            && matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("md" | "txt" | "markdown")
            )
        {
            match Document::parse(bytes) {
                Ok(doc) => {
                    if let Some(map) = doc.metadata.as_object() {
                        for key in map.keys() {
                            if ![
                                "title",
                                "summary",
                                "tags",
                                "sources",
                                "source",
                                "source_url",
                                "type",
                                "compiled",
                                "ingested_at",
                                "created",
                                "updated",
                                "aliases",
                                "merged_from",
                                "visibility",
                                "private",
                                "work_id",
                            ]
                            .contains(&key.as_str())
                            {
                                unknown.insert(key.clone());
                            }
                        }
                    }
                    if relative.starts_with("wiki/concepts/") {
                        let slug = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or_default();
                        if !slugs.insert(slug.to_owned()) {
                            conflicts.push(format!("duplicate concept slug: {slug}"));
                        }
                    }
                }
                Err(e) => parse_errors.push(format!("{relative}: {e}")),
            }
        }
    }
    let mut database_files = Vec::new();
    for name in ["llmbase.sqlite3", "llmbase.sqlite3-wal"] {
        let path = config.meta.join(name);
        if path.is_file() {
            let bytes = fs::read(path)?;
            if !bytes.is_empty() {
                database_files.push(FileRecord {
                    path: name.into(),
                    bytes: bytes.len() as u64,
                    sha256: hash(&bytes),
                });
            }
        }
    }
    let total_bytes = files
        .iter()
        .chain(database_files.iter())
        .map(|f| f.bytes)
        .sum();
    Ok(ScanReport {
        library_id: config.library_id.clone(),
        files,
        total_bytes,
        unknown_metadata_keys: unknown.into_iter().collect(),
        parse_errors,
        identity_conflicts: conflicts,
        native_database: config.database().is_file(),
        database_files,
    })
}

/// Copy and index into a new owned target, retaining the old library unchanged.
/// A matching incomplete receipt allows resuming identical files without rewriting them.
pub fn copy_first(config: &LibraryConfig, target: impl AsRef<Path>) -> Result<MigrationReceipt> {
    let _lock = existing_lock(config)?;
    let source_manifest = inspect(config)?;
    if !source_manifest.parse_errors.is_empty() || !source_manifest.identity_conflicts.is_empty() {
        return Err(Error::Conflict(
            "migration dry-run reports unreadable text or identity conflicts".into(),
        ));
    }
    let target = prospective_path(target.as_ref())?;
    if target.starts_with(&config.base) || config.base.starts_with(&target) {
        return Err(Error::Invalid(
            "migration target must be separate from the original library".into(),
        ));
    }
    for root in [&config.raw, &config.concepts, &config.outputs, &config.meta] {
        let root = prospective_path(root)?;
        if target.starts_with(&root) || root.starts_with(&target) {
            return Err(Error::Invalid(
                "migration target overlaps a source root".into(),
            ));
        }
    }
    // All overlap checks precede creation, so an invalid destination cannot
    // mutate the original library, even by leaving an empty directory behind.
    fs::create_dir_all(&target)?;
    let target = target.canonicalize()?;
    let source = config.base.to_string_lossy().into_owned();
    let receipt_path = target.join(RECEIPT);
    let mut receipt = if receipt_path.exists() {
        let receipt: MigrationReceipt = serde_json::from_slice(&fs::read(&receipt_path)?)?;
        if receipt.source != source || receipt.source_manifest != source_manifest {
            return Err(Error::Conflict(
                "existing target belongs to a different source snapshot".into(),
            ));
        }
        if receipt.state == "complete" {
            verify_copy(&target)?;
            return Ok(receipt);
        }
        receipt
    } else {
        if fs::read_dir(&target)?.next().is_some() {
            return Err(Error::Conflict(
                "target is nonempty and has no migration receipt".into(),
            ));
        }
        let receipt = MigrationReceipt {
            version: 1,
            library_id: config.library_id.clone(),
            source,
            source_manifest: source_manifest.clone(),
            state: "copying".into(),
            destination_files: Vec::new(),
        };
        atomic_bytes(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
        receipt
    };
    for (relative, source_path) in mapped_files(config)? {
        let destination = crate::config::scoped_path(&target, &relative)?;
        let bytes = fs::read(source_path)?;
        if destination.exists() {
            if fs::read(&destination)? != bytes {
                return Err(Error::Conflict(format!(
                    "incomplete target file differs: {relative}"
                )));
            }
        } else {
            atomic_bytes(&destination, &bytes)?;
        }
    }
    // Preserve exact original configuration separately; only target paths are rewritten.
    let original_config = config.base.join("config.yaml");
    if original_config.exists() {
        atomic_bytes(
            &target.join("migration-original-config.yaml"),
            &fs::read(original_config)?,
        )?;
    }
    let mut settings = config.settings.clone();
    settings["paths"] = json!({"raw":"raw","wiki":"wiki","concepts":"wiki/concepts","outputs":"wiki/outputs","meta":"wiki/_meta"});
    atomic_bytes(
        &target.join("config.yaml"),
        crate::document::yaml_metadata(&settings)?.as_bytes(),
    )?;
    if config.database().exists() {
        let source_db =
            Connection::open_with_flags(config.database(), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let pending: i64 = source_db.query_row(
            "SELECT count(*) FROM write_intents WHERE status='pending'",
            [],
            |r| r.get(0),
        )?;
        if pending != 0 {
            return Err(Error::Conflict(
                "recover pending source writes before copying its database".into(),
            ));
        }
        let target_db_path = target.join("wiki/_meta/llmbase.sqlite3");
        fs::create_dir_all(target_db_path.parent().unwrap())?;
        let staging = target_db_path.with_extension(format!("{}.copy", Uuid::new_v4()));
        {
            let mut destination = Connection::open(&staging)?;
            let backup = Backup::new(&source_db, &mut destination)?;
            backup.run_to_completion(128, Duration::from_millis(5), None)?;
        }
        fs::rename(staging, target_db_path)?;
    }
    // Source verification happens before accepting target. Originals were only read.
    if inspect(config)? != source_manifest {
        return Err(Error::Conflict(
            "original library changed during copy; receipt remains resumable".into(),
        ));
    }
    {
        let mut target_library = Library::open(LibraryConfig::load(&target, &config.library_id)?)?;
        target_library.sync()?;
        target_library
            .db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    receipt.destination_files = target_records(&target)?;
    receipt.state = "complete".into();
    atomic_bytes(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    verify_copy(&target)?;
    Ok(receipt)
}

fn prospective_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => {
                resolved.pop();
            }
            _ => resolved.push(component.as_os_str()),
        }
        match fs::symlink_metadata(&resolved) {
            Ok(_) => resolved = resolved.canonicalize()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(resolved)
}

/// A backup is an immutable copy-first snapshot, including DB-only history and jobs.
pub fn backup(config: &LibraryConfig, target: impl AsRef<Path>) -> Result<MigrationReceipt> {
    copy_first(config, target)
}

pub fn verify_copy(target: impl AsRef<Path>) -> Result<MigrationReceipt> {
    let target = target.as_ref();
    let receipt: MigrationReceipt = serde_json::from_slice(&fs::read(target.join(RECEIPT))?)?;
    if receipt.version != 1 || receipt.state != "complete" {
        return Err(Error::Conflict(
            "snapshot has no completed compatible receipt".into(),
        ));
    }
    if target_records(target)? != receipt.destination_files {
        return Err(Error::Conflict(
            "snapshot files differ from their receipt".into(),
        ));
    }
    let config = LibraryConfig::load(target, &receipt.library_id)?;
    let db = Connection::open_with_flags(config.database(), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let library: String = db.query_row("SELECT id FROM library", [], |r| r.get(0))?;
    if integrity != "ok" || library != receipt.library_id {
        return Err(Error::Conflict(
            "snapshot database integrity/scope mismatch".into(),
        ));
    }
    Ok(receipt)
}

pub fn restore(snapshot: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<MigrationReceipt> {
    let snapshot = snapshot.as_ref();
    let receipt = verify_copy(snapshot)?;
    copy_first(&LibraryConfig::load(snapshot, receipt.library_id)?, target)
}

/// Rollback means returning to the preserved original; it never overwrites later data.
pub fn verify_rollback(snapshot: impl AsRef<Path>) -> Result<ScanReport> {
    let receipt = verify_copy(snapshot)?;
    let original = LibraryConfig::load(&receipt.source, &receipt.library_id)?;
    let actual = inspect(&original)?;
    if actual != receipt.source_manifest {
        return Err(Error::Conflict(
            "preserved original changed since migration; inspect before switching".into(),
        ));
    }
    Ok(actual)
}

fn mapped_files(config: &LibraryConfig) -> Result<BTreeMap<String, PathBuf>> {
    let mut result = BTreeMap::new();
    let wiki = config.base.join(
        config
            .settings
            .get("paths")
            .and_then(|v| v.get("wiki"))
            .and_then(Value::as_str)
            .unwrap_or("wiki"),
    );
    let roots = [
        ("raw", config.raw.clone()),
        ("wiki/concepts", config.concepts.clone()),
        ("wiki/outputs", config.outputs.clone()),
        ("wiki/_meta", config.meta.clone()),
        (".pipeline", config.base.join(".pipeline")),
        (".chunk_cache", config.base.join(".chunk_cache")),
        ("wiki", wiki.clone()),
    ];
    for (prefix, root) in roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(&root).follow_links(false).sort_by_file_name() {
            let entry = entry.map_err(|e| Error::Invalid(format!("scan: {e}")))?;
            if entry.file_type().is_symlink() {
                return Err(Error::Invalid("migration does not follow symlinks".into()));
            }
            if !entry.file_type().is_file() {
                continue;
            }
            if prefix == "wiki"
                && [&config.concepts, &config.outputs, &config.meta]
                    .iter()
                    .any(|r| entry.path().starts_with(r))
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if name.starts_with("llmbase.sqlite3")
                || name == "llmbase.writer.lock"
                || name.ends_with(".stage")
                || name.ends_with(".copy")
            {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&root)
                .map_err(|_| Error::Scope)?
                .to_str()
                .ok_or_else(|| Error::Invalid("non-UTF8 filename".into()))?;
            let key = format!("{prefix}/{relative}");
            if result
                .insert(key.clone(), entry.path().to_owned())
                .is_some()
            {
                return Err(Error::Conflict(format!(
                    "overlapping configured destinations: {key}"
                )));
            }
        }
    }
    if config.base.join("config.yaml").is_file() {
        result.insert(
            "migration-original-config.yaml".into(),
            config.base.join("config.yaml"),
        );
    }
    // Prior receipts and original configs are authoritative migration history,
    // including when a backup is later restored into another new directory.
    for (name, folder, extension) in [
        (RECEIPT, "migration_receipts", "json"),
        (
            "migration-original-config.yaml",
            "migration_configs",
            "yaml",
        ),
    ] {
        let path = config.base.join(name);
        if path.is_file() {
            let key = format!(
                "wiki/_meta/{folder}/{}.{}",
                hash(&fs::read(&path)?),
                extension
            );
            result.entry(key).or_insert(path);
        }
    }
    Ok(result)
}

fn target_records(target: &Path) -> Result<Vec<FileRecord>> {
    let mut records = Vec::new();
    for entry in WalkDir::new(target).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(|e| Error::Invalid(format!("verify: {e}")))?;
        if entry.file_type().is_symlink() {
            return Err(Error::Invalid("snapshot contains a symlink".into()));
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name.ends_with("-wal")
            && entry
                .metadata()
                .map_err(|e| Error::Invalid(e.to_string()))?
                .len()
                > 0
        {
            return Err(Error::Conflict(
                "snapshot has an uncheckpointed WAL; it is not immutable".into(),
            ));
        }
        if name == RECEIPT
            || name == "llmbase.writer.lock"
            || name.ends_with("-wal")
            || name.ends_with("-shm")
            || name.ends_with(".stage")
            || name.ends_with(".copy")
        {
            continue;
        }
        let bytes = fs::read(entry.path())?;
        records.push(FileRecord {
            path: entry
                .path()
                .strip_prefix(target)
                .map_err(|_| Error::Scope)?
                .to_str()
                .ok_or_else(|| Error::Invalid("non-UTF8 snapshot path".into()))?
                .into(),
            bytes: bytes.len() as u64,
            sha256: hash(&bytes),
        });
    }
    records.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(records)
}

fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("destination has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".{}.stage", Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(staging, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn existing_lock(config: &LibraryConfig) -> Result<Option<File>> {
    let path = config.meta.join("llmbase.writer.lock");
    if !path.exists() {
        return Ok(None);
    }
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    file.try_lock_exclusive().map_err(|e| {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            Error::Busy
        } else {
            e.into()
        }
    })?;
    Ok(Some(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    #[test]
    fn overlapping_destination_is_rejected_before_creating_any_directory() {
        let original = TempDir::new().unwrap();
        let cfg = LibraryConfig::load(original.path(), "overlap").unwrap();
        let target = original.path().join("new/nested/target");
        assert!(matches!(copy_first(&cfg, &target), Err(Error::Invalid(_))));
        assert!(!original.path().join("new").exists());
        assert!(!cfg.meta.exists());
    }
    #[test]
    fn dry_run_copy_verify_restore_and_rollback_preserve_unknown_fields_and_original_bytes() {
        let original = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        fs::create_dir_all(original.path().join("custom/raw")).unwrap();
        fs::write(
            original.path().join("config.yaml"),
            "paths:\n  raw: custom/raw\n  meta: custom/meta\ncustom_profile: keep\n",
        )
        .unwrap();
        let bytes = "---\ntitle: 典\nunknown: {rare: 𠀀, null: null}\n---\n學而時習之。";
        fs::write(original.path().join("custom/raw/a.md"), bytes).unwrap();
        let cfg = LibraryConfig::load(original.path(), "migration").unwrap();
        let before = inspect(&cfg).unwrap();
        assert!(!cfg.meta.exists());
        assert_eq!(before.unknown_metadata_keys, vec!["unknown"]);
        let target = container.path().join("migrated");
        copy_first(&cfg, &target).unwrap();
        verify_copy(&target).unwrap();
        assert_eq!(verify_rollback(&target).unwrap(), before);
        assert!(!cfg.meta.exists());
        assert_eq!(
            fs::read_to_string(original.path().join("custom/raw/a.md")).unwrap(),
            bytes
        );
        assert_eq!(fs::read_to_string(target.join("raw/a.md")).unwrap(), bytes);
        let restored = container.path().join("restored");
        restore(&target, &restored).unwrap();
        verify_copy(&restored).unwrap();
        let library = Library::open(LibraryConfig::load(&restored, "migration").unwrap()).unwrap();
        assert_eq!(
            library.get("典", None, false).unwrap().metadata["unknown"]["rare"],
            "𠀀"
        );
    }
    #[test]
    fn backup_keeps_deleted_revisions_jobs_and_tampering_is_detected() {
        let original = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let cfg = LibraryConfig::load(original.path(), "backup").unwrap();
        let mut library = Library::open(cfg.clone()).unwrap();
        library
            .write_source("raw", "a.md", b"original evidence".to_vec(), None)
            .unwrap();
        let source = library.get("a", None, false).unwrap();
        let job = library.enqueue_job("index", json!({}), 2).unwrap();
        library
            .delete_source(&source.source_id, &source.content_hash)
            .unwrap();
        let target = container.path().join("backup");
        backup(&cfg, &target).unwrap();
        let restored = container.path().join("restored");
        restore(&target, &restored).unwrap();
        let restored_library =
            Library::open(LibraryConfig::load(&restored, "backup").unwrap()).unwrap();
        assert!(
            restored_library
                .get(&source.source_id, None, true)
                .unwrap()
                .deleted
        );
        assert_eq!(restored_library.job(&job.id).unwrap().id, job.id);
        fs::write(target.join("corrupt-extra.md"), "unrecorded").unwrap();
        assert!(verify_copy(&target).is_err());
    }
}
