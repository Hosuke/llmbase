//! Local CLI file access. Remote transports only accept content or scoped paths.
use crate::Gateway;
use llmbase_core::{
    Error,
    application::{Principal, Request},
};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};

pub fn read_file(path: &Path) -> Result<Vec<u8>, Error> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Invalid("source must be a regular file".into()));
    }
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(Error::Invalid("source exceeds32 MiB".into()));
    }
    Ok(bytes)
}

pub async fn file(
    gateway: &Gateway,
    principal: Principal,
    library_id: &str,
    path: &Path,
) -> Result<Value, Error> {
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Invalid("filename must be UTF-8".into()))?;
    llmbase_core::ingest::text_path(filename)?;
    let content = String::from_utf8(read_file(path)?)
        .map_err(|_| Error::Capability("binary source requires a parser adapter".into()))?;
    gateway
        .execute(
            principal,
            Request {
                library_id: library_id.into(),
                operation: "ingest_text".into(),
                args: json!({"filename":filename,"content":content}),
            },
        )
        .await
}

pub async fn pdf(
    gateway: &Gateway,
    principal: Principal,
    library_id: &str,
    path: &Path,
    chunk_pages: usize,
) -> Result<Value, Error> {
    let bytes = read_file(path)?;
    gateway.execute(principal,Request{library_id:library_id.into(),operation:"plugin_ingest".into(),args:json!({"adapter":"pdf","input":{"filename":path.file_name().and_then(|s|s.to_str()).unwrap_or("source.pdf"),"content_bytes":bytes,"chunk_pages":chunk_pages}})}).await
}

pub async fn directory(
    gateway: &Gateway,
    principal: Principal,
    library_id: &str,
    path: &Path,
) -> Result<Value, Error> {
    if !path.is_dir() {
        return Err(Error::Invalid("source must be a directory".into()));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let p = entry.path();
            if matches!(
                p.extension()
                    .and_then(|s| s.to_str())
                    .map(str::to_lowercase)
                    .as_deref(),
                Some("md" | "markdown" | "txt" | "py" | "json" | "csv" | "pdf")
            ) {
                files.push(p);
            }
        }
        if files.len() > 1000 {
            return Err(Error::Invalid(
                "directory ingestion is bounded to1000 files".into(),
            ));
        }
    }
    files.sort();
    let mut accepted = Vec::new();
    for path in files {
        let result = if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            pdf(gateway, principal.clone(), library_id, &path, 20).await
        } else {
            file(gateway, principal.clone(), library_id, &path).await
        };
        match result {
            Ok(result) => accepted.push(result),
            Err(error) if accepted.is_empty() => return Err(error),
            Err(error) => {
                return Ok(
                    json!({"status":"partial","accepted":accepted,"error":crate::error_payload(&error)}),
                );
            }
        }
    }
    Ok(json!({"status":"succeeded","accepted":accepted}))
}
