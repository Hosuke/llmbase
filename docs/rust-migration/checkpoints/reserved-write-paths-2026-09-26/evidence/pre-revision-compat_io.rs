//! Local compatibility file transport. Callers provide an explicit configured
//! library or a standalone path; this module never guesses a library ancestor.
use crate::{Error, LibraryConfig, Result, document::hash, store::WriterSession};
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

pub fn atomic_write(path: &Path, bytes: &[u8], config: Option<&LibraryConfig>) -> Result<Value> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| Error::Invalid("file path is required".into()))?;
    let destination = resolve_path(parent)?.join(name);
    if let Some(config) = config {
        let mut inside = false;
        for root in [&config.raw, &config.concepts, &config.outputs, &config.meta] {
            if destination.starts_with(resolve_path(root)?) {
                inside = true;
                break;
            }
        }
        if !inside {
            return Err(Error::Scope);
        }
        // Engine-owned and coordination artifacts are never valid destinations.
        // The destination is compared after parent symlink resolution, so an
        // alias into meta cannot bypass this. Replacing the database corrupts
        // persistent state; renaming over the held writer lock swaps its inode
        // and defeats flock coordination. The per-write spellings this module
        // generates next to every destination — `.<name>.llmbase.lock` and
        // `.llmbase-<id>.stage` — are reserved within configured roots, so a
        // direct write can never replace a live lock or collide with a stage.
        // Refusal happens before any session, directory, lock or stage is made.
        let meta = resolve_path(&config.meta)?;
        let protected = [
            meta.join("llmbase.sqlite3"),
            meta.join("llmbase.sqlite3-wal"),
            meta.join("llmbase.sqlite3-shm"),
            meta.join("llmbase.sqlite3-journal"),
            meta.join("llmbase.writer.lock"),
        ];
        let name = name.to_string_lossy();
        if protected.iter().any(|path| destination == *path)
            || (name.starts_with('.') && name.ends_with(".llmbase.lock"))
            || (name.starts_with(".llmbase-") && name.ends_with(".stage"))
        {
            return Err(Error::Scope);
        }
    }
    let _writer = config.map(WriterSession::acquire).transpose()?;
    let parent = destination.parent().unwrap();
    fs::create_dir_all(parent)?;
    let lock_path = parent.join(format!(".{}.llmbase.lock", name.to_string_lossy()));
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    FileExt::try_lock_exclusive(&lock).map_err(|e| {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            Error::Busy
        } else {
            e.into()
        }
    })?;
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
    Ok(
        json!({"path":destination,"content_hash":hash(bytes),"bytes":bytes.len(),
        "coordination":if config.is_some(){"library_writer_and_file_lock"}else{"standalone_file_lock"},
        "durability":"file_fsync_rename_directory_fsync","journal":false}),
    )
}
