//! Local compatibility file transport. Callers provide an explicit configured
//! library or a standalone path; this module never guesses a library ancestor.
use crate::{Error, Library, LibraryConfig, Result, document::hash, store::WriterSession};
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub fn resolve_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Resolve existing symlinks before appending the missing suffix, matching
    // pathlib.resolve(strict=False) for compatibility roots and new files.
    if absolute.exists() {
        return Ok(absolute.canonicalize()?);
    }
    let parent = absolute
        .parent()
        .ok_or_else(|| Error::Invalid("path has no parent".into()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| Error::Invalid("path has no filename".into()))?;
    Ok(resolve_path(parent)?.join(name))
}

/// The per-file coordination lock spelled beside any destination, shared by
/// standalone writes, journaled writes and journal recovery.
pub(crate) fn file_lock_path(destination: &Path) -> Result<PathBuf> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::Invalid("file path is required".into()))?;
    let name = destination
        .file_name()
        .ok_or_else(|| Error::Invalid("file path is required".into()))?;
    Ok(parent.join(format!(".{}.llmbase.lock", name.to_string_lossy())))
}

/// Engine-owned and coordination artifacts are never valid destinations. The
/// destination is compared after parent symlink resolution, so an alias into
/// meta cannot bypass this. Replacing the database corrupts persistent state;
/// renaming over the held writer lock swaps its inode and defeats flock
/// coordination. The per-write spellings this module generates next to every
/// destination — `.<name>.llmbase.lock` and `.llmbase-<id>.stage` — are
/// reserved within configured roots, so a direct write can never replace a
/// live lock or collide with a stage. ASCII case-insensitive comparison is
/// applied uniformly on every platform: on case-insensitive filesystems
/// `LLMBASE.SQLITE3` is the same inode as `llmbase.sqlite3`, so folding case
/// reserves those aliases everywhere, including Linux CI. The spelled meta
/// parent is checked as well so recovery replay stays safe even if a stored
/// path was built before a meta symlink swap.
pub(crate) fn reserved_destination(config: &LibraryConfig, destination: &Path) -> Result<bool> {
    let meta = resolve_path(&config.meta)?;
    let Some(name) = destination.file_name() else {
        return Ok(false);
    };
    let name = name.to_string_lossy();
    let parent = destination.parent();
    let engine_owned = (parent == Some(meta.as_path()) || parent == Some(config.meta.as_path()))
        && [
            "llmbase.sqlite3",
            "llmbase.sqlite3-wal",
            "llmbase.sqlite3-shm",
            "llmbase.sqlite3-journal",
            "llmbase.writer.lock",
        ]
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved));
    let folded = name.to_ascii_lowercase();
    Ok(engine_owned
        || (folded.starts_with('.') && folded.ends_with(".llmbase.lock"))
        || (folded.starts_with(".llmbase-") && folded.ends_with(".stage")))
}

pub fn atomic_write(path: &Path, bytes: &[u8], config: Option<&LibraryConfig>) -> Result<Value> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| Error::Invalid("file path is required".into()))?;
    let destination = resolve_path(parent)?.join(name);
    let mut journal_root: Option<(&'static str, PathBuf)> = None;
    if let Some(config) = config {
        for (key, root) in [
            ("raw", &config.raw),
            ("concepts", &config.concepts),
            ("outputs", &config.outputs),
            ("meta", &config.meta),
        ] {
            let resolved = resolve_path(root)?;
            if destination.starts_with(&resolved) {
                journal_root = Some((key, resolved));
                break;
            }
        }
        if journal_root.is_none() {
            return Err(Error::Scope);
        }
        // Refusal happens before any session, directory, lock or stage is made.
        if reserved_destination(config, &destination)? {
            return Err(Error::Scope);
        }
        // Journaled writes never replace a final-component symlink: the
        // before-image would otherwise capture an external referent's bytes
        // and the rename would silently swap the link. Parent aliases may
        // still resolve into permitted roots above. Standalone writes keep
        // the replace-the-link behavior.
        if let Ok(meta) = fs::symlink_metadata(&destination)
            && meta.file_type().is_symlink()
        {
            return Err(Error::Scope);
        }
    }
    let _writer = config.map(WriterSession::acquire).transpose()?;
    // The explicit-library path journals through the real core database.
    // Opening it under the shared lease also replays any pending intents
    // before this new write is authorized.
    let library = match config {
        Some(config) => Some(Library::open(config.clone())?),
        None => None,
    };
    let parent = destination.parent().unwrap();
    fs::create_dir_all(parent)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(file_lock_path(&destination)?)?;
    FileExt::try_lock_exclusive(&lock).map_err(|e| {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            Error::Busy
        } else {
            e.into()
        }
    })?;
    // The per-file lock is held before the intent is committed, so a Busy
    // refusal can never leave a new pending write that later takes effect.
    let intent_id = match (&library, &journal_root) {
        (Some(library), Some((root_key, root_path))) => {
            if let Ok(meta) = fs::symlink_metadata(&destination)
                && meta.file_type().is_symlink()
            {
                return Err(Error::Scope);
            }
            let before = match fs::read(&destination) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            let relative = destination
                .strip_prefix(root_path)
                .map_err(|_| Error::Scope)?
                .to_str()
                .ok_or_else(|| Error::Invalid("non-UTF-8 library path cannot be journaled".into()))?
                .to_owned();
            if relative.is_empty() {
                return Err(Error::Invalid("file path is required".into()));
            }
            let intent = crate::recovery::JournalIntent {
                id: Uuid::new_v4().to_string(),
                root: (*root_key).into(),
                relative,
                before,
                after: bytes.to_vec(),
            };
            library.record_journal(&intent)?;
            Some(intent.id)
        }
        _ => None,
    };
    let temporary = parent.join(format!(".llmbase-{}.stage", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &destination)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    if let (Some(library), Some(intent_id)) = (&library, &intent_id) {
        library.mark_journal_applied(intent_id)?;
    }
    let mut receipt = json!({"path":destination,"content_hash":hash(bytes),"bytes":bytes.len(),
        "coordination":if config.is_some(){"library_writer_and_file_lock"}else{"standalone_file_lock"},
        "durability":"file_fsync_rename_directory_fsync","journal":intent_id.is_some()});
    if let Some(intent_id) = intent_id {
        receipt["intent_id"] = json!(intent_id);
    }
    Ok(receipt)
}
