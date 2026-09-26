use crate::{Error, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const PARSER_VERSION: &str = "frontmatter-v1";
pub const CHUNKER_VERSION: &str = "scalar-window-v1";

#[derive(Debug)]
pub struct Document {
    pub original: String,
    pub metadata: Value,
    pub body_start: usize,
    pub hash: String,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// JSON flow mappings are valid YAML 1.2 frontmatter. Serialize them directly
/// rather than forwarding serde_json's arbitrary-precision Number internals
/// to another serializer, which would turn numbers into tagged implementation
/// maps. The JSON read path also retains integers beyond YAML's u64 limit.
pub fn yaml_metadata(metadata: &Value) -> Result<String> {
    if !metadata.is_object() {
        return Err(Error::Invalid("metadata must be a mapping".into()));
    }
    Ok(format!("{}\n", serde_json::to_string_pretty(metadata)?))
}

pub fn parse_metadata(text: &str) -> Result<Value> {
    match serde_json::from_str(text) {
        Ok(value) => Ok(value),
        Err(_) => crate::compat_yaml::parse(text),
    }
}

impl Document {
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        let digest = hash(&bytes);
        let original = String::from_utf8(bytes).map_err(|_| {
            Error::Invalid(
                "text source must be valid UTF-8; binary originals require a parser adapter".into(),
            )
        })?;
        let mut metadata = serde_json::json!({});
        let mut body_start = 0;
        let start = if original.starts_with("---\r\n") {
            5
        } else if original.starts_with("---\n") {
            4
        } else {
            0
        };
        if start > 0 {
            let mut pos = start;
            let mut closed = false;
            for line in original[start..].split_inclusive('\n') {
                if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
                    metadata = parse_metadata(&original[start..pos])?;
                    if metadata.is_null() {
                        metadata = serde_json::json!({});
                    }
                    if !metadata.is_object() {
                        return Err(Error::Invalid("frontmatter must be a mapping".into()));
                    }
                    body_start = pos + line.len();
                    closed = true;
                    break;
                }
                pos += line.len();
            }
            if !closed {
                return Err(Error::Invalid("unterminated frontmatter".into()));
            }
        }
        Ok(Self {
            original,
            metadata,
            body_start,
            hash: digest,
        })
    }
    pub fn title(&self, fallback: &str) -> String {
        self.metadata
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .into()
    }
    pub fn source_url(&self) -> Option<String> {
        self.metadata
            .get("source_url")
            .or_else(|| self.metadata.get("source"))
            .and_then(Value::as_str)
            .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
            .map(str::to_owned)
    }
    pub fn chunks(&self, max_chars: usize) -> Result<Vec<(usize, usize)>> {
        if !(16..=32_000).contains(&max_chars) {
            return Err(Error::Invalid("chunk_chars must be 16..32000".into()));
        }
        let mut spans = Vec::new();
        let mut start = self.body_start;
        let mut count = 0;
        for (offset, _) in self.original[self.body_start..].char_indices() {
            if count == max_chars {
                let end = self.body_start + offset;
                spans.push((start, end));
                start = end;
                count = 0;
            }
            count += 1;
        }
        if start < self.original.len() {
            spans.push((start, self.original.len()));
        }
        Ok(spans)
    }
}

pub fn byte_to_scalar(text: &str, offset: usize) -> Result<usize> {
    text.get(..offset)
        .map(|s| s.chars().count())
        .ok_or_else(|| Error::Invalid("offset is not a UTF-8 boundary".into()))
}

pub fn scalar_to_byte(text: &str, offset: usize) -> Result<usize> {
    text.char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .nth(offset)
        .ok_or_else(|| Error::Invalid("scalar offset exceeds source".into()))
}
