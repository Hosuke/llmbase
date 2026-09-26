//! Native knowledge-library application core. No interpreter or Python dependency.
pub mod answers;
pub mod application;
pub mod ask;
pub mod catalog;
pub mod compat_compile;
pub mod compat_config;
pub mod compat_io;
pub mod compat_kb;
pub mod compat_maintenance;
pub mod compat_pipeline;
pub mod compat_query;
pub mod compat_resolve;
pub mod compat_search;
pub mod compat_taxonomy;
pub mod compat_util;
pub mod compat_workflows;
pub mod compat_yaml;
pub mod compile;
pub mod config;
pub mod document;
pub mod entities_model;
pub mod fetch;
pub mod ingest;
pub mod jobs;
pub mod maintenance;
pub mod maintenance_model;
pub mod maintenance_repair;
pub mod migration;
pub mod plugins;
pub mod profile;
pub mod recovery;
pub mod store;
pub mod text;
pub mod transcription;
pub mod vector;
pub mod visibility;
pub mod web_read;
pub mod web_write;
pub mod worker;

use serde::{Deserialize, Serialize};

pub use config::LibraryConfig;
pub use store::Library;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("library scope denied")]
    Scope,
    #[error("library writer is busy")]
    Busy,
    #[error("capability unavailable: {0}")]
    Capability(String),
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("database operation failed: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("model request failed: {0}")]
    Model(#[from] llmbase_model::ModelError),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Raw,
    Compiled,
    Note,
}
impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Compiled => "compiled",
            Self::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    pub library_id: String,
    pub source_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub passage_id: String,
    /// UTF-8 byte offsets into the complete original revision, including frontmatter.
    pub start: usize,
    pub end: usize,
    pub offset_unit: String,
    pub exact_quote: String,
    pub source_url: Option<String>,
    pub locator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedEvidence {
    pub evidence: Evidence,
    pub stale: bool,
    pub deleted: bool,
    pub current_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub library_id: String,
    pub source_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub kind: String,
    pub rel_path: String,
    pub slug: String,
    pub title: String,
    pub metadata: serde_json::Value,
    pub content: String,
    pub body_start: usize,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub source_id: String,
    pub slug: String,
    pub title: String,
    pub source_kind: String,
    pub score: f64,
    pub snippet: String,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub library_id: String,
    pub mode: String,
    pub degradation: Option<String>,
    pub results: Vec<SearchHit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexReport {
    pub added: usize,
    pub updated: usize,
    pub moved: usize,
    pub deleted: usize,
    pub unchanged: usize,
    pub passages_written: usize,
}
