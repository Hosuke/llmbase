use fs2::FileExt;
use llmbase_core::{LibraryConfig, compat_io, store::WriterSession};
use std::fs;

#[test]
fn standalone_atomic_write_is_durable_exact_and_fenced() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("new/value.json");
    let receipt = compat_io::atomic_write(&path, b"{\"v\":NaN}", None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{\"v\":NaN}");
    assert_eq!(receipt["coordination"], "standalone_file_lock");
    assert_eq!(receipt["journal"], false);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.parent().unwrap().join(".value.json.llmbase.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(matches!(
        compat_io::atomic_write(&path, b"bad", None),
        Err(llmbase_core::Error::Busy)
    ));
    assert_eq!(fs::read(&path).unwrap(), b"{\"v\":NaN}");
    FileExt::unlock(&lock).unwrap();
    compat_io::atomic_write(&path, b"replacement", None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"replacement");
    assert!(
        !fs::read_dir(path.parent().unwrap()).unwrap().any(|p| p
            .unwrap()
            .path()
            .extension()
            .is_some_and(|e| e == "stage"))
    );
}

#[test]
fn explicit_library_atomic_write_reuses_shared_writer_and_rejects_escape() {
    let temp = tempfile::tempdir().unwrap();
    let config = LibraryConfig::load(temp.path(), "fixture").unwrap();
    let path = config.meta.join("custom.json");
    let session = WriterSession::acquire(&config).unwrap();
    let receipt = compat_io::atomic_write(&path, b"{}", Some(&config)).unwrap();
    assert_eq!(receipt["coordination"], "library_writer_and_file_lock");
    // Journaled contract: the real core database stores the exact-byte intent.
    assert_eq!(receipt["journal"], true);
    assert!(
        receipt["intent_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(config.database().exists());
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&path).unwrap().permissions())
            & 0o777,
        0o600
    );
    assert!(matches!(
        compat_io::atomic_write(&temp.path().join("outside.json"), b"{}", Some(&config)),
        Err(llmbase_core::Error::Scope)
    ));
    drop(session);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(config.meta.join("llmbase.writer.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(matches!(
        compat_io::atomic_write(&path, b"bad", Some(&config)),
        Err(llmbase_core::Error::Busy)
    ));
    assert_eq!(fs::read(path).unwrap(), b"{}");
}

#[test]
fn engine_owned_and_coordination_destinations_are_scope_refused_without_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let config = LibraryConfig::load(temp.path(), "fixture").unwrap();
    fs::create_dir_all(&config.meta).unwrap();
    fs::create_dir_all(&config.concepts).unwrap();
    let mut protected = vec![config.meta.join("llmbase.writer.lock")];
    for suffix in ["", "-wal", "-shm", "-journal"] {
        protected.push(config.meta.join(format!("llmbase.sqlite3{suffix}")));
    }
    for target in &protected {
        // Existing destination: sentinel bytes and file identity survive.
        fs::write(target, b"SENTINEL").unwrap();
        #[cfg(unix)]
        let inode = std::os::unix::fs::MetadataExt::ino(&fs::metadata(target).unwrap());
        assert!(
            matches!(
                compat_io::atomic_write(target, b"bad", Some(&config)),
                Err(llmbase_core::Error::Scope)
            ),
            "{target:?}"
        );
        assert_eq!(fs::read(target).unwrap(), b"SENTINEL");
        #[cfg(unix)]
        assert_eq!(
            inode,
            std::os::unix::fs::MetadataExt::ino(&fs::metadata(target).unwrap())
        );
        // Missing destination: refused before anything is created.
        fs::remove_file(target).unwrap();
        assert!(matches!(
            compat_io::atomic_write(target, b"bad", Some(&config)),
            Err(llmbase_core::Error::Scope)
        ));
        assert!(!target.exists());
    }
    // This module's own per-write lock/stage spellings are reserved in any root.
    for target in [
        config.meta.join(".custom.json.llmbase.lock"),
        config.concepts.join(".a.md.llmbase.lock"),
        config.meta.join(".llmbase-deadbeef.stage"),
        config.concepts.join(".llmbase-0123456789abcdef.stage"),
    ] {
        assert!(matches!(
            compat_io::atomic_write(&target, b"bad", Some(&config)),
            Err(llmbase_core::Error::Scope)
        ));
        assert!(!target.exists());
    }
    // No refused operation created a session lock, per-file lock or stage.
    assert_eq!(fs::read_dir(&config.meta).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&config.concepts).unwrap().count(), 0);
    // Ordinary exact-byte writes to content/metadata roots still work.
    compat_io::atomic_write(
        &config.concepts.join("a.md"),
        b"---\n---\nbody",
        Some(&config),
    )
    .unwrap();
    let receipt = compat_io::atomic_write(
        &config.meta.join("custom.json"),
        b"{\"v\":NaN}",
        Some(&config),
    )
    .unwrap();
    assert_eq!(receipt["journal"], true);
    assert!(
        receipt["intent_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert_eq!(
        fs::read(config.meta.join("custom.json")).unwrap(),
        b"{\"v\":NaN}"
    );
}

#[test]
fn rejected_writer_lock_replacement_keeps_inode_and_fencing() {
    let temp = tempfile::tempdir().unwrap();
    let config = LibraryConfig::load(temp.path(), "fixture").unwrap();
    let session = WriterSession::acquire(&config).unwrap();
    let lock_path = config.meta.join("llmbase.writer.lock");
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&fs::metadata(&lock_path).unwrap());
    assert!(matches!(
        compat_io::atomic_write(&lock_path, b"bad", Some(&config)),
        Err(llmbase_core::Error::Scope)
    ));
    // The held lock's inode is unchanged and a second OS acquisition stays fenced.
    #[cfg(unix)]
    assert_eq!(
        inode,
        std::os::unix::fs::MetadataExt::ino(&fs::metadata(&lock_path).unwrap())
    );
    let second = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    assert!(second.try_lock_exclusive().is_err());
    drop(second);
    // The refused write created no per-file lock or stage of its own.
    assert!(
        !config
            .meta
            .join(".llmbase.writer.lock.llmbase.lock")
            .exists()
    );
    drop(session);
    let released = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    released.try_lock_exclusive().unwrap();
}

#[cfg(unix)]
#[test]
fn parent_symlink_alias_cannot_bypass_protected_paths() {
    let temp = tempfile::tempdir().unwrap();
    let config = LibraryConfig::load(temp.path(), "fixture").unwrap();
    fs::create_dir_all(&config.meta).unwrap();
    fs::write(config.database(), b"SENTINEL").unwrap();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&config.meta, &alias).unwrap();
    // The alias resolves into meta before the protected-path rule runs.
    assert!(matches!(
        compat_io::atomic_write(&alias.join("llmbase.sqlite3"), b"bad", Some(&config)),
        Err(llmbase_core::Error::Scope)
    ));
    assert!(matches!(
        compat_io::atomic_write(&alias.join("llmbase.writer.lock"), b"bad", Some(&config)),
        Err(llmbase_core::Error::Scope)
    ));
    assert_eq!(fs::read(config.database()).unwrap(), b"SENTINEL");
    // The journaled contract opens the real core database, so the sentinel
    // stub is removed once the refusal checks above have passed.
    fs::remove_file(config.database()).unwrap();
    // An ordinary write through the alias lands in meta with exact bytes.
    compat_io::atomic_write(&alias.join("custom.json"), b"{\"v\":NaN}", Some(&config)).unwrap();
    assert_eq!(
        fs::read(config.meta.join("custom.json")).unwrap(),
        b"{\"v\":NaN}"
    );
    // A final-component symlink is REFUSED for journaled library writes: both
    // the link and its external referent survive untouched, because the
    // before-image must never capture an external referent's bytes.
    let outside = temp.path().join("outside-target.json");
    fs::write(&outside, b"TARGET").unwrap();
    let link = config.meta.join("link.json");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(matches!(
        compat_io::atomic_write(&link, b"new", Some(&config)),
        Err(llmbase_core::Error::Scope)
    ));
    assert_eq!(fs::read(&outside).unwrap(), b"TARGET");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&link).unwrap(), b"TARGET");
    // Standalone writes keep the replace-the-link behavior: rename swaps the
    // link itself, so the external target keeps its bytes.
    compat_io::atomic_write(&link, b"new", None).unwrap();
    assert_eq!(fs::read(&outside).unwrap(), b"TARGET");
    assert_eq!(fs::read(&link).unwrap(), b"new");
    assert!(
        !fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn standalone_write_keeps_existing_contract_without_library_ancestor() {
    let temp = tempfile::tempdir().unwrap();
    // config=None never guesses a library ancestor and reserves no spellings.
    let path = temp.path().join("meta/llmbase.sqlite3");
    compat_io::atomic_write(&path, b"{\"v\":NaN}", None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{\"v\":NaN}");
    let artifact = temp.path().join("meta/.llmbase-deadbeef.stage");
    compat_io::atomic_write(&artifact, b"caller-managed", None).unwrap();
    assert_eq!(fs::read(&artifact).unwrap(), b"caller-managed");
}

#[test]
fn case_aliases_of_protected_names_are_scope_refused() {
    let temp = tempfile::tempdir().unwrap();
    let config = LibraryConfig::load(temp.path(), "fixture").unwrap();
    fs::create_dir_all(&config.meta).unwrap();
    fs::create_dir_all(&config.concepts).unwrap();
    fs::write(config.database(), b"SENTINEL").unwrap();
    fs::write(config.meta.join("llmbase.writer.lock"), b"LOCK").unwrap();
    // Case aliases of engine-owned basenames are refused within meta.
    for alias in [
        "LLMBASE.SQLITE3",
        "Llmbase.Sqlite3",
        "LLMBASE.SQLITE3-WAL",
        "llmbase.SQLITE3-shm",
        "LLMBASE.SQLITE3-JOURNAL",
        "LLMBASE.WRITER.LOCK",
        "Llmbase.Writer.Lock",
    ] {
        assert!(
            matches!(
                compat_io::atomic_write(&config.meta.join(alias), b"bad", Some(&config)),
                Err(llmbase_core::Error::Scope)
            ),
            "{alias}"
        );
    }
    // Coordination spellings fold case in every configured root.
    for alias in [
        config.meta.join(".CUSTOM.JSON.LLMBASE.LOCK"),
        config.concepts.join(".A.MD.LLMBASE.LOCK"),
        config.meta.join(".LLMBASE-DEADBEEF.STAGE"),
        config.concepts.join(".Llmbase-X.Stage"),
    ] {
        assert!(matches!(
            compat_io::atomic_write(&alias, b"bad", Some(&config)),
            Err(llmbase_core::Error::Scope)
        ));
        assert!(!alias.exists());
    }
    // Existing bytes survived every refusal, and no alias-named file, per-file
    // lock or stage was created.
    assert_eq!(fs::read(config.database()).unwrap(), b"SENTINEL");
    assert_eq!(
        fs::read(config.meta.join("llmbase.writer.lock")).unwrap(),
        b"LOCK"
    );
    let mut entries: Vec<_> = fs::read_dir(&config.meta)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(entries, vec!["llmbase.sqlite3", "llmbase.writer.lock"]);
    assert_eq!(fs::read_dir(&config.concepts).unwrap().count(), 0);
    // Ordinary mixed-case names that are not reserved still write fine. The
    // journaled contract opens the real core database, so the sentinel stub
    // is removed once its refusal checks above have passed.
    fs::remove_file(config.database()).unwrap();
    let receipt =
        compat_io::atomic_write(&config.meta.join("Custom.JSON"), b"{}", Some(&config)).unwrap();
    assert_eq!(receipt["journal"], true);
    assert_eq!(fs::read(config.meta.join("Custom.JSON")).unwrap(), b"{}");
    // A parent symlink alias plus a case alias still cannot reach the database.
    #[cfg(unix)]
    {
        let alias_dir = temp.path().join("case-alias");
        std::os::unix::fs::symlink(&config.meta, &alias_dir).unwrap();
        let before = fs::read(config.database()).unwrap();
        assert!(matches!(
            compat_io::atomic_write(&alias_dir.join("LLMBASE.SQLITE3"), b"bad", Some(&config)),
            Err(llmbase_core::Error::Scope)
        ));
        assert_eq!(fs::read(config.database()).unwrap(), before);
    }
}
