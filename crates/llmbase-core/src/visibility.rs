//! One fail-closed policy for accepted and live metadata across transports.
use serde_json::Value;

/// Only an absent, null, or explicitly false private flag permits public access.
/// Malformed strings/numbers/containers must never accidentally grant publicity.
pub fn private_flag(metadata: &Value) -> bool {
    !matches!(
        metadata.get("private"),
        None | Some(Value::Null | Value::Bool(false))
    )
}

pub fn visibility(metadata: &Value) -> &'static str {
    if private_flag(metadata) {
        return "private";
    }
    match metadata.get("visibility") {
        None => "public",
        Some(Value::String(value)) if value == "public" => "public",
        Some(Value::String(value)) if value == "internal" => "internal",
        _ => "private",
    }
}

pub fn is_public(metadata: &Value) -> bool {
    visibility(metadata) == "public"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn malformed_flags_and_visibility_fail_closed() {
        for value in [
            json!(true),
            json!("yes"),
            json!("on"),
            json!("false"),
            json!(""),
            json!(0),
            json!([]),
            json!({}),
        ] {
            assert!(!is_public(&json!({"private":value})), "{value}");
        }
        for value in [Value::Null, json!(false)] {
            assert!(is_public(&json!({"private":value})));
        }
        assert!(is_public(&json!({})));
        assert!(is_public(&json!({"visibility":"public"})));
        assert_eq!(visibility(&json!({"visibility":"internal"})), "internal");
        for value in [
            Value::Null,
            json!(false),
            json!(""),
            json!("PUBLIC"),
            json!(0),
        ] {
            assert!(!is_public(&json!({"visibility":value})), "{value}");
        }
        assert_eq!(
            visibility(&json!({"visibility":"public","private":"yes"})),
            "private"
        );
    }
}
