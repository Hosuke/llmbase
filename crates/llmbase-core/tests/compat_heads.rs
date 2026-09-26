use llmbase_core::{
    Error,
    compat_util::{normalize_heads_with, transform},
};
use serde_json::{Value, json};

#[test]
fn frozen_native_dialect_oracle() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../tests/rust_migration/fixtures/heads.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in oracle["cases"].as_array().unwrap() {
        if case["name"]
            .as_str()
            .unwrap()
            .starts_with("python-pattern-")
        {
            continue;
        }
        assert_eq!(
            transform(
                "normalize_heads",
                &json!({
                    "body":case["body"], "rules":case["rules"]
                })
            )
            .unwrap(),
            case["expected"],
            "{}",
            case["name"]
        );
        count += 1;
    }
    assert_eq!(count, 52);
}

#[test]
fn scanner_calls_only_eligible_titles_in_order_and_keeps_groups() {
    let body = "  ###\t First  ##  \r\n```\n# skipped\n```\n<div>\n# skipped2\n\n## Second\n";
    let mut titles = Vec::new();
    let value = normalize_heads_with(body, true, |title| {
        titles.push(title.to_owned());
        Ok(Some(4))
    })
    .unwrap();
    assert_eq!(titles, ["First", "Second"]);
    assert_eq!(
        value,
        "  ####\t First  ##\r\n```\r\n# skipped\r\n```\r\n<div>\r\n# skipped2\r\n\r\n#### Second\r\n"
    );
    let mut calls = 0;
    let error = normalize_heads_with("# first\n# second", true, |_| {
        calls += 1;
        Err(Error::Capability("expected matcher error".into()))
    })
    .unwrap_err();
    assert_eq!(calls, 1);
    assert!(error.to_string().contains("expected matcher error"));
}

#[test]
fn no_rules_preserves_bytes_and_never_invokes_classifier() {
    let body = "# heading\r\ntext\u{2028}\r";
    assert_eq!(
        normalize_heads_with(body, false, |_| panic!("disabled matcher")).unwrap(),
        body
    );
    assert_eq!(
        transform("normalize_heads", &json!({"body":body,"rules":[]})).unwrap(),
        body
    );
    assert_eq!(
        transform(
            "normalize_heads",
            &json!({"body":"# title","rules":[
                {"pattern":"title","level":2}, {"pattern":".","level":6}
            ]})
        )
        .unwrap(),
        "## title"
    );
}

#[test]
fn invalid_rules_fail_eagerly_without_python_fallback() {
    for rules in [
        json!(null),
        json!({}),
        json!([null]),
        json!([{"pattern":null,"level":2}]),
        json!([{"pattern":"x"}]),
        json!([{"pattern":"x","level":0}]),
        json!([{"pattern":"x","level":7}]),
        json!([{"pattern":"x","level":true}]),
        json!([{"pattern":"x","level":2.0}]),
        json!([{"pattern":"[","level":2}]),
        json!([{"pattern":"(?=x)","level":2}]),
        json!([{"pattern":r"(x)\1","level":2}]),
    ] {
        assert!(
            transform("normalize_heads", &json!({"body":"","rules":rules})).is_err(),
            "{rules}"
        );
    }
    assert!(transform("normalize_heads", &json!({"body":42,"rules":[]})).is_err());
    assert!(normalize_heads_with("# x", true, |_| Ok(Some(0))).is_err());
    assert!(normalize_heads_with("# x", true, |_| Ok(Some(7))).is_err());
}

#[test]
fn resource_bounds_are_enforced() {
    let rule = json!({"pattern":"x","level":2});
    assert!(
        transform(
            "normalize_heads",
            &json!({"body":"", "rules":vec![rule;257]})
        )
        .is_err()
    );
    assert!(
        transform(
            "normalize_heads",
            &json!({"body":"", "rules":[
                {"pattern":"x".repeat(64*1024+1),"level":2}
            ]})
        )
        .is_err()
    );
    let error = transform(
        "normalize_heads",
        &json!({"body":"", "rules":[
            {"pattern":r"\w{1000}","level":2}
        ]}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid native heading regex"));
    assert!(normalize_heads_with(&"x".repeat(16 * 1024 * 1024 + 1), false, |_| Ok(None)).is_err());
}
