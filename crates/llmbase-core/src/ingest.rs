//! Text ingestion with original-byte preservation and the shared source journal.
//! Filesystem paths are read only by the local CLI adapter, never by this DTO.
use crate::application::{Application, Principal, Request};
use crate::{
    Error, Library, Result,
    config::scoped_path,
    document::{Document, hash},
    store::WriterSession,
};
use llmbase_model::CancellationToken;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};

const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRequest {
    pub filename: String,
    pub content: String,
    #[serde(default)]
    pub expected_hash: Option<String>,
}

pub fn text_path(filename: &str) -> Result<String> {
    if filename.is_empty()
        || filename.starts_with('.')
        || filename.len() > 255
        || filename.contains(['/', '\\', '\0'])
        || filename == "."
        || filename == ".."
        || filename.chars().any(char::is_control)
    {
        return Err(Error::Invalid(
            "ingest filename must be a single bounded filename".into(),
        ));
    }
    let file = Path::new(filename);
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Invalid("invalid filename".into()))?;
    let mut slug = String::new();
    for ch in stem.to_lowercase().trim().chars().take(80) {
        if ch.is_alphanumeric() {
            slug.push(ch)
        } else if (ch.is_whitespace() || ch == '_' || ch == '-') && !slug.ends_with('-') {
            slug.push('-')
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        return Err(Error::Invalid(
            "filename produces an empty source slug".into(),
        ));
    }
    let extension = file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "md" | "markdown" | "txt" | "py" | "json" | "csv"
    ) {
        return Err(Error::Capability(
            "this format requires a configured parser adapter".into(),
        ));
    }
    // Non-Markdown text becomes a directly compilable Markdown source. Its
    // bytes are copied exactly; no code or markup is executed during ingestion.
    let target = if matches!(extension.as_str(), "md" | "markdown") {
        filename
    } else {
        "index.md"
    };
    Ok(format!("{slug}/{target}"))
}

impl Library {
    /// A remote operation may read an already-present file only inside its
    /// configured library. The CLI can separately read a user-selected file.
    pub fn ingest_existing(&mut self, source: &str) -> Result<Value> {
        let candidate = Path::new(source);
        let relative = if candidate.is_absolute() {
            candidate
                .strip_prefix(&self.config.base)
                .map_err(|_| Error::Scope)?
        } else {
            candidate
        };
        let path = scoped_path(
            &self.config.base,
            relative
                .to_str()
                .ok_or_else(|| Error::Invalid("source path must be UTF-8".into()))?,
        )?;
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::Invalid("source filename is required".into()))?
            .to_owned();
        text_path(&filename)?;
        let file = fs::File::open(&path)?;
        if !file.metadata()?.is_file() {
            return Err(Error::Invalid("source must be a regular file".into()));
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid("source exceeds32 MiB".into()));
        }
        let content = String::from_utf8(bytes)
            .map_err(|_| Error::Capability("binary source requires a parser adapter".into()))?;
        self.ingest_text(TextRequest {
            filename,
            content,
            expected_hash: None,
        })
    }

    pub fn ingest_text(&mut self, request: TextRequest) -> Result<Value> {
        if request.content.len() > MAX_BYTES {
            return Err(Error::Invalid("source exceeds32 MiB".into()));
        }
        let relative = text_path(&request.filename)?;
        Document::parse(request.content.as_bytes().to_vec())?;
        let _writer = WriterSession::acquire(&self.config)?;
        let path = scoped_path(&self.config.raw, &relative)?;
        let unchanged = match fs::read(&path) {
            Ok(bytes) => bytes == request.content.as_bytes(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        let report = if unchanged {
            self.sync()?
        } else {
            self.write_source(
                "raw",
                &relative,
                request.content.as_bytes().to_vec(),
                request.expected_hash.as_deref(),
            )?
        };
        let source = self
            .list(Some("raw"), true)?
            .into_iter()
            .find(|s| s.rel_path == relative)
            .ok_or_else(|| Error::Conflict("accepted source was not indexed".into()))?;
        Ok(
            json!({"path":relative,"source_id":source.source_id,"content_hash":hash(request.content.as_bytes()),
            "unchanged":unchanged,"index":report}),
        )
    }
}

impl Application {
    /// Remote input is an HTTP URL, never a server filesystem path. HTML and
    /// binary formats require the explicitly configured parser process boundary.
    pub async fn ingest_url(
        &self,
        principal: &Principal,
        library_id: &str,
        url: &str,
        cancel: CancellationToken,
    ) -> Result<Value> {
        self.authorize(
            principal,
            &Request {
                library_id: library_id.into(),
                operation: "ingest_url".into(),
                args: json!({"url":url}),
            },
        )?;
        let _cancel_guard = cancel.clone().drop_guard();
        let fetched =
            crate::fetch::fetch(url, crate::fetch::FetchOptions::default(), cancel.clone()).await?;
        if fetched.content_type.contains("html") {
            let body = String::from_utf8(fetched.body_bytes)
                .map_err(|_| Error::Capability("HTML parser input must be UTF-8".into()))?;
            let mut result=self.ingest_with_adapter(principal,library_id,crate::plugins::AdapterRequest{
                adapter:"html".into(),input:json!({"url":fetched.url,"content_type":fetched.content_type,"html":body}),
            },cancel).await?;
            result["path"] = result
                .pointer("/accepted/0/path")
                .cloned()
                .ok_or_else(|| Error::Conflict("parser accepted no source".into()))?;
            return Ok(result);
        }
        if !fetched.content_type.is_empty()
            && !fetched.content_type.starts_with("text/")
            && !fetched.content_type.contains("json")
        {
            return Err(Error::Capability(
                "this URL format requires a configured parser adapter".into(),
            ));
        }
        let doc = Document::parse(fetched.body_bytes)?;
        let mut metadata = doc.metadata.clone();
        metadata["source"] = json!(fetched.url);
        metadata["source_content_hash"] = json!(hash(doc.original.as_bytes()));
        metadata["original_file"] = json!("source.original");
        metadata["original_sha256"] = json!(hash(doc.original.as_bytes()));
        metadata["original_media_type"] = json!(fetched.content_type);
        metadata["type"] = json!("url");
        metadata["compiled"] = json!(false);
        let content = format!(
            "---\n{}---\n{}",
            crate::document::yaml_metadata(&metadata)?,
            &doc.original[doc.body_start..]
        );
        let name = format!("web-{}.md", &hash(url.as_bytes())[..20]);
        let config = self.library_config(library_id)?;
        let original = doc.original.into_bytes();
        tokio::task::spawn_blocking(move || {
            if cancel.is_cancelled() {
                return Err(llmbase_model::ModelError::Cancelled.into());
            }
            let _writer = WriterSession::acquire(&config)?;
            let relative = text_path(&name)?;
            let asset = format!(
                "{}/source.original",
                relative.rsplit_once('/').expect("text path").0
            );
            let current_hash = |relative: &str| -> Result<Option<String>> {
                match fs::read(scoped_path(&config.raw, relative)?) {
                    Ok(bytes) => Ok(Some(hash(&bytes))),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e.into()),
                }
            };
            let expected_hash = current_hash(&relative)?;
            let asset_hash = current_hash(&asset)?;
            let mut library = Library::open(config)?;
            let asset = library.write_raw_asset(&asset, original, asset_hash.as_deref())?;
            let mut result = library.ingest_text(TextRequest {
                filename: name,
                content,
                expected_hash,
            })?;
            result["original_asset"] = asset;
            Ok(result)
        })
        .await
        .map_err(|_| Error::Capability("URL ingest storage worker failed".into()))?
    }
}
