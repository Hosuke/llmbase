use llmbase_core::compat_resolve::{resolve, transform};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

#[test]
fn ordered_alias_resolution_preserves_exact_hyphen_space_variants_and_fuzzy_first_match() {
    let aliases = vec![
        ("a-b".into(), json!("hyphen")),
        ("「shared」".into(), json!("first")),
        ("shared!".into(), json!("second")),
        ("四諦".into(), json!("sutra")),
    ];
    assert_eq!(resolve(" A B ", &aliases, &[]), Some(json!("hyphen")));
    assert_eq!(resolve("shared", &aliases, &[]), Some(json!("first")));
    assert_eq!(
        resolve("四谛", &aliases, &["四諦".into()]),
        Some(json!("sutra"))
    );
    assert_eq!(resolve("", &aliases, &[]), None);
    assert_eq!(
        transform("normalize", &json!({"text":"\u{1c} Alpha \u{1f}"})).unwrap(),
        "alpha"
    );
}

#[test]
fn alias_snapshot_has_legacy_title_and_merged_names_current_privacy_and_real_file_order() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("a.md"),
        "---\ntitle: Four truths / 四諦\nmerged_from: [old-name]\n---\nOriginal𠮷",
    )
    .unwrap();
    fs::write(
        root.path().join("b.md"),
        "---\ntitle: Secret\nprivate: true\n---\nHidden",
    )
    .unwrap();
    let before = fs::read(root.path().join("a.md")).unwrap();
    let pairs: Vec<(String, Value)> = serde_json::from_value(
        transform(
            "build",
            &json!({"concepts_dir":root.path(),"public_only":true}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(resolve("四谛", &pairs, &[]), Some(json!("a")));
    assert_eq!(resolve("old-name", &pairs, &[]), Some(json!("a")));
    assert_eq!(resolve("Secret", &pairs, &[]), None);
    assert_eq!(fs::read(root.path().join("a.md")).unwrap(), before);
    fs::write(
        root.path().join("aliases.json"),
        r#"{"z!":"first","z?":"second","z!":"updated"}"#,
    )
    .unwrap();
    let pairs: Vec<(String, Value)> =
        serde_json::from_value(transform("load", &json!({"meta_dir":root.path()})).unwrap())
            .unwrap();
    assert_eq!(pairs[0], ("z!".into(), json!("updated")));
    assert_eq!(resolve("z", &pairs, &[]), Some(json!("updated")));
    assert!(!root.path().join("llmbase.sqlite3").exists());
}

#[test]
fn variants_preserve_original_and_last_generated_collision_semantics() {
    let pairs = transform(
        "variants",
        &json!({"aliases":[["發", "first"],["髮","second"]]}),
    )
    .unwrap();
    let pairs: Vec<(String, Value)> = serde_json::from_value(pairs).unwrap();
    assert_eq!(resolve("發", &pairs, &[]), Some(json!("first")));
    assert_eq!(resolve("发", &pairs, &[]), Some(json!("second")));
    let updated = transform(
        "register",
        &json!({"aliases":[["key","first"]],"name":" KEY ","slug":"second"}),
    )
    .unwrap();
    assert_eq!(updated, json!([["key", "first"]]));
}
