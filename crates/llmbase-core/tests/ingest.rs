use llmbase_core::{
    Error, Library, LibraryConfig,
    application::{Application, Principal, Request},
    document::hash,
    ingest::TextRequest,
};
use serde_json::json;
use std::fs;

#[test]
fn accepted_text_keeps_all_original_bytes_unknown_metadata_and_idempotent_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut library = Library::open(LibraryConfig::load(root.path(), "ingest").unwrap()).unwrap();
    let bytes =
        "---\r\ntitle: 文言\r\nunknown: {count: 42}\r\n---\r\n學而時習之。𠮷野 e\u{301}\r\n ";
    let request = TextRequest {
        filename: "My Source.md".into(),
        content: bytes.into(),
        expected_hash: None,
    };
    let first = library.ingest_text(request.clone()).unwrap();
    assert_eq!(first["path"], "my-source/My Source.md");
    assert_eq!(
        fs::read(root.path().join("raw/my-source/My Source.md")).unwrap(),
        bytes.as_bytes()
    );
    assert_eq!(first["content_hash"], hash(bytes.as_bytes()));
    let second = library.ingest_text(request).unwrap();
    assert_eq!(second["source_id"], first["source_id"]);
    assert_eq!(second["unchanged"], true);
    let source = library
        .get(first["source_id"].as_str().unwrap(), None, true)
        .unwrap();
    assert_eq!(source.revision, 1);
    assert_eq!(source.metadata["unknown"]["count"], 42);
}

#[test]
fn filename_scope_and_existing_content_require_explicit_cas() {
    let root = tempfile::tempdir().unwrap();
    let mut library = Library::open(LibraryConfig::load(root.path(), "ingest").unwrap()).unwrap();
    for filename in ["../secret.md", "nested/file.md", "..", ".md", "file.pdf"] {
        assert!(
            library
                .ingest_text(TextRequest {
                    filename: filename.into(),
                    content: "x".into(),
                    expected_hash: None
                })
                .is_err()
        );
    }
    let first = library
        .ingest_text(TextRequest {
            filename: "資料.txt".into(),
            content: "original".into(),
            expected_hash: None,
        })
        .unwrap();
    assert_eq!(first["path"], "資料/index.md");
    assert!(matches!(
        library.ingest_text(TextRequest {
            filename: "資料.txt".into(),
            content: "changed".into(),
            expected_hash: None
        }),
        Err(Error::Conflict(_))
    ));
    library
        .ingest_text(TextRequest {
            filename: "資料.txt".into(),
            content: "changed".into(),
            expected_hash: Some(hash(b"original")),
        })
        .unwrap();
    assert_eq!(
        library
            .get(first["source_id"].as_str().unwrap(), None, true)
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        library
            .get(first["source_id"].as_str().unwrap(), Some(1), true)
            .unwrap()
            .content,
        "original"
    );
}

#[test]
fn public_caller_cannot_ingest_or_choose_a_server_filesystem_path() {
    let root = tempfile::tempdir().unwrap();
    let app = Application::new(vec![LibraryConfig::load(root.path(), "ingest").unwrap()]).unwrap();
    assert!(matches!(
        app.execute(
            &Principal::public("ingest"),
            Request {
                library_id: "ingest".into(),
                operation: "ingest_text".into(),
                args: json!({"filename":"x.md","content":"x"})
            }
        ),
        Err(Error::Scope)
    ));
    assert!(
        app.execute(
            &Principal::local("ingest"),
            Request {
                library_id: "ingest".into(),
                operation: "ingest_text".into(),
                args: json!({"filename":"x.md","content":"x","server_path":"/etc/passwd"})
            }
        )
        .is_err()
    );
    assert!(!root.path().join("raw/x/x.md").exists());
}

#[test]
fn opaque_originals_keep_binary_versions_in_the_write_journal_and_stay_out_of_search() {
    let root = tempfile::tempdir().unwrap();
    let cfg = LibraryConfig::load(root.path(), "ingest").unwrap();
    let mut library = Library::open(cfg.clone()).unwrap();
    let first = vec![0, 255, 128, 1, 13, 10];
    let receipt = library
        .write_raw_asset("paper/source.original", first.clone(), None)
        .unwrap();
    assert_eq!(receipt["content_hash"], hash(&first));
    assert!(library.list(Some("raw"), true).unwrap().is_empty());
    let changed = vec![255, 0, 42];
    assert!(
        library
            .write_raw_asset("paper/source.original", changed.clone(), None)
            .is_err()
    );
    library
        .write_raw_asset(
            "paper/source.original",
            changed.clone(),
            Some(&hash(&first)),
        )
        .unwrap();
    drop(library);
    let _reopened = Library::open(cfg.clone()).unwrap();
    assert_eq!(
        fs::read(cfg.raw.join("paper/source.original")).unwrap(),
        changed
    );
    let history = rusqlite::Connection::open(cfg.database()).unwrap();
    let old: Vec<u8> = history
        .query_row(
            "SELECT content FROM write_intents WHERE id=?1",
            [receipt["intent_id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old, first);
    drop(history);
    let archives = tempfile::tempdir().unwrap();
    let backup = archives.path().join("backup");
    llmbase_core::migration::backup(&cfg, &backup).unwrap();
    let restored = archives.path().join("restored");
    llmbase_core::migration::restore(&backup, &restored).unwrap();
    assert_eq!(
        fs::read(restored.join("raw/paper/source.original")).unwrap(),
        changed
    );
}

#[test]
fn opaque_asset_recovery_does_not_index_unrelated_malformed_markdown() {
    let root = tempfile::tempdir().unwrap();
    let cfg = LibraryConfig::load(root.path(), "ingest").unwrap();
    let mut library = Library::open(cfg.clone()).unwrap();
    fs::create_dir_all(&cfg.concepts).unwrap();
    fs::write(
        cfg.concepts.join("malformed.md"),
        "---\nmalformed: [\n---\nbody",
    )
    .unwrap();
    library
        .write_raw_asset("source.original", vec![255], None)
        .unwrap();
    drop(library);
    let db = rusqlite::Connection::open(cfg.database()).unwrap();
    db.execute("INSERT INTO write_intents(id,library_id,kind,relative_path,operation,expected_hash,content) VALUES('asset-restart','ingest','raw','source.original','write_asset',?1,?2)",rusqlite::params![hash(&[255]),vec![0u8,42]]).unwrap();
    drop(db);
    let reopened = Library::open(cfg.clone()).unwrap();
    assert!(reopened.list(None, true).unwrap().is_empty());
    assert_eq!(
        fs::read(cfg.raw.join("source.original")).unwrap(),
        vec![0, 42]
    );
}
