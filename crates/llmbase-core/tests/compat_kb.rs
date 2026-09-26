use llmbase_core::{LibraryConfig, compat_kb};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, LibraryConfig) {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("wiki/concepts")).unwrap();
    fs::create_dir_all(temp.path().join("raw/book")).unwrap();
    fs::write(temp.path().join("wiki/concepts/alpha.md"),"---\ntitle: null\nsummary: 42\ntags: [經, 7, false]\nsources: {kind: original}\n---\n序𠮷é\n## 所出\n正文\n### 字例\n𠮷é\n").unwrap();
    fs::write(
        temp.path().join("wiki/concepts/beta.md"),
        "---\ntitle: Beta\ntags: 經學\n---\nsecond",
    )
    .unwrap();
    fs::write(temp.path().join("raw/book/index.md"),"---\ntitle: null\nsource: /private/example/file\ncustom: {safe: 7, private: /private/example/key, nothing: null}\nitems: [one, /private/example/key, null, false]\n---\n原典").unwrap();
    let config = LibraryConfig::load(temp.path(), "compat-kb").unwrap();
    (temp, config)
}
fn op(config: &LibraryConfig, name: &str, args: Value) -> Value {
    compat_kb::execute(config, name, &args).unwrap()["result"].clone()
}

#[test]
fn visibility_snapshot_is_current_scoped_and_does_not_initialize_storage() {
    let (temp, config) = fixture();
    fs::create_dir_all(config.raw.join("book/.original")).unwrap();
    fs::write(
        config.raw.join("book/.original/opaque.md"),
        "---\nprivate: true\n---\nORIGINAL",
    )
    .unwrap();
    let initial = op(&config, "compat_kb_visibility", json!({}));
    assert_eq!(initial["compiled"]["alpha"]["public"], true);
    assert_eq!(initial["raw"]["book"]["public"], true);
    assert_eq!(initial["has_nonpublic"], false);
    assert!(initial["raw"].get("book/.original/opaque").is_none());
    fs::write(
        config.concepts.join("alpha.md"),
        "---\nprivate: true\n---\nSECRET",
    )
    .unwrap();
    fs::write(
        config.concepts.join("broken.md"),
        "---\nvisibility: [bad\n---\nSECRET",
    )
    .unwrap();
    let current = op(&config, "compat_kb_visibility", json!({}));
    assert_eq!(current["compiled"]["alpha"]["public"], false);
    assert_eq!(current["compiled"]["broken"]["public"], false);
    assert_eq!(current["has_nonpublic"], true);
    assert!(!current.to_string().contains("SECRET"));
    assert!(!temp.path().join("wiki/_meta").exists());
}

#[test]
fn public_trails_report_true_empty_only_and_never_hide_parse_errors() {
    let (_temp, config) = fixture();
    let args = json!({"public_only":true});
    assert_eq!(
        op(&config, "compat_kb_trails", args.clone()),
        json!({"trails":[]})
    );
    fs::create_dir_all(&config.meta).unwrap();
    let path = config.meta.join("trails.json");
    fs::write(&path, r#"{"trails":[],"private_extra":"SECRET"}"#).unwrap();
    assert_eq!(
        op(&config, "compat_kb_trails", args.clone()),
        json!({"trails":[]})
    );
    fs::write(&path, r#"{"trails":[{"name":"SECRET"}]}"#).unwrap();
    assert!(matches!(
        compat_kb::execute(&config, "compat_kb_trails", &args),
        Err(llmbase_core::Error::Scope)
    ));
    assert_eq!(
        op(&config, "compat_kb_trails", json!({}))["trails"][0]["name"],
        "SECRET"
    );
    fs::write(&path, "{broken").unwrap();
    assert!(compat_kb::execute(&config, "compat_kb_trails", &args).is_err());
    assert!(!config.database().exists());
}

#[test]
fn get_and_list_preserve_existing_json_metadata_types_without_creating_database() {
    let (temp, config) = fixture();
    let found = op(&config, "compat_kb_get", json!({"slug":"alpha"}));
    assert_eq!(found["title"], Value::Null);
    assert_eq!(found["summary"], 42);
    assert_eq!(found["tags"], json!(["經", 7, false]));
    assert_eq!(found["sources"], json!({"kind":"original"}));
    assert_eq!(found["content"], "序𠮷é\n## 所出\n正文\n### 字例\n𠮷é");
    let listing = op(&config, "compat_kb_list", json!({"tag":"經"}));
    assert_eq!(listing["articles"].as_array().unwrap().len(), 2);
    assert_eq!(listing["articles"][0]["slug"], "alpha");
    assert!(!temp.path().join("wiki/_meta").exists());
    assert_eq!(
        op(&config, "compat_kb_get", json!({"slug":"missing"})),
        json!({"found":false,"slug":"missing"})
    );
}

#[test]
fn aliases_follow_file_order_and_cannot_escape_the_configured_root() {
    let (_temp, config) = fixture();
    fs::create_dir_all(&config.meta).unwrap();
    fs::write(
        config.meta.join("aliases.json"),
        r#"{"x-y":"beta","xy":"alpha","named-key":"alpha","escape":"../secret"}"#,
    )
    .unwrap();
    assert_eq!(
        op(&config, "compat_kb_get", json!({"slug":"x_y!"}))["found"],
        false
    );
    assert_eq!(
        op(&config, "compat_kb_get", json!({"slug":"x.y!"}))["slug"],
        "beta"
    );
    assert_eq!(
        op(&config, "compat_kb_get", json!({"slug":"Named Key"}))["slug"],
        "alpha"
    );
    assert_eq!(
        op(
            &config,
            "compat_kb_get",
            json!({"slug":"Named Key","no_alias":true})
        )["found"],
        false
    );
    assert_eq!(
        op(&config, "compat_kb_get", json!({"slug":"escape"}))["found"],
        false
    );
}

#[test]
fn sections_retain_scalar_offsets_subtrees_and_collision_suffixes() {
    let (_temp, config) = fixture();
    let article = op(&config, "compat_kb_get", json!({"slug":"alpha"}));
    let sections = op(&config, "compat_kb_sections", json!({"slug":"alpha"}));
    let first = &sections["sections"][0];
    assert_eq!(first["start"], 5);
    assert_eq!(first["children"][0]["start"], 14);
    let child = &first["children"][0];
    let extracted = op(
        &config,
        "compat_kb_get",
        json!({"slug":"alpha","section":child["anchor"]}),
    );
    assert_eq!(extracted["content"], "### 字例\n𠮷é");
    assert_eq!(article["title"], extracted["title"]);
    assert_eq!(
        op(
            &config,
            "compat_kb_get",
            json!({"slug":"alpha","section":"missing"})
        )["section_found"],
        false
    );
    let repeated = compat_kb::parse_sections("# A\n## 子\n```\n## hidden\n```\n## 子\n");
    assert_eq!(repeated[0]["children"].as_array().unwrap().len(), 2);
    assert_eq!(
        repeated[0]["children"][1]["anchor"],
        format!(
            "{}-2",
            repeated[0]["children"][0]["anchor"].as_str().unwrap()
        )
    );
}

#[test]
fn raw_listing_redacts_recursive_paths_and_retains_null_base_metadata() {
    let (_temp, config) = fixture();
    let list = op(&config, "compat_kb_raw_list", json!({}));
    assert_eq!(list[0]["path"], "book");
    assert_eq!(list[0]["title"], Value::Null);
    assert_eq!(list[0]["source"], Value::Null);
    assert_eq!(list[0]["items"], json!(["one", false]));
    assert_eq!(list[0]["custom"], json!({"safe":7}));
}

#[test]
fn raw_writes_use_existing_native_writer_lock_without_scanning_unrelated_files() {
    use fs2::FileExt;
    let (_temp, config) = fixture();
    fs::write(
        config.raw.join("malformed.md"),
        "---\ntitle: [bad\n---\nbad",
    )
    .unwrap();
    let write = op(
        &config,
        "compat_kb_raw_write",
        json!({"path":"new/index.md","content":"---\ntitle: New\n---\ncontent"}),
    );
    assert!(!config.database().exists());
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(config.meta.join("llmbase.writer.lock"))
        .unwrap();
    file.try_lock_exclusive().unwrap();
    assert!(
        compat_kb::execute(
            &config,
            "compat_kb_raw_write",
            &json!({"path":"blocked.md","content":"blocked"})
        )
        .is_err()
    );
    assert!(compat_kb::execute(&config, "compat_kb_list", &json!({})).is_err());
    FileExt::unlock(&file).unwrap();
    assert!(
        compat_kb::execute(
            &config,
            "compat_kb_raw_write",
            &json!({"path":"new/index.md","content":"changed","expected_hash":"wrong"})
        )
        .is_err()
    );
    let replaced = op(
        &config,
        "compat_kb_raw_write",
        json!({"path":"new/index.md","content":"changed","expected_hash":write["content_hash"]}),
    );
    assert_ne!(replaced["content_hash"], write["content_hash"]);
    assert_eq!(
        fs::read_to_string(config.raw.join("new/index.md")).unwrap(),
        "changed"
    );
    assert!(
        compat_kb::execute(
            &config,
            "compat_kb_raw_write",
            &json!({"path":"../escape.md","content":"bad"})
        )
        .is_err()
    );
}

#[test]
fn stats_count_raw_entries_and_health_preserves_existing_payload() {
    let (_temp, config) = fixture();
    fs::write(config.raw.join("loose.txt"), "loose").unwrap();
    let stats = op(&config, "compat_kb_stats", json!({}));
    assert_eq!(stats["articles"], 2);
    assert_eq!(stats["raw_documents"], 2);
    assert_eq!(stats["filed_outputs"], 0);
    assert_eq!(
        op(&config, "compat_kb_health", json!({}))["report"],
        Value::Null
    );
    fs::create_dir_all(&config.meta).unwrap();
    fs::write(
        config.meta.join("health.json"),
        "{\"unknown\":[7,null,false]}",
    )
    .unwrap();
    assert_eq!(
        op(&config, "compat_kb_health", json!({}))["report"],
        json!({"unknown":[7,null,false]})
    );
}
