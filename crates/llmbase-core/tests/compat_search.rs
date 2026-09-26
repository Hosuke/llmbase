use llmbase_core::{LibraryConfig, compat_search};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, LibraryConfig) {
    let directory = TempDir::new().unwrap();
    fs::create_dir_all(directory.path().join("wiki/concepts")).unwrap();
    fs::create_dir_all(directory.path().join("wiki/outputs")).unwrap();
    fs::create_dir_all(directory.path().join("raw/book")).unwrap();
    fs::write(
        directory.path().join("wiki/concepts/alpha.md"),
        "---\ntitle: Alpha\nsummary: first\n---\nalpha alpha",
    )
    .unwrap();
    fs::write(
        directory.path().join("wiki/concepts/beta.md"),
        "---\ntitle: Beta\n---\nalpha",
    )
    .unwrap();
    fs::write(
        directory.path().join("raw/book/index.md"),
        "---\ntitle: 原典\nsource: /private/example/book\n---\n學而時習之，𠮷野。\n",
    )
    .unwrap();
    let config = LibraryConfig::load(directory.path(), "compat-test").unwrap();
    (directory, config)
}

#[test]
fn default_native_rank_keeps_weights_keys_and_does_not_create_database() {
    let (directory, config) = fixture();
    let result = compat_search::search(
        &config,
        &json!({"query":"alpha","kind":"compiled","stopwords":[],"cjk_stopwords":[]}),
        true,
    )
    .unwrap();
    let hits = result["results"].as_array().unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0]["slug"], "alpha");
    assert_eq!(hits[0]["score"], 2.386);
    assert_eq!(hits[1]["score"], 1.0);
    assert_eq!(hits[0]["matched_terms"], json!(["alpha"]));
    assert_eq!(hits[0].as_object().unwrap().len(), 7);
    assert_eq!(result["trace"]["core"], "rust");
    assert_eq!(result["trace"]["storage"], "native_filesystem_snapshot");
    assert!(!directory.path().join("wiki/_meta").exists());
}

#[test]
fn source_edits_and_mutable_stopwords_are_visible_on_every_call() {
    let (directory, config) = fixture();
    let stopped = compat_search::search(
        &config,
        &json!({"query":"alpha","stopwords":["alpha"]}),
        true,
    )
    .unwrap();
    assert!(stopped["results"].as_array().unwrap().is_empty());
    fs::write(
        directory.path().join("wiki/concepts/new.md"),
        "---\ntitle: New\n---\nnewmarker",
    )
    .unwrap();
    let result =
        compat_search::search(&config, &json!({"query":"newmarker","stopwords":[]}), true).unwrap();
    assert_eq!(result["results"][0]["slug"], "new");
}

#[test]
fn raw_skips_malformed_files_and_redacts_private_source_paths() {
    let (directory, config) = fixture();
    fs::write(
        directory.path().join("raw/book/bad.md"),
        "---\ntitle: [unterminated\n---\nbad",
    )
    .unwrap();
    let result =
        compat_search::search(&config, &json!({"query":"𠮷野","kind":"raw"}), true).unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), 1);
    let hit = &result["results"][0];
    assert_eq!(hit["source"], "book");
    assert_eq!(hit["rel_path"], "book/index.md");
    assert_eq!(hit["source_url"], "");
    assert!(hit.get("path").is_none());
    fs::write(
        directory.path().join("wiki/concepts/bad.md"),
        "---\ntitle: [unterminated\n---\nbad",
    )
    .unwrap();
    assert!(
        compat_search::search(&config, &json!({"query":"bad","kind":"compiled"}), true).is_err()
    );
}

#[test]
fn cjk_non_bmp_combining_marks_and_stopword_bigram_contract() {
    let stops = HashSet::from(["the".into()]);
    let cjk_stops = HashSet::from(["的".into()]);
    assert_eq!(
        compat_search::tokenize("The Go语言 cafe\u{301} 𠮷野的空", &stops, &cjk_stops),
        [
            "go", "cafe", "语", "言", "语言", "𠮷", "野", "空", "𠮷野", "野的", "的空"
        ]
    );
}

fn callback_request(snapshot: &Value) -> Value {
    let docs: Vec<_> = snapshot["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|doc| {
            json!({"source_id":doc["source_id"],"content_hash":doc["content_hash"],
            "tokens":if doc["slug"] == "beta" {vec!["custom"]} else {Vec::<&str>::new()}})
        })
        .collect();
    json!({"query":"not-a-matching-literal","kind":"compiled","top_k":10,
        "tokenized":{"snapshot":snapshot["snapshot"],"query_terms":["custom"],"documents":docs}})
}

#[test]
fn python_callback_tokens_are_actually_scored_by_native() {
    let (_directory, config) = fixture();
    let snapshot = compat_search::documents(&config, "compiled", true).unwrap();
    let result = compat_search::search(&config, &callback_request(&snapshot), true).unwrap();
    assert_eq!(result["results"][0]["slug"], "beta");
    assert_eq!(result["results"].as_array().unwrap().len(), 1);
    assert_eq!(result["trace"]["tokenizer"], "python_callback");
}

#[test]
fn stale_incomplete_duplicate_or_forged_callback_snapshots_fail() {
    let (directory, config) = fixture();
    let snapshot = compat_search::documents(&config, "compiled", true).unwrap();
    let original = callback_request(&snapshot);
    let mut forged = original.clone();
    forged["tokenized"]["documents"][0]["content_hash"] = json!("forged");
    assert!(compat_search::search(&config, &forged, true).is_err());
    let mut missing = original.clone();
    missing["tokenized"]["documents"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(compat_search::search(&config, &missing, true).is_err());
    let mut duplicate = original.clone();
    duplicate["tokenized"]["documents"][1] = duplicate["tokenized"]["documents"][0].clone();
    assert!(compat_search::search(&config, &duplicate, true).is_err());
    fs::write(
        directory.path().join("wiki/concepts/alpha.md"),
        "changed source",
    )
    .unwrap();
    assert!(compat_search::search(&config, &original, true).is_err());
}

#[test]
fn compiled_includes_flat_notes_and_preserves_negative_slice_while_raw_clamps() {
    let (directory, config) = fixture();
    fs::write(directory.path().join("wiki/outputs/note.md"), "alpha note").unwrap();
    fs::create_dir_all(directory.path().join("wiki/concepts/nested")).unwrap();
    fs::write(
        directory.path().join("wiki/concepts/nested/ignored.md"),
        "alpha nested",
    )
    .unwrap();
    let all = compat_search::search(&config, &json!({"query":"alpha","top_k":null}), true).unwrap();
    assert_eq!(all["results"].as_array().unwrap().len(), 3);
    let limited =
        compat_search::search(&config, &json!({"query":"alpha","top_k":-1}), true).unwrap();
    assert_eq!(limited["results"].as_array().unwrap().len(), 2);
    let raw = compat_search::search(
        &config,
        &json!({"query":"學","kind":"raw","top_k":-1}),
        true,
    )
    .unwrap();
    assert!(raw["results"].as_array().unwrap().is_empty());
}

#[test]
fn private_documents_require_capability_even_in_legacy_snapshot() {
    let (directory, config) = fixture();
    fs::write(
        directory.path().join("wiki/concepts/secret.md"),
        "---\nprivate: true\n---\nsecretmarker",
    )
    .unwrap();
    let public = compat_search::search(&config, &json!({"query":"secretmarker"}), false).unwrap();
    assert!(public["results"].as_array().unwrap().is_empty());
    let private = compat_search::search(&config, &json!({"query":"secretmarker"}), true).unwrap();
    assert_eq!(private["results"][0]["slug"], "secret");
}

#[test]
fn local_public_only_excludes_private_before_idf_and_callback_documents() {
    let (directory, config) = fixture();
    let request = json!({"query":"alpha","public_only":true});
    let baseline = compat_search::search(&config, &request, true).unwrap();
    fs::write(
        directory.path().join("wiki/concepts/secret.md"),
        "---\nprivate: true\n---\nalpha secretmarker",
    )
    .unwrap();
    let public = compat_search::search(&config, &request, true).unwrap();
    assert_eq!(public["results"], baseline["results"]);
    assert_eq!(public["trace"]["documents"], 2);
    assert_eq!(public["trace"]["visibility"], "public");
    let documents = compat_search::documents(&config, "compiled", false).unwrap();
    assert!(!documents.to_string().contains("secretmarker"));
    let mut callback = callback_request(&documents);
    callback["public_only"] = json!(true);
    assert!(compat_search::search(&config, &callback, true).is_ok());
    fs::create_dir_all(config.raw.join("book/.original")).unwrap();
    fs::write(
        config.raw.join("book/.original/opaque.md"),
        "opaqueartifactmarker",
    )
    .unwrap();
    let raw_request = json!({"query":"opaqueartifactmarker","kind":"raw","public_only":true});
    assert_eq!(
        compat_search::search(&config, &raw_request, true).unwrap()["results"],
        json!([])
    );
    assert!(
        !compat_search::documents(&config, "raw", false)
            .unwrap()
            .to_string()
            .contains("opaqueartifactmarker")
    );
    // Explicit local legacy search preserves its established all-files behavior.
    assert_eq!(
        compat_search::search(
            &config,
            &json!({"query":"opaqueartifactmarker","kind":"raw"}),
            true
        )
        .unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn empty_query_and_callback_do_not_parse_malformed_compiled_sources() {
    let (directory, config) = fixture();
    fs::write(
        directory.path().join("wiki/concepts/bad.md"),
        "---\ntitle: [unterminated\n---\nbad",
    )
    .unwrap();
    for args in [
        json!({"query":""}),
        json!({"query":"the","stopwords":["the"]}),
        json!({"query":"custom","tokenized":{"snapshot":"","documents":[],"query_terms":[]}}),
    ] {
        let result = compat_search::search(&config, &args, true).unwrap();
        assert_eq!(result["results"], json!([]));
        assert_eq!(result["trace"]["short_circuit"], true);
        assert_eq!(result["trace"]["snapshot"], Value::Null);
    }
}

#[test]
fn raw_null_title_and_unrelated_compiled_metadata_keep_legacy_behavior() {
    let (directory, config) = fixture();
    fs::write(
        directory.path().join("raw/null.md"),
        "---\ntitle: null\nsummary: null\ntags: false\n---\nuniquemarker",
    )
    .unwrap();
    let result =
        compat_search::search(&config, &json!({"query":"uniquemarker","kind":"raw"}), true)
            .unwrap();
    assert_eq!(result["results"][0]["title"], "null");
}
