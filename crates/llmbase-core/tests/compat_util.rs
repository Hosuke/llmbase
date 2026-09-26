use llmbase_core::compat_util::transform;
use serde_json::{Value, json};
fn op(name: &str, args: Value) -> Value {
    transform(name, &args).unwrap()
}

#[test]
fn sections_and_flat_split_preserve_original_unicode_scalar_slices() {
    let body =
        "序𠮷é\r\n## 第一章\r\n正文\r\n```\r\n## 偽標\r\n```\r\n### 子\r\n子文\r\n## 第一章\r末";
    let sections = op("sections", json!({"body":body}));
    assert_eq!(sections.as_array().unwrap().len(), 2);
    assert_eq!(sections[0]["start"], 6);
    assert_eq!(sections[0]["children"][0]["title"], "子");
    assert_eq!(
        sections[1]["anchor"],
        format!("{}-2", sections[0]["anchor"].as_str().unwrap())
    );
    let anchor = sections[0]["children"][0]["anchor"].clone();
    assert_eq!(
        op(
            "find_section_path",
            json!({"sections":sections,"anchor":anchor})
        ),
        json!([0, 0])
    );
    assert_eq!(
        op(
            "extract_section",
            json!({"body":body,"sections":sections,"anchor":anchor})
        ),
        "### 子\r\n子文\r\n"
    );
    let split = op("split", json!({"body":body,"level":2}));
    assert_eq!(split[0]["header_line"], "## 第一章");
    assert_eq!(
        split[0]["content"],
        "正文\r\n```\r\n## 偽標\r\n```\r\n### 子\r\n子文"
    );
    assert_eq!(split[1]["content"], "末");
}

#[test]
fn span_anchoring_maps_back_to_exact_original_without_byte_offset_confusion() {
    let body = "序𠮷，甲 é\u{3000}乙！尾";
    assert_eq!(
        op(
            "locate_span",
            json!({"content":body,"head":"𠮷甲","tail":"乙"})
        ),
        json!({"start":1,"end":9,"strategy":"exact"})
    );
    assert_eq!(
        op(
            "locate_span",
            json!({"content":body,"head":"甲","tail":"不存在"})
        ),
        json!({"start":3,"end":11,"strategy":"head_only"})
    );
    assert_eq!(
        op(
            "normalize_span_map",
            json!({"text":"𠮷， a\u{1f}b","level":"punct_spaces"})
        ),
        json!(["𠮷ab", [0, 3, 5]])
    );
    assert_eq!(
        op(
            "locate_span",
            json!({"content":"甲乙甲乙","head":"甲乙","tail":"甲乙"})
        ),
        json!({"start":0,"end":4,"strategy":"exact"})
    );
    assert_eq!(
        op("locate_span", json!({"content":body,"head":"？！"})),
        Value::Null
    );
    assert!(transform("locate_span", &json!({"content":body,"head":""})).is_err());
}

#[test]
fn anchor_configuration_and_unknown_actions_are_not_cached_or_silently_ignored() {
    assert_eq!(
        op(
            "normalize_title",
            json!({"text":"緒論\u{3000}（一）：第一章\u{200b}"})
        ),
        "緒論一第一章"
    );
    let standard = op("make_anchor", json!({"level":2,"titles":["緒論"]}));
    let custom = op(
        "make_anchor",
        json!({"level":2,"titles":["緒論"],"slug_max":1,"hash_len":2}),
    );
    assert_ne!(standard, custom);
    assert!(custom.as_str().unwrap().starts_with("h2-緒-"));
    assert!(transform("make_anchor", &json!({"level":2,"titles":[]})).is_err());
    assert!(transform("unknown", &json!({})).is_err());
}
