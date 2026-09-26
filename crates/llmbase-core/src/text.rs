use crate::{Error, Result};
use ferrous_opencc::{OpenCC, config::BuiltinConfig};
use unicode_normalization::UnicodeNormalization;

/// Normalization is used only for retrieval. Evidence always uses original revision bytes.
pub struct Normalizer {
    converter: OpenCC,
}
impl Normalizer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            converter: OpenCC::from_config(BuiltinConfig::T2s)
                .map_err(|e| Error::Capability(format!("CJK dictionary: {e}")))?,
        })
    }
    pub fn normalize(&self, text: &str) -> String {
        self.converter
            .convert(&text.nfkc().collect::<String>())
            .to_lowercase()
    }
    pub fn tokens(&self, text: &str) -> Vec<String> {
        let normalized = self.normalize(text);
        let mut result = Vec::new();
        let mut word = String::new();
        let mut previous = None;
        for c in normalized.chars() {
            if is_cjk(c) {
                if !word.is_empty() {
                    result.push(std::mem::take(&mut word));
                }
                result.push(c.to_string());
                if let Some(p) = previous {
                    result.push(format!("{p}{c}"));
                }
                previous = Some(c);
            } else {
                previous = None;
                if c.is_alphanumeric() || c == '_' {
                    word.push(c);
                } else if !word.is_empty() {
                    result.push(std::mem::take(&mut word));
                }
            }
        }
        if !word.is_empty() {
            result.push(word);
        }
        result
    }
    pub fn representation(&self, text: &str) -> String {
        self.tokens(text).join(" ")
    }
    pub fn fts_query(&self, text: &str) -> String {
        self.tokens(text)
            .into_iter()
            .take(96)
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ")
    }
}
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30ff | 0x31f0..=0x31ff | 0x3400..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xff65..=0xff9f | 0x20000..=0x3ffff)
}
