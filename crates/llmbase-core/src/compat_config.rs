//! Configuration filesystem orchestration. Format decoders are explicit hosts;
//! native LibraryConfig uses the same reader with its native YAML decoder.
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

pub const MAX_CONFIG_BYTES: usize = 16 * 1024 * 1024;
pub const PATH_KEYS: [&str; 5] = ["raw", "wiki", "outputs", "meta", "concepts"];

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("configuration I/O at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{0}")]
    Invalid(String),
    #[error("symlink loop at {0}")]
    SymlinkLoop(PathBuf),
}

fn io_at(path: &Path, source: io::Error) -> Failure {
    Failure::Io {
        path: path.to_owned(),
        source,
    }
}

impl From<Failure> for crate::Error {
    fn from(value: Failure) -> Self {
        match value {
            Failure::Io { source, .. } => Self::Io(source),
            other => Self::Invalid(other.to_string()),
        }
    }
}

/// A missing first observation selects defaults. A later open/read failure must
/// propagate: a concurrently removed config must not silently change policy.
pub fn read_bytes(base: &Path) -> Result<Option<Vec<u8>>, Failure> {
    let path = base.join("config.yaml");
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Ok(None); // pathlib.exists ignores unrepresentable names.
    }
    match fs::metadata(&path) {
        Ok(_) => (),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None);
        }
        // pathlib's initial exists() treats a symlink loop as missing. Resolve
        // only on this exceptional path so normal reads do not add extra stats.
        Err(_) if matches!(resolve_path(&path), Err(Failure::SymlinkLoop(_))) => return Ok(None),
        Err(error) => return Err(io_at(&path, error)),
    }
    let file = fs::File::open(&path).map_err(|error| io_at(&path, error))?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_at(&path, error))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(Failure::Invalid("config.yaml exceeds 16 MiB".into()));
    }
    Ok(Some(bytes))
}

/// Non-strict config path resolution, including missing suffixes and dangling
/// links. This is not an authorization primitive; compat_io guards are separate.
pub fn resolve_path(path: &Path) -> Result<PathBuf, Failure> {
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(Failure::Invalid("embedded null byte".into()));
    }
    let current = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().map_err(|error| io_at(path, error))?
    };
    let mut active = HashSet::new();
    let mut budget = 4096_usize;
    resolve_components(current, path, &mut active, &mut budget)
}

fn resolve_components(
    mut current: PathBuf,
    path: &Path,
    active: &mut HashSet<PathBuf>,
    budget: &mut usize,
) -> Result<PathBuf, Failure> {
    for part in path.components() {
        if *budget == 0 || active.len() > 128 {
            return Err(Failure::Invalid(
                "configuration path resolution limit exceeded".into(),
            ));
        }
        *budget -= 1;
        match part {
            Component::Prefix(prefix) => current = PathBuf::from(prefix.as_os_str()),
            Component::RootDir => current.push(part.as_os_str()),
            Component::CurDir => (),
            Component::ParentDir => {
                current.pop();
            }
            Component::Normal(name) => {
                let next = current.join(name);
                match fs::symlink_metadata(&next) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        if !active.insert(next.clone()) {
                            return Err(Failure::SymlinkLoop(next));
                        }
                        let target = fs::read_link(&next).map_err(|error| io_at(&next, error))?;
                        current = resolve_components(current, &target, active, budget)?;
                        active.remove(&next);
                    }
                    Ok(_) => current = next,
                    // Non-strict realpath retains inaccessible/missing names;
                    // the eventual read/mkdir is responsible for its IO error.
                    Err(_) => current = next,
                }
            }
        }
    }
    Ok(current)
}

pub enum ReadStep<'a> {
    Defaults,
    Decode(&'a [u8]),
    Path(&'static str),
    ResolvedPath(&'static str, &'a Path),
    Base(&'a Path),
}

/// Host keeps its document opaque. Each live lookup, truth test and assignment
/// happens immediately, preserving alias identity and partial mutation on error.
/// Returns true when the decoder was used, false when defaults were used.
pub fn read_with<E: From<Failure>>(
    base: &Path,
    mut host: impl FnMut(ReadStep<'_>) -> Result<Option<PathBuf>, E>,
) -> Result<bool, E> {
    let bytes = read_bytes(base)?;
    if let Some(bytes) = &bytes {
        host(ReadStep::Decode(bytes))?;
        for key in PATH_KEYS {
            if let Some(path) = host(ReadStep::Path(key))? {
                let resolved = resolve_path(&base.join(path))?;
                host(ReadStep::ResolvedPath(key, &resolved))?;
            }
        }
    } else {
        host(ReadStep::Defaults)?;
    }
    host(ReadStep::Base(&resolve_path(base)?))?;
    Ok(bytes.is_some())
}

pub fn default_paths(base: &Path) -> Result<[(&'static str, PathBuf); 5], Failure> {
    let base = resolve_path(base)?;
    Ok([
        ("raw", base.join("raw")),
        ("wiki", base.join("wiki")),
        ("outputs", base.join("wiki/outputs")),
        ("meta", base.join("wiki/_meta")),
        ("concepts", base.join("wiki/concepts")),
    ])
}

// Match recursive mkdir's error filename as well as its partial effects. Each
// attempted ancestor is retained rather than flattening errors to the leaf.
fn mkdir_parents(path: &Path) -> Result<(), Failure> {
    let mut pending = vec![path.to_owned()];
    let mut attempts = 0_usize;
    while let Some(next) = pending.last().cloned() {
        attempts += 1;
        if attempts > 8192 {
            return Err(Failure::Invalid(
                "configuration directory depth limit exceeded".into(),
            ));
        }
        match fs::create_dir(&next) {
            Ok(()) => {
                pending.pop();
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let parent = next
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                if parent == next {
                    return Err(io_at(&next, error));
                }
                pending.push(parent.to_owned());
            }
            Err(_) if next.is_dir() => {
                pending.pop();
            }
            Err(error) => return Err(io_at(&next, error)),
        }
    }
    Ok(())
}

pub fn ensure_dirs_with<E: From<Failure>>(
    mut path_for: impl FnMut(&'static str) -> Result<PathBuf, E>,
) -> Result<(), E> {
    for key in PATH_KEYS {
        mkdir_parents(&path_for(key)?)?;
    }
    Ok(())
}
