use llmbase_core::{
    Error, Library, LibraryConfig,
    document::{Document, hash},
    maintenance::{FixOptions, LintOptions},
    profile::LanguageProfile,
};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

const ARTICLE: &str = "---\ntitle: Learning / 學習\nsummary: An accepted summary.\ntags: [practice, 'category:ethics']\nunknown:\n  array: [1, null, 文言]\n---\n## English\nLearning is a practice preserved in this accepted article. [[修養|cultivation]] is related.\n## 中文\n學而時習之，所學須踐履。此句須保全原貌。\n## 日本語\n学びは実践によって深まる。\n";
fn fixture() -> (TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("wiki/concepts")).unwrap();
    fs::write(dir.path().join("wiki/concepts/a.md"), ARTICLE).unwrap();
    fs::write(dir.path().join("wiki/concepts/b.md"),"---\ntitle: Cultivation / 修養\nsummary: A second accepted article.\ntags: [practice, 'category:ethics']\n---\nCultivation and [[Learning]] illuminate each other through sustained and thoughtful practice.\n").unwrap();
    fs::write(dir.path().join("wiki/concepts/secret.md"),"---\ntitle: PRIVATE TITLE\nprivate: true\nsummary: PRIVATE SUMMARY\ntags: [practice]\n---\nPRIVATE TEXT [[a]].\n").unwrap();
    fs::write(
        dir.path().join("wiki/index.md"),
        "Human authored index must remain unchanged.\n",
    )
    .unwrap();
    let mut library =
        Library::open(LibraryConfig::load(dir.path(), "maintenance-test").unwrap()).unwrap();
    library.sync().unwrap();
    (dir, library)
}
fn trilingual() -> LanguageProfile {
    serde_json::from_value(json!({"sections":[
    {"code":"en","key":"english","header":"## English"},
    {"code":"zh","key":"中文","header":"## 中文"},
    {"code":"ja","key":"日本語","header":"## 日本語"}],"default_lang":"en","api_default_lang":"zh"})).unwrap()
}

#[test]
fn native_exports_preserve_legacy_shapes_sections_aliases_and_private_boundaries() {
    let (_dir, library) = fixture();
    let profile = trilingual();
    let article = library.export_article("學習", &profile, false).unwrap();
    assert_eq!(article["slug"], "a");
    assert!(
        article["content"]["english"]
            .as_str()
            .unwrap()
            .starts_with("Learning is a practice")
    );
    assert!(
        article["content"]["zh"]
            .as_str()
            .unwrap()
            .contains("此句須保全原貌")
    );
    assert!(article["content"]["ja"].is_string());
    assert_eq!(article["tags"], json!(["practice"]));
    assert_eq!(
        article["outgoing_links"],
        json!([{"slug":"b","title":"Cultivation / 修養"}])
    );
    assert_eq!(article["backlinks"].as_array().unwrap().len(), 1);
    assert_eq!(article["related_by_tags"].as_array().unwrap().len(), 1);
    assert!(!article.to_string().contains("PRIVATE"));
    assert!(library.export_article("secret", &profile, false).is_err());
    assert_eq!(library.export_tag("PrAcTiCe", false).unwrap()["count"], 2);
    assert_eq!(library.export_tag("PrAcTiCe", true).unwrap()["count"], 3);
    let graph = library.export_graph("學習", 2, false).unwrap();
    assert_eq!(graph["root"], "a");
    assert_eq!(graph["count"], 2);
    assert_eq!(library.export_graph("a", 0, false).unwrap()["count"], 1);
    assert!(library.export_graph("a", 5, false).is_err());
    let bare:LanguageProfile=serde_json::from_value(json!({"sections":[{"code":"wenyan","key":"文言","header":""}],"default_lang":"wenyan","api_default_lang":"wenyan"})).unwrap();
    assert!(
        library.export_article("a", &bare, false).unwrap()["content"]["文言"]
            .as_str()
            .unwrap()
            .contains("## 中文")
    );
}

#[test]
fn rebuilding_real_legacy_files_preserves_manual_aliases_and_never_publishes_private_articles() {
    let (dir, mut library) = fixture();
    let original = fs::read(dir.path().join("wiki/concepts/a.md")).unwrap();
    fs::write(
        library.config.meta.join("aliases.json"),
        serde_json::to_vec(&json!({"human-override":"b","concealed-alias":"secret"})).unwrap(),
    )
    .unwrap();
    let report = library.rebuild_metadata().unwrap();
    assert_eq!(report["article_count"], 2);
    assert_eq!(report["indexed_article_count"], 3);
    assert_eq!(report["projection_visibility"], "public");
    let index: Value =
        serde_json::from_slice(&fs::read(library.config.meta.join("index.json")).unwrap()).unwrap();
    assert_eq!(index.as_array().unwrap().len(), 2);
    assert!(!index.to_string().contains("PRIVATE"));
    assert_eq!(index[0]["slug"], "a");
    assert!(index[0].get("sources").is_some());
    let aliases: Value =
        serde_json::from_slice(&fs::read(library.config.meta.join("aliases.json")).unwrap())
            .unwrap();
    assert_eq!(aliases["human-override"], "b");
    assert!(aliases.get("concealed-alias").is_none());
    assert_eq!(aliases["學習"], "a");
    assert_eq!(aliases["学习"], "a");
    let backlinks: Value =
        serde_json::from_slice(&fs::read(library.config.meta.join("backlinks.json")).unwrap())
            .unwrap();
    assert_eq!(backlinks["a"], json!(["b"]));
    let markdown = fs::read_to_string(library.config.meta.join("_index.md")).unwrap();
    assert!(markdown.contains("# Knowledge Base Index"));
    assert!(markdown.contains("[[a|Learning / 學習]]"));
    assert!(markdown.contains("*2 articles indexed*"));
    assert!(!markdown.contains("PRIVATE"));
    assert_eq!(
        fs::read(dir.path().join("wiki/concepts/a.md")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("wiki/index.md")).unwrap(),
        "Human authored index must remain unchanged.\n"
    );
    let manifest: Value = serde_json::from_slice(
        &fs::read(library.config.meta.join("projection-generation.json")).unwrap(),
    )
    .unwrap();
    for (name, expected) in manifest["files"].as_object().unwrap() {
        assert_eq!(
            hash(&fs::read(library.config.meta.join(name)).unwrap()),
            expected.as_str().unwrap()
        );
    }
    assert_eq!(
        library
            .export_article("human-override", &trilingual(), false)
            .unwrap()["slug"],
        "b"
    );
}

#[test]
fn journaled_projection_batches_recover_both_crash_windows_before_any_native_read() {
    for first_file_installed in [false, true] {
        let (_dir, mut library) = fixture();
        library.rebuild_metadata().unwrap();
        let config = library.config.clone();
        let db = rusqlite::Connection::open(config.database()).unwrap();
        let (id, payload): (String, String) = db
            .query_row(
                "SELECT id,artifacts FROM maintenance_projection_batches WHERE status='complete'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let artifacts: Vec<Value> = serde_json::from_str(&payload).unwrap();
        for (index, artifact) in artifacts.iter().enumerate() {
            if !(first_file_installed && index == 0) {
                fs::remove_file(config.meta.join(artifact["file"].as_str().unwrap())).unwrap();
            }
        }
        db.execute(
            "UPDATE maintenance_projection_batches SET status='pending' WHERE id=?1",
            [&id],
        )
        .unwrap();
        drop(db);
        drop(library);
        let library = Library::open(config).unwrap();
        for artifact in artifacts {
            assert_eq!(
                fs::read_to_string(library.config.meta.join(artifact["file"].as_str().unwrap()))
                    .unwrap(),
                artifact["content"].as_str().unwrap()
            );
        }
        let db = rusqlite::Connection::open(library.config.database()).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT status FROM maintenance_projection_batches WHERE id=?1",
                [&id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "complete"
        );
    }
}

#[test]
fn recovery_refuses_to_overwrite_new_manual_alias_edits() {
    let (_dir, mut library) = fixture();
    library.rebuild_metadata().unwrap();
    let config = library.config.clone();
    let db = rusqlite::Connection::open(config.database()).unwrap();
    db.execute(
        "UPDATE maintenance_projection_batches SET status='pending'",
        [],
    )
    .unwrap();
    drop(db);
    drop(library);
    let manual = b"{\"new-human-alias\":\"a\"}";
    fs::write(config.meta.join("aliases.json"), manual).unwrap();
    assert!(matches!(
        Library::open(config.clone()),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(config.meta.join("aliases.json")).unwrap(), manual);
}

#[test]
fn compatibility_rebuild_preserves_unknown_backlink_targets_but_excludes_private_aliases() {
    let (dir, mut library) = fixture();
    fs::write(
        dir.path().join("wiki/concepts/a.md"),
        format!("{ARTICLE}\n[[Missing Target]] [[PRIVATE TITLE]] [[secret]]\n"),
    )
    .unwrap();
    let entries = library.rebuild_metadata_compat().unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 2);
    let backlinks: Value =
        serde_json::from_slice(&fs::read(library.config.meta.join("backlinks.json")).unwrap())
            .unwrap();
    assert_eq!(backlinks["missing-target"], json!(["a"]));
    assert!(backlinks.get("secret").is_none());
    assert!(backlinks.get("private-title").is_none());
    let index_before = fs::read(library.config.meta.join("index.json")).unwrap();
    let markdown_before = fs::read(library.config.meta.join("_index.md")).unwrap();
    let aliases_before = fs::read(library.config.meta.join("aliases.json")).unwrap();
    fs::write(
        dir.path().join("wiki/concepts/a.md"),
        format!("{ARTICLE}\n[[New Unknown]]\n"),
    )
    .unwrap();
    let rebuilt = library.rebuild_backlinks_compat().unwrap();
    assert_eq!(rebuilt["new-unknown"], json!(["a"]));
    assert_eq!(
        fs::read(library.config.meta.join("index.json")).unwrap(),
        index_before
    );
    assert_eq!(
        fs::read(library.config.meta.join("_index.md")).unwrap(),
        markdown_before
    );
    assert_eq!(
        fs::read(library.config.meta.join("aliases.json")).unwrap(),
        aliases_before
    );
}

#[test]
fn deterministic_fix_uses_source_journal_preserves_body_unknown_fields_and_original_revision() {
    let (dir, mut library) = fixture();
    let path = dir.path().join("wiki/concepts/a.md");
    let dirty = ARTICLE.replace(
        "tags: [practice, 'category:ethics']",
        "tags: [' Practice ', practice, practice, 12]",
    );
    fs::write(&path, &dirty).unwrap();
    library.sync().unwrap();
    let before = library.get("a", None, false).unwrap();
    let secret_before = fs::read(dir.path().join("wiki/concepts/secret.md")).unwrap();
    let lint = library.lint(LintOptions::default(), false).unwrap();
    assert!(!lint["dirty_tags"].as_array().unwrap().is_empty());
    let result = library
        .fix_deterministic(FixOptions::default(), false)
        .unwrap();
    assert_eq!(result["status"], "succeeded");
    assert_eq!(result["fix_count"], 1);
    let after = library.get(&before.source_id, None, false).unwrap();
    assert_eq!(after.metadata["tags"], json!(["practice", 12]));
    assert_eq!(after.metadata["unknown"], before.metadata["unknown"]);
    assert_eq!(
        &after.content[after.body_start..],
        &before.content[before.body_start..]
    );
    assert_eq!(
        library
            .get(&before.source_id, Some(before.revision), false)
            .unwrap()
            .content,
        dirty
    );
    assert_eq!(
        fs::read(dir.path().join("wiki/concepts/secret.md")).unwrap(),
        secret_before
    );
    assert!(Document::parse(fs::read(path).unwrap()).is_ok());
    let db = rusqlite::Connection::open(library.config.database()).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM write_intents WHERE status='applied'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        library
            .fix_deterministic(
                FixOptions {
                    normalize_tags: true,
                    rebuild_metadata: false
                },
                false
            )
            .unwrap()["fix_count"],
        0
    );
}

#[test]
fn stale_unindexed_source_edit_prevents_automatic_tag_overwrite() {
    let (dir, mut library) = fixture();
    let path = dir.path().join("wiki/concepts/a.md");
    fs::write(
        &path,
        ARTICLE.replace(
            "tags: [practice, 'category:ethics']",
            "tags: [' Practice ', practice]",
        ),
    )
    .unwrap();
    library.sync().unwrap();
    let external =
        "---\ntitle: New private edit\nprivate: true\n---\nAccepted by a human after indexing.";
    fs::write(&path, external).unwrap();
    assert!(matches!(
        library.fix_deterministic(
            FixOptions {
                normalize_tags: true,
                rebuild_metadata: false
            },
            false
        ),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
}

#[test]
fn lint_reports_real_links_metadata_duplicates_and_explicit_cjk_policy() {
    let (dir, mut library) = fixture();
    fs::write(
        dir.path().join("wiki/concepts/仁.md"),
        "---\ntitle: 仁\n---\n[[does-not-exist]]\n",
    )
    .unwrap();
    library.sync().unwrap();
    let strict = library.lint(LintOptions::default(), false).unwrap();
    assert!(
        strict["broken_links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("does-not-exist"))
    );
    assert!(
        strict["missing_metadata"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "Missing summary: 仁")
    );
    assert!(
        strict["duplicates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("a <-> b"))
    );
    assert!(
        strict["stubs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().starts_with("CJK slug"))
    );
    let allowed = library
        .lint(
            LintOptions {
                allow_cjk_slugs: true,
            },
            false,
        )
        .unwrap();
    assert!(
        !allowed["stubs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().starts_with("CJK slug"))
    );
    assert!(!allowed.to_string().contains("PRIVATE"));
    assert_eq!(
        strict["total_issues"].as_u64().unwrap(),
        strict
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(_, v)| v.as_array())
            .map(|v| v.len() as u64)
            .sum::<u64>()
    );
}

#[test]
fn xici_cache_reads_validate_language_and_do_not_expose_unproven_private_aggregates() {
    let (_dir, library) = fixture();
    assert_eq!(
        library.xici("wenyan", false).unwrap()["origin"],
        "not_generated"
    );
    let path = library.config.meta.join("xici-wenyan.json");
    let cached = json!({"text":"A genuine cached introduction.","themes":["learning"],"lang":"wenyan","generated_at":"2026-09-20","article_count":3});
    fs::write(&path, serde_json::to_vec(&cached).unwrap()).unwrap();
    assert!(matches!(
        library.xici("wenyan", false),
        Err(Error::Capability(_))
    ));
    assert_eq!(
        library.xici("wenyan", true).unwrap()["text"],
        cached["text"]
    );
    let public_source = library.get("a", None, false).unwrap();
    let mut public = cached.clone();
    public["source_ids"] = json!([public_source.source_id]);
    public["article_count"] = json!(1);
    fs::write(&path, serde_json::to_vec(&public).unwrap()).unwrap();
    assert_eq!(
        library.xici("wenyan", false).unwrap()["text"],
        cached["text"]
    );
    assert!(library.xici("../private", true).is_err());
    fs::write(&path, "{invalid").unwrap();
    assert!(matches!(library.xici("wenyan", true), Err(Error::Json(_))));
}
