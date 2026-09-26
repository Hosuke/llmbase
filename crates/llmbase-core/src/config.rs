use crate::{Error, Result, SourceKind};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LibraryConfig {
    pub library_id: String,
    pub base: PathBuf,
    pub raw: PathBuf,
    pub concepts: PathBuf,
    pub outputs: PathBuf,
    pub meta: PathBuf,
    pub settings: Value,
    pub chunk_chars: usize,
}

impl LibraryConfig {
    pub fn load(base: impl AsRef<Path>, library_id: impl Into<String>) -> Result<Self> {
        let library_id = library_id.into();
        validate_id(&library_id)?;
        let base = base.as_ref().canonicalize()?;
        let settings: Value = if let Some(bytes) = crate::compat_config::read_bytes(&base)? {
            let text = String::from_utf8(bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            crate::document::parse_metadata(&text)?
        } else {
            serde_json::json!({})
        };
        if !settings.is_object() {
            return Err(Error::Invalid("config must be a mapping".into()));
        }
        let resolve = |key: &str, default: &str| -> PathBuf {
            let p = settings
                .get("paths")
                .and_then(|v| v.get(key))
                .and_then(Value::as_str)
                .unwrap_or(default);
            base.join(p)
        };
        Ok(Self {
            library_id,
            raw: resolve("raw", "raw"),
            concepts: resolve("concepts", "wiki/concepts"),
            outputs: resolve("outputs", "wiki/outputs"),
            meta: resolve("meta", "wiki/_meta"),
            base,
            settings,
            chunk_chars: 800,
        })
    }
    pub fn roots(&self) -> [(SourceKind, &Path); 3] {
        [
            (SourceKind::Raw, &self.raw),
            (SourceKind::Compiled, &self.concepts),
            (SourceKind::Note, &self.outputs),
        ]
    }
    pub fn database(&self) -> PathBuf {
        self.meta.join("llmbase.sqlite3")
    }
    pub fn root_for(&self, kind: &str) -> Result<&Path> {
        match kind {
            "raw" => Ok(&self.raw),
            "compiled" => Ok(&self.concepts),
            "note" => Ok(&self.outputs),
            _ => Err(Error::Invalid("unknown source kind".into())),
        }
    }
}

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
    {
        return Err(Error::Invalid(
            "library ID must contain 1–128 ASCII letters, digits, hyphens or underscores".into(),
        ));
    }
    Ok(())
}

/// Reject traversal, symlinks and escaping ancestors. Caller supplies a trusted configured root.
pub fn scoped_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || !rel.components().all(|p| matches!(p, Component::Normal(_)))
    {
        return Err(Error::Invalid(
            "path must be a non-empty relative path".into(),
        ));
    }
    let mut path = root.to_path_buf();
    for part in rel.components() {
        path.push(part);
        if let Ok(meta) = std::fs::symlink_metadata(&path)
            && meta.file_type().is_symlink()
        {
            return Err(Error::Invalid(
                "symlinks are not accepted as source paths".into(),
            ));
        }
    }
    Ok(path)
}
