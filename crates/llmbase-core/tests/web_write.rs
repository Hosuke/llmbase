use llmbase_core::{Error, Library, LibraryConfig, web_write};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, Library) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    fs::write(root.path().join("wiki/concepts/keep.md"),"---\ntitle: Keep accepted article\nsummary: Accepted by a human\ntags: [practice]\n---\nAn accepted article with enough real content to stay outside the legacy garbage predicates.\n").unwrap();
    fs::write(
        root.path().join("wiki/concepts/stub.md"),
        "---\ntitle: Short stub\n---\nShort body",
    )
    .unwrap();
    fs::write(
        root.path().join("wiki/concepts/private.md"),
        "---\ntitle: PRIVATE\nprivate: true\n---\nShort PRIVATE",
    )
    .unwrap();
    let mut library =
        Library::open(LibraryConfig::load(root.path(), "web-write").unwrap()).unwrap();
    library.sync().unwrap();
    (root, library)
}
#[test]
fn cleanup_and_delete_are_real_journaled_mutations_with_old_revisions_and_visibility() {
    let (root, mut library) = fixture();
    let stub = library.get("stub", None, false).unwrap();
    let private = fs::read(root.path().join("wiki/concepts/private.md")).unwrap();
    let report =
        web_write::execute(&mut library, "web_write_wiki_clean", &json!({}), false).unwrap();
    assert_eq!(report["removed"], 1);
    assert_eq!(report["slugs"], json!(["stub"]));
    assert!(!root.path().join("wiki/concepts/stub.md").exists());
    assert_eq!(
        library
            .get(&stub.source_id, Some(stub.revision), false)
            .unwrap()
            .content,
        stub.content
    );
    assert_eq!(
        fs::read(root.path().join("wiki/concepts/private.md")).unwrap(),
        private
    );
    assert!(matches!(
        web_write::execute(
            &mut library,
            "web_write_article_delete",
            &json!({"slug":"private"}),
            false
        ),
        Err(Error::NotFound(_))
    ));
    let accepted = library.get("keep", None, false).unwrap();
    assert_eq!(
        web_write::execute(
            &mut library,
            "web_write_article_delete",
            &json!({"slug":"keep"}),
            false
        )
        .unwrap()["deleted"],
        "keep"
    );
    assert_eq!(
        library
            .get(&accepted.source_id, Some(accepted.revision), false)
            .unwrap()
            .content,
        accepted.content
    );
    assert!(!root.path().join("wiki/concepts/keep.md").exists());
    let db = rusqlite::Connection::open(library.config.database()).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM write_intents WHERE status='applied'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        2
    );
}
#[test]
fn stale_disk_edit_aborts_delete_and_manual_taxonomy_is_preserved_in_native_history() {
    let (root, mut library) = fixture();
    let document = json!({"categories":[{"id":"practice","label":{"en":"Practice"},"article_slugs":["keep"]}],"custom":{"accepted":"verbatim"}});
    let updated = web_write::execute(
        &mut library,
        "web_write_taxonomy_update",
        &json!({"document":document}),
        false,
    )
    .unwrap();
    assert_eq!(updated["locked"], true);
    assert_eq!(updated["status"], "ok");
    let db = rusqlite::Connection::open(library.config.database()).unwrap();
    let payload: String = db
        .query_row("SELECT payload FROM catalog_document_revisions", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap()["custom"],
        document["custom"]
    );
    let changed = "---\nprivate: true\n---\nManual accepted change";
    fs::write(root.path().join("wiki/concepts/keep.md"), changed).unwrap();
    assert!(matches!(
        web_write::execute(
            &mut library,
            "web_write_article_delete",
            &json!({"slug":"keep"}),
            false
        ),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        fs::read_to_string(root.path().join("wiki/concepts/keep.md")).unwrap(),
        changed
    );
}
#[test]
fn trails_persist_real_steps_timestamps_unknown_fields_revision_history_and_recovery() {
    let (_root, mut library) = fixture();
    let path = library.config.meta.join("trails.json");
    let original = r#"{"trails":[],"custom":{"accepted":[1,null,"文言"]}}"#;
    fs::write(&path, original).unwrap();
    let result=web_write::execute(&mut library,"web_write_trail_save",&json!({"name":"Human research","step":{"type":"article","slug":"keep","ts":"forged","extra":42}}),true).unwrap();
    let id = result["trail"]["id"].as_str().unwrap();
    assert_ne!(result["trail"]["steps"][0]["ts"], "forged");
    assert_eq!(result["trail"]["steps"][0]["extra"], 42);
    web_write::execute(
        &mut library,
        "web_write_trail_save",
        &json!({"trail_id":id,"step":{"type":"query","question":"why"}}),
        true,
    )
    .unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["custom"]["accepted"], json!([1, null, "文言"]));
    assert_eq!(saved["trails"][0]["steps"].as_array().unwrap().len(), 2);
    assert!(matches!(
        web_write::execute(
            &mut library,
            "web_write_trail_delete",
            &json!({"trail_id":id}),
            false
        ),
        Err(Error::Scope)
    ));
    web_write::execute(
        &mut library,
        "web_write_trail_delete",
        &json!({"trail_id":id}),
        true,
    )
    .unwrap();
    assert!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap()["trails"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let config = library.config.clone();
    let db = rusqlite::Connection::open(config.database()).unwrap();
    let first: Vec<u8> = db
        .query_row(
            "SELECT original FROM maintenance_trail_revisions ORDER BY revision LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(first, original.as_bytes());
    let pending:(String,String)=db.query_row("SELECT id,artifacts FROM maintenance_projection_batches WHERE artifacts LIKE '%trails.json%' ORDER BY created_at DESC,rowid DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let artifacts: Value = serde_json::from_str(&pending.1).unwrap();
    let expected = artifacts[0]["expected"].as_str().unwrap();
    let before: Vec<u8> = db
        .query_row(
            "SELECT original FROM maintenance_trail_revisions WHERE hash=?1 LIMIT 1",
            [expected],
            |r| r.get(0),
        )
        .unwrap();
    fs::write(&path, before).unwrap();
    db.execute(
        "UPDATE maintenance_projection_batches SET status='pending' WHERE id=?1",
        [pending.0],
    )
    .unwrap();
    drop(db);
    drop(library);
    let _reopened = Library::open(config).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        artifacts[0]["content"].as_str().unwrap()
    );
}
