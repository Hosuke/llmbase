//! Python JSON numbers for the local pipeline transport. Collision-checked large
//! integers stand in for non-finite tokens only in memory. JSONL and responses
//! contain ordinary NaN/Infinity tokens; strings are never tags. No private APIs.
use crate::{Error, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct Codec {
    replacements: HashMap<String, String>,
    seen_numbers: HashSet<String>,
}

// Transform complete unquoted tokens; the JSON parser validates the grammar.
fn tokens(text: &str, mut visit: impl FnMut(&str) -> String) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        let start = cursor;
        if bytes[cursor] == b'"' {
            cursor += 1;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'\\' => cursor = (cursor + 2).min(bytes.len()),
                    b'"' => {
                        cursor += 1;
                        break;
                    }
                    _ => cursor += 1,
                }
            }
            output.push_str(&text[start..cursor]);
        } else if bytes[cursor].is_ascii_whitespace() || b"{}[],:".contains(&bytes[cursor]) {
            output.push(bytes[cursor] as char);
            cursor += 1;
        } else {
            while cursor < bytes.len()
                && !bytes[cursor].is_ascii_whitespace()
                && !b"{}[],:\"".contains(&bytes[cursor])
            {
                cursor += 1;
            }
            output.push_str(&visit(&text[start..cursor]));
        }
    }
    output
}

impl Codec {
    fn fresh(&mut self, input: &str, original: &str) -> String {
        loop {
            // >u128: native counters/offsets cannot alias these markers.
            let marker = format!("8{}{}", Uuid::new_v4().as_u128(), "0".repeat(64));
            if !input.contains(&marker)
                && !self.seen_numbers.contains(&marker)
                && !self.replacements.contains_key(&marker)
            {
                self.replacements
                    .insert(marker.clone(), original.to_owned());
                self.seen_numbers.insert(marker.clone());
                return marker;
            }
        }
    }

    pub(super) fn parse(&mut self, text: &str) -> Result<Value> {
        // Reserve every original number before choosing any marker, including
        // literals after the non-finite token in this same record.
        tokens(text, |token| {
            if serde_json::from_str::<serde_json::Number>(token).is_ok() {
                self.seen_numbers.insert(token.to_owned());
            }
            token.to_owned()
        });
        let encoded = tokens(text, |token| {
            if matches!(token, "NaN" | "Infinity" | "-Infinity")
                || self.replacements.contains_key(token)
            {
                // A later literal can deliberately equal an earlier marker.
                // Escape it with its literal spelling as the replacement.
                self.fresh(text, token)
            } else {
                token.to_owned()
            }
        });
        Ok(serde_json::from_str(&encoded)?)
    }

    pub(super) fn encode(&self, value: &Value) -> Result<String> {
        let encoded = serde_json::to_string(value)?;
        // Restore once: escaped literal markers must never recursively expand.
        Ok(tokens(&encoded, |token| {
            self.replacements
                .get(token)
                .cloned()
                .unwrap_or_else(|| token.to_owned())
        }))
    }

    pub(super) fn python_number(&self, value: &serde_json::Number) -> String {
        let token = value.to_string();
        match self.replacements.get(&token).map(String::as_str) {
            Some("NaN") => "nan".into(),
            Some("Infinity") => "inf".into(),
            Some("-Infinity") => "-inf".into(),
            Some(literal) => literal.to_owned(),
            None => token,
        }
    }

    pub(super) fn finite_value(&self, value: Value) -> Result<Value> {
        serde_json::from_str(&self.encode(&value)?).map_err(|_| {
            Error::Capability(
                "non-finite legacy pipeline result requires the extended-JSON local transport"
                    .into(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_marker_collision_is_escaped_and_restored_once() {
        let mut codec = Codec::default();
        let first = codec.parse(r#"{"x":NaN}"#).unwrap();
        let marker = first["x"].to_string();
        let literal = format!(r#"{{"literal":{marker},"string":"{marker}","new":Infinity}}"#);
        let second = codec.parse(&literal).unwrap();
        let encoded = codec.encode(&serde_json::json!([first, second])).unwrap();
        assert!(encoded.contains(&format!("\"literal\":{marker}")));
        assert!(encoded.contains(&format!("\"string\":\"{marker}\"")));
        assert!(encoded.contains("\"x\":NaN"));
        assert!(encoded.contains("\"new\":Infinity"));
        assert_eq!(
            codec.python_number(second["literal"].as_number().unwrap()),
            marker
        );
    }
    #[test]
    fn strings_escapes_big_numbers_and_bad_tokens_are_not_tags() {
        let mut codec = Codec::default();
        let text = r#"{"NaN":"Infinity -Infinity NaN \\\"","huge":1208925819614629174706176,"values":[NaN,Infinity,-Infinity]}"#;
        let value = codec.parse(text).unwrap();
        assert_eq!(codec.encode(&value).unwrap(), text);
        assert!(codec.parse(r#"{"x":NaNx}"#).is_err());
        assert!(codec.parse(r#"{"x":+Infinity}"#).is_err());
    }
}
