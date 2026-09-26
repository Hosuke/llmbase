use llmbase_core::LibraryConfig;
use llmbase_core::compat_config::{self, Failure, MAX_CONFIG_BYTES, ReadStep};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn bounded_read_is_readonly_and_native_parser_keeps_its_contract() {
    let dir = tempdir().unwrap();
    assert!(compat_config::read_bytes(dir.path()).unwrap().is_none());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    let path = dir.path().join("config.yaml");
    fs::write(&path, "paths: {raw: custom}\nunknown: [kept, 7]\n").unwrap();
    let cfg = LibraryConfig::load(dir.path(), "config-test").unwrap();
    assert_eq!(cfg.raw, dir.path().join("custom"));
    assert_eq!(cfg.concepts, dir.path().join("wiki/concepts"));
    assert_eq!(cfg.settings["unknown"][1], 7);
    assert!(!cfg.database().exists());
    fs::write(&path, "[]").unwrap();
    assert!(LibraryConfig::load(dir.path(), "config-test").is_err());
    fs::write(&path, vec![b' '; MAX_CONFIG_BYTES]).unwrap();
    assert_eq!(
        compat_config::read_bytes(dir.path())
            .unwrap()
            .unwrap()
            .len(),
        MAX_CONFIG_BYTES
    );
    fs::write(&path, vec![b' '; MAX_CONFIG_BYTES + 1]).unwrap();
    assert!(matches!(
        compat_config::read_bytes(dir.path()),
        Err(Failure::Invalid(_))
    ));
}

#[test]
fn native_defaults_and_host_steps_are_sequential() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("missing/../library");
    let paths = compat_config::default_paths(&base).unwrap();
    assert_eq!(paths[0].1, dir.path().join("library/raw"));
    assert!(!dir.path().join("library").exists());
    let mut events = Vec::new();
    let parsed = compat_config::read_with::<Failure>(&base, |step| {
        events.push(match step {
            ReadStep::Defaults => "defaults",
            ReadStep::Base(_) => "base",
            _ => panic!("unexpected existing step"),
        });
        Ok(None)
    })
    .unwrap();
    assert!(!parsed);
    assert_eq!(events, ["defaults", "base"]);

    fs::write(dir.path().join("config.yaml"), "opaque bytes").unwrap();
    events.clear();
    assert!(
        compat_config::read_with::<Failure>(dir.path(), |step| {
            match step {
                ReadStep::Decode(bytes) => {
                    assert_eq!(bytes, b"opaque bytes");
                    events.push("decode");
                }
                ReadStep::Path(key) => {
                    events.push(key);
                    return Ok((key == "raw").then(|| PathBuf::from("missing/../raw")));
                }
                ReadStep::ResolvedPath(key, path) => {
                    assert_eq!(key, "raw");
                    assert_eq!(path, dir.path().join("raw"));
                    events.push("set-raw");
                }
                ReadStep::Base(path) => {
                    assert_eq!(path, dir.path());
                    events.push("base");
                }
                ReadStep::Defaults => panic!("not missing"),
            }
            Ok(None)
        })
        .unwrap()
    );
    assert_eq!(
        events,
        [
            "decode", "raw", "set-raw", "wiki", "outputs", "meta", "concepts", "base"
        ]
    );
}

#[test]
fn mkdir_failure_keeps_earlier_directories_and_stops_lookups() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("wiki"), "file").unwrap();
    let mut visited = Vec::new();
    let error = compat_config::ensure_dirs_with::<Failure>(|key| {
        visited.push(key);
        Ok(dir.path().join(key))
    })
    .unwrap_err();
    assert!(
        matches!(error, Failure::Io { path, source } if path == dir.path().join("wiki") && source.kind() == std::io::ErrorKind::AlreadyExists)
    );
    assert_eq!(visited, ["raw", "wiki"]);
    assert!(dir.path().join("raw").is_dir());
    assert!(!dir.path().join("outputs").exists());
}

#[cfg(unix)]
#[test]
fn symlinks_resolve_before_parent_and_missing_components() {
    use std::os::unix::fs::symlink;
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("target/deep")).unwrap();
    symlink("target/deep", dir.path().join("alias")).unwrap();
    symlink("not-here/deep", dir.path().join("dangling")).unwrap();
    symlink("loop-b", dir.path().join("loop-a")).unwrap();
    symlink("loop-a", dir.path().join("loop-b")).unwrap();
    assert_eq!(
        compat_config::resolve_path(&dir.path().join("alias/../missing/..")).unwrap(),
        dir.path().join("target")
    );
    assert_eq!(
        compat_config::resolve_path(&dir.path().join("dangling/..")).unwrap(),
        dir.path().join("not-here")
    );
    assert!(matches!(
        compat_config::resolve_path(&dir.path().join("loop-a")),
        Err(Failure::SymlinkLoop(_))
    ));
    symlink("config.yaml", dir.path().join("config.yaml")).unwrap();
    assert!(compat_config::read_bytes(dir.path()).unwrap().is_none());
}
