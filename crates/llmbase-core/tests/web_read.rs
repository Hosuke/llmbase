use llmbase_core::{Error, Library, LibraryConfig, web_read};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn fixture(config: Option<Value>) -> (TempDir, Library) {
    let root = tempfile::tempdir().unwrap();
    for dir in ["wiki/concepts", "raw/book", "raw/hidden", "wiki/outputs"] {
        fs::create_dir_all(root.path().join(dir)).unwrap();
    }
    fs::write(root.path().join("wiki/concepts/a.md"),"---\ntitle: Learning / 學習\nsummary: Practice matters\ntags: [practice]\nsources:\n  - url: 'javascript:bad'\n    title: Source\n---\n## English\nLearning through [[b]].\n## 中文\n𠮷學之。\n").unwrap();
    fs::write(root.path().join("wiki/concepts/b.md"),"---\ntitle: Second article\nsummary: Other practice\ntags: [practice]\n---\n## English\nA return link [[a]].\n").unwrap();
    fs::write(
        root.path().join("wiki/concepts/secret.md"),
        "---\ntitle: SECRET\nprivate: true\n---\nSECRET [[a]].",
    )
    .unwrap();
    fs::write(root.path().join("raw/book/index.md"),"---\ntitle: Original book\ntype: book\ncompiled: false\nwork: accepted\nlocal_path: /secret/operator/path\n---\n𠮷原典 body beyond limit.").unwrap();
    fs::write(
        root.path().join("raw/hidden/index.md"),
        "---\ntitle: SECRET RAW\nprivate: true\n---\nSECRET",
    )
    .unwrap();
    fs::write(
        root.path().join("wiki/outputs/note.md"),
        "---\ntitle: Note\n---\nPublic note",
    )
    .unwrap();
    if let Some(config) = config {
        // Write JSON (a supported config syntax) rather than serde_yaml output:
        // serializing a serde_json::Value with arbitrary_precision through
        // serde_yaml turns numbers into `$serde_json::private::Number` maps,
        // so the loaded setting would not be an integer.
        fs::write(
            root.path().join("config.yaml"),
            serde_json::to_string(&config).unwrap(),
        )
        .unwrap();
    }
    let mut library = Library::open(LibraryConfig::load(root.path(), "web").unwrap()).unwrap();
    library.rebuild_metadata().unwrap();
    (root, library)
}
fn call(lib: &mut Library, op: &str, args: Value, private: bool) -> Value {
    web_read::execute(lib, op, &args, private).unwrap()
}

#[test]
fn web_config_whitelists_secrets_preserves_legacy_defaults_and_custom_language_views() {
    let (_root, mut library) = fixture(Some(
        json!({"branding":{"name":"Scholar","name_short":"S"},"llm":{"api_key":"SECRET-KEY"},"paths":{"raw":"raw"}}),
    ));
    let config = call(&mut library, "web_config", json!({}), false);
    assert_eq!(config["branding"]["name"], "Scholar");
    assert_eq!(
        config["languages"]["codes"],
        json!(["en", "zh", "ja", "zh-en"])
    );
    assert_eq!(config["languages"]["api_default_lang"], "zh");
    assert!(!config.to_string().contains("SECRET-KEY"));
    assert!(config.get("paths").is_none());
    let references = call(&mut library, "web_refs_plugins", json!({}), false);
    let plugins = references["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 3);
    assert_eq!(plugins[0]["id"], "cbeta");
    assert_eq!(plugins[1]["id"], "wikisource");
    assert_eq!(plugins[2]["id"], "ctext");
    assert_eq!(plugins[0]["name"]["en"], "CBETA Buddhist Canon");
    let (_root, mut custom) = fixture(Some(
        json!({"languages":{"default":"wenyan","profiles":{"wenyan":{"sections":[{"key":"文言","header":"","code":"wenyan"}]}}}}),
    ));
    let config = call(&mut custom, "web_config", json!({}), false);
    assert_eq!(config["languages"]["bare_body"], true);
    assert_eq!(config["languages"]["sections"][0]["label"], "文言");
    assert_eq!(
        call(&mut custom, "web_xici", json!({}), false)["lang"],
        "wenyan"
    );
    assert!(matches!(
        web_read::execute(&mut custom, "web_taxonomy", &json!({"lang":"zh"}), false),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn public_trails_returns_only_true_empty_and_never_withholds_nonempty_as_empty() {
    let (_root, mut library) = fixture(None);
    assert_eq!(
        call(&mut library, "web_trails", json!({}), false),
        json!({"trails":[]})
    );
    fs::write(
        library.config.meta.join("trails.json"),
        r#"{"trails":[],"private_metadata":"hidden"}"#,
    )
    .unwrap();
    assert_eq!(
        call(&mut library, "web_trails", json!({}), false),
        json!({"trails":[]})
    );
    fs::write(
        library.config.meta.join("trails.json"),
        r#"{"trails":[{"name":"Secret"}]}"#,
    )
    .unwrap();
    assert!(matches!(
        web_read::execute(&mut library, "web_trails", &json!({}), false),
        Err(Error::Scope)
    ));
    assert_eq!(
        call(&mut library, "web_trails", json!({}), true)["trails"][0]["name"],
        "Secret"
    );
}

#[test]
fn web_article_pagination_sections_raw_and_statistics_follow_real_content() {
    let (_root, mut library) = fixture(Some(
        json!({"sources":{"max_content_chars":3},"collections":{"practice":{"label":"Practice Collection"}}}),
    ));
    // The loaded cap must be a real integer, not an arbitrary_precision
    // mapping such as {"$serde_json::private::Number":"3"}.
    assert_eq!(
        library.config.settings["sources"]["max_content_chars"].as_i64(),
        Some(3)
    );
    let listed = call(&mut library, "web_articles", json!({}), false);
    assert_eq!(listed["articles"].as_array().unwrap().len(), 2);
    assert!(listed.get("total").is_none());
    let first = call(
        &mut library,
        "web_articles",
        json!({"limit":1,"tag":"practice","fields":"slug,title"}),
        false,
    );
    assert_eq!(first["next_cursor"], "a");
    assert_eq!(first["total"], 2);
    assert!(first["articles"][0].get("tags").is_none());
    let last = call(
        &mut library,
        "web_articles",
        json!({"limit":1,"cursor":"a"}),
        false,
    );
    assert_eq!(last["count"], 1);
    assert!(last["next_cursor"].is_null());
    assert_eq!(
        call(&mut library, "web_articles", json!({"q":"other"}), false)["count"],
        1
    );
    assert_eq!(
        call(
            &mut library,
            "web_articles",
            json!({"lite":true,"tag":"missing"}),
            false
        ),
        json!({"articles":[],"total":0})
    );
    let article = call(&mut library, "web_article", json!({"slug":"學習"}), false);
    assert_eq!(article["slug"], "a");
    assert_eq!(article["sources"][0]["url"], "");
    assert_eq!(
        article["backlinks"],
        json!([{"slug":"b","title":"Second article"}])
    );
    let sections = call(
        &mut library,
        "web_article",
        json!({"slug":"a","sections":true}),
        false,
    );
    assert_eq!(sections["sections"].as_array().unwrap().len(), 2);
    let content = article["content"].as_str().unwrap();
    let zh = &sections["sections"][1];
    let start = zh["start"].as_u64().unwrap() as usize;
    let end = zh["end"].as_u64().unwrap() as usize;
    assert!(
        content
            .chars()
            .skip(start)
            .take(end - start)
            .collect::<String>()
            .contains("𠮷學之")
    );
    let raw = call(&mut library, "web_sources", json!({}), false);
    assert_eq!(raw["documents"][0]["path"], "book");
    assert_eq!(raw["documents"][0]["work"], "accepted");
    assert!(raw["documents"][0].get("local_path").is_none());
    let detail = call(&mut library, "web_source", json!({"slug":"book"}), false);
    assert_eq!(detail["content"], "𠮷原典");
    assert_eq!(detail["metadata"]["compiled"], "False");
    let stats = call(&mut library, "web_stats", json!({}), false);
    assert_eq!(stats["article_count"], 2);
    assert_eq!(stats["raw_count"], 1);
    assert_eq!(stats["output_count"], 1);
    assert_eq!(stats["link_count"], 2);
    assert_eq!(stats["health_score"], 100);
    assert_eq!(
        call(&mut library, "web_collections", json!({}), false)["collections"][0]["label"],
        "Practice Collection"
    );
}

#[test]
fn web_reads_fail_closed_after_unindexed_private_edit_and_filter_private_caches() {
    let (root, mut library) = fixture(None);
    fs::write(library.config.meta.join("health.json"),r#"{"checked_at":"today","results":{"total_issues":1,"secret":"SECRET"},"fixes_applied":[]}"#).unwrap();
    assert!(call(&mut library, "web_health", json!({}), false)["report"].is_null());
    assert_eq!(
        call(&mut library, "web_health", json!({}), true)["report"]["results"]["secret"],
        "SECRET"
    );
    fs::write(
        library.config.meta.join("trails.json"),
        r#"{"trails":[{"id":"accepted","name":"SECRET"}]}"#,
    )
    .unwrap();
    assert!(matches!(
        web_read::execute(&mut library, "web_trails", &json!({}), false),
        Err(Error::Scope)
    ));
    assert_eq!(
        call(&mut library, "web_trails", json!({}), true)["trails"][0]["id"],
        "accepted"
    );
    assert!(matches!(
        web_read::execute(
            &mut library,
            "web_source",
            &json!({"slug":"../secret"}),
            true
        ),
        Err(Error::Invalid(_))
    ));
    fs::write(
        root.path().join("wiki/concepts/a.md"),
        "---\nprivate: true\n---\nNew SECRET",
    )
    .unwrap();
    for op in [
        "web_articles",
        "web_stats",
        "web_entities",
        "web_health",
        "web_aliases",
    ] {
        assert!(
            matches!(
                web_read::execute(&mut library, op, &json!({}), false),
                Err(Error::NotFound(_))
            ),
            "{op}"
        );
    }
    library.sync().unwrap();
    assert_eq!(
        call(&mut library, "web_articles", json!({}), false)["articles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn web_source_literal_yaml_cap_loads_integer_and_preserves_raw_bytes() {
    let (root, library) = fixture(None);
    drop(library);
    fs::write(
        root.path().join("config.yaml"),
        "sources:\n  max_content_chars: 3\n",
    )
    .unwrap();
    let mut library = Library::open(LibraryConfig::load(root.path(), "web").unwrap()).unwrap();
    assert_eq!(
        library.config.settings["sources"]["max_content_chars"].as_i64(),
        Some(3)
    );
    let detail = call(&mut library, "web_source", json!({"slug":"book"}), false);
    assert_eq!(detail["content"], "𠮷原典");
    assert_eq!(
        fs::read(root.path().join("raw/book/index.md")).unwrap(),
        "---\ntitle: Original book\ntype: book\ncompiled: false\nwork: accepted\nlocal_path: /secret/operator/path\n---\n𠮷原典 body beyond limit.".as_bytes()
    );
}
