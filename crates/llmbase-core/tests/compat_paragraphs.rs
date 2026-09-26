use llmbase_core::compat_util::transform;
use serde_json::{Value, json};

#[test]
fn frozen_legacy_paragraph_oracle() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../tests/rust_migration/fixtures/paragraphs.json"
    ))
    .unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 229);
    for case in cases {
        let result = transform(
            "normalize_paragraphs",
            &json!({
                "body": case["body"], "terminators": case["terminators"],
                "wrappers": case["wrappers"],
            }),
        )
        .unwrap();
        assert_eq!(result, case["expected"], "{}", case["name"]);
    }
}

#[test]
fn paragraph_arguments_are_strings_and_unknown_actions_fail() {
    let args = json!({"body":"甲\n乙", "terminators":"。", "wrappers":"」"});
    for name in ["body", "terminators", "wrappers"] {
        let mut invalid = args.clone();
        invalid.as_object_mut().unwrap().remove(name);
        assert!(transform("normalize_paragraphs", &invalid).is_err());
        invalid[name] = json!(42);
        assert!(transform("normalize_paragraphs", &invalid).is_err());
    }
    assert!(transform("normalize_paragraphs", &json!([])).is_err());
    assert!(transform("unknown", &args).is_err());
}
