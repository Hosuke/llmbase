use llmbase_core::document::{byte_to_scalar, scalar_to_byte};
use llmbase_core::{Error, Library, LibraryConfig};
use std::fs;
use tempfile::TempDir;

fn library(name: &str) -> (TempDir, Library) {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("raw/classics")).unwrap();
    let config = LibraryConfig::load(temp.path(), name).unwrap();
    (temp, Library::open(config).unwrap())
}

const CLASSIC: &str = "---\ntitle: 論語 / Analects\naliases: [学而, 學而]\nsource: https://example.invalid/analects\nunknown:\n  preserved: [甲, null, 42]\n  date: 2026-09-20\n---\n# 學而第一\n子曰：學而時習之，不亦說乎？𠀀 e\u{301}，禮。\n";

#[test]
fn accepted_metadata_preserves_nested_numbers_without_serializer_markers() {
    use llmbase_core::document::{Document, yaml_metadata};
    let metadata: serde_json::Value = serde_json::from_str(r#"{"title":"數","unknown":{"small":42,"negative":-9,"fraction":1.25,"huge":123456789012345678901234567890,"array":[true,null,"42"]}}"#).unwrap();
    let header = yaml_metadata(&metadata).unwrap();
    assert!(!header.contains("$serde_json"));
    let body = "𠀀 e\u{301}\r\n原文。\n";
    let parsed = Document::parse(format!("---\n{header}---\n{body}").into_bytes()).unwrap();
    assert_eq!(parsed.metadata, metadata);
    assert_eq!(&parsed.original[parsed.body_start..], body);
    assert_eq!(
        parsed.metadata["unknown"]["huge"].to_string(),
        "123456789012345678901234567890"
    );
}

#[test]
fn unsynced_private_metadata_blocks_public_reads_and_writes_across_native_operations() {
    use llmbase_core::{
        application::{Application, Capability, Principal, Request},
        document::hash,
    };
    use serde_json::json;
    let (dir, mut library) = library("privacy");
    let path = dir.path().join("raw/classics/a.md");
    fs::write(&path, CLASSIC).unwrap();
    library.sync().unwrap();
    let evidence = library
        .search("學而", 1, Some("raw"), false)
        .unwrap()
        .results[0]
        .evidence
        .clone();
    let private = CLASSIC.replacen("title:", "private: true\ntitle:", 1);
    fs::write(&path, &private).unwrap();
    assert!(library.get(&evidence.source_id, None, false).is_err());
    assert!(library.get(&evidence.source_id, Some(1), false).is_err());
    assert!(library.search("學而", 10, Some("raw"), false).is_err());
    assert!(library.resolve_evidence(&evidence, false).is_err());
    let app = Application::new(vec![LibraryConfig::load(dir.path(), "privacy").unwrap()]).unwrap();
    let mut writer = Principal::public("privacy");
    writer.capabilities.insert(Capability::Write);
    assert!(app.execute(&writer,Request {
        library_id:"privacy".into(),operation:"source_write".into(),
        args:json!({"kind":"raw","path":"classics/a.md","content":CLASSIC,"expected_hash":hash(private.as_bytes())}),
    }).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), private);
}

#[test]
fn original_revision_evidence_survives_update_move_and_deletion() {
    let (dir, mut lib) = library("siwen");
    let path = dir.path().join("raw/classics/analects.md");
    fs::write(&path, CLASSIC).unwrap();
    let report = lib.sync().unwrap();
    assert_eq!(report.added, 1);
    assert_eq!(fs::read(&path).unwrap(), CLASSIC.as_bytes());
    let result = lib.search("学而时习之", 10, Some("raw"), false).unwrap();
    assert_eq!(result.mode, "lexical");
    assert_eq!(result.results.len(), 1);
    let evidence = &result.results[0].evidence;
    assert_eq!(&CLASSIC[evidence.start..evidence.end], evidence.exact_quote);
    assert!(!lib.resolve_evidence(evidence, false).unwrap().stale);
    let source = lib.get("学而", None, false).unwrap();
    assert_eq!(source.metadata["unknown"]["preserved"][2], 42);
    assert_eq!(source.metadata["unknown"]["date"], "2026-09-20");
    assert_eq!(lib.sync().unwrap().passages_written, 0);
    fs::write(&path, CLASSIC.replace("不亦說乎", "有朋自遠方來")).unwrap();
    assert_eq!(lib.sync().unwrap().updated, 1);
    assert!(lib.resolve_evidence(evidence, false).unwrap().stale);
    assert_eq!(lib.get(&source.source_id, None, false).unwrap().revision, 2);
    let renamed = dir.path().join("raw/classics/renamed.md");
    fs::rename(&path, &renamed).unwrap();
    assert_eq!(lib.sync().unwrap().moved, 1);
    assert_eq!(lib.get(&source.source_id, None, false).unwrap().revision, 3);
    assert_eq!(
        lib.get(&source.source_id, None, false).unwrap().rel_path,
        "classics/renamed.md"
    );
    fs::remove_file(renamed).unwrap();
    assert_eq!(lib.sync().unwrap().deleted, 1);
    assert!(
        lib.search("學而", 10, None, false)
            .unwrap()
            .results
            .is_empty()
    );
    assert!(lib.resolve_evidence(evidence, false).unwrap().deleted);
    assert_eq!(
        lib.get(&source.source_id, Some(1), false).unwrap().content,
        CLASSIC
    );
}

#[test]
fn index_failure_does_not_publish_partial_changes_or_deletions() {
    let (dir, mut lib) = library("test");
    let path = dir.path().join("raw/classics/a.md");
    fs::write(&path, CLASSIC).unwrap();
    lib.sync().unwrap();
    fs::write(&path, "different replacement").unwrap();
    fs::write(
        dir.path().join("raw/classics/bad.md"),
        "---\ntitle: [broken\n---\nbody",
    )
    .unwrap();
    assert!(lib.sync().is_err());
    assert_eq!(lib.get("学而", None, false).unwrap().revision, 1);
    assert_eq!(
        lib.search("學而", 10, None, false).unwrap().results.len(),
        1
    );
}

#[test]
fn scope_alias_visibility_and_forged_citation_boundaries() {
    let (a, mut first) = library("siwen");
    let (b, mut second) = library("taixu");
    fs::write(a.path().join("raw/classics/a.md"), CLASSIC).unwrap();
    fs::write(
        b.path().join("raw/classics/a.md"),
        CLASSIC.replace("學而時習之", "般若波羅蜜"),
    )
    .unwrap();
    first.sync().unwrap();
    second.sync().unwrap();
    let evidence = first.search("學而", 10, None, false).unwrap().results[0]
        .evidence
        .clone();
    assert!(matches!(
        second.resolve_evidence(&evidence, false),
        Err(Error::Scope)
    ));
    assert!(second.get(&evidence.source_id, None, false).is_err());
    assert!(matches!(
        Library::open(LibraryConfig::load(a.path(), "taixu").unwrap()),
        Err(Error::Scope)
    ));
    let mut forged = evidence.clone();
    forged.exact_quote = "nonexistent".into();
    assert!(first.resolve_evidence(&forged, false).is_err());
    fs::write(
        a.path().join("raw/classics/private.md"),
        "---\ntitle: Secret\nvisibility: private\n---\nhidden confidential sentence",
    )
    .unwrap();
    first.sync().unwrap();
    assert!(
        first
            .search("confidential", 10, None, false)
            .unwrap()
            .results
            .is_empty()
    );
    assert_eq!(
        first
            .search("confidential", 10, None, true)
            .unwrap()
            .results
            .len(),
        1
    );
    assert!(first.get("Secret", None, false).is_err());
    fs::write(a.path().join("raw/classics/collision.md"), CLASSIC).unwrap();
    first.sync().unwrap();
    assert!(matches!(
        first.get("学而", None, false),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn old_revision_keeps_its_title_and_privacy_after_publication_and_rename() {
    let (dir, mut lib) = library("historical");
    let path = dir.path().join("raw/classics/private.md");
    fs::write(
        &path,
        "---\ntitle: Confidential original\nprivate: true\n---\nSecret sentence.",
    )
    .unwrap();
    lib.sync().unwrap();
    let old = lib.get("private", None, true).unwrap();
    let evidence = lib.search("Secret", 10, None, true).unwrap().results[0]
        .evidence
        .clone();
    fs::write(
        &path,
        "---\ntitle: Public replacement\n---\nPublic sentence.",
    )
    .unwrap();
    lib.sync().unwrap();
    fs::rename(path, dir.path().join("raw/classics/published.md")).unwrap();
    lib.sync().unwrap();
    let current = lib.get(&old.source_id, None, false).unwrap();
    assert_eq!(current.slug, "published");
    assert_eq!(current.title, "Public replacement");
    let historical = lib.get(&old.source_id, Some(1), true).unwrap();
    assert_eq!(historical.title, "Confidential original");
    assert_eq!(historical.slug, "private");
    assert!(matches!(
        lib.get(&old.source_id, Some(1), false),
        Err(Error::NotFound(_))
    ));
    assert!(lib.resolve_evidence(&evidence, false).is_err());
    assert!(lib.resolve_evidence(&evidence, true).unwrap().stale);
}

#[test]
fn unicode_offsets_and_chunk_boundaries_are_original_bytes() {
    let (dir, mut lib) = library("unicode");
    lib.config.chunk_chars = 16;
    let text = "𠀀甲乙丙丁戊己庚辛壬癸。e\u{301}。𠮷𠮷𠮷繁體禮樂，カタカナ。";
    fs::write(dir.path().join("raw/classics/unicode.md"), text).unwrap();
    lib.sync().unwrap();
    let hits = lib.search("𠀀𠮷礼乐", 100, None, false).unwrap().results;
    assert!(hits.len() > 1);
    for hit in hits {
        let ev = hit.evidence;
        assert_eq!(text.get(ev.start..ev.end), Some(ev.exact_quote.as_str()));
        assert!(lib.resolve_evidence(&ev, false).is_ok());
        let scalar = byte_to_scalar(text, ev.end).unwrap();
        assert_eq!(scalar_to_byte(text, scalar).unwrap(), ev.end);
    }
    assert!(byte_to_scalar(text, 1).is_err());
}

#[test]
fn custom_data_roots_and_index_fingerprint_only_rebuild_affected_sources() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("documents")).unwrap();
    fs::write(
        dir.path().join("config.yaml"),
        "paths:\n  raw: documents\n  meta: state\n",
    )
    .unwrap();
    fs::write(dir.path().join("documents/a.md"), CLASSIC).unwrap();
    fs::write(
        dir.path().join("documents/b.md"),
        "independent English document",
    )
    .unwrap();
    let mut lib = Library::open(LibraryConfig::load(dir.path(), "custom").unwrap()).unwrap();
    assert_eq!(lib.sync().unwrap().added, 2);
    fs::write(
        dir.path().join("documents/b.md"),
        "updated English document",
    )
    .unwrap();
    let report = lib.sync().unwrap();
    assert_eq!(report.updated, 1);
    assert_eq!(report.unchanged, 1);
    lib.config.chunk_chars = 32;
    assert_eq!(lib.sync().unwrap().updated, 2);
    assert_eq!(lib.get("學而", None, false).unwrap().revision, 1);
    assert_eq!(lib.sync().unwrap().passages_written, 0);
}

#[test]
fn application_denies_scope_and_capabilities_before_core_execution() {
    use llmbase_core::application::{Application, Principal, Request};
    use serde_json::json;
    let (dir, mut lib) = library("scope");
    fs::write(dir.path().join("raw/classics/a.md"), CLASSIC).unwrap();
    lib.sync().unwrap();
    let app = Application::new(vec![lib.config.clone()]).unwrap();
    let public = Principal::public("scope");
    let request = |library: &str, operation: &str, args| Request {
        library_id: library.into(),
        operation: operation.into(),
        args,
    };
    assert!(matches!(
        app.execute(&public, request("scope", "index", json!({}))),
        Err(Error::Scope)
    ));
    assert!(matches!(
        app.execute(
            &public,
            request("other", "kb_search_raw", json!({"query":"學而"}))
        ),
        Err(Error::Scope)
    ));
    assert!(
        app.execute(
            &public,
            request("scope", "kb_search_raw", json!({"query":"學而","top_k":-1}))
        )
        .is_err()
    );
    let result = app
        .execute(
            &public,
            request("scope", "kb_search_raw", json!({"query":"學而"})),
        )
        .unwrap();
    assert_eq!(result["results"][0]["evidence"]["library_id"], "scope");
}

#[test]
fn cross_process_lock_is_not_a_pid_file_and_survives_stale_contents() {
    use fs2::FileExt;
    use std::fs::OpenOptions;
    let (dir, mut lib) = library("locks");
    let path = lib.config.meta.join("llmbase.writer.lock");
    // Arbitrary old PID metadata cannot take ownership; kernel ownership is authoritative.
    fs::write(&path, "{\"pid\":1,\"started_at\":\"1970-01-01\"}").unwrap();
    fs::write(dir.path().join("raw/classics/a.md"), CLASSIC).unwrap();
    assert_eq!(lib.sync().unwrap().added, 1);
    let held = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    held.lock_exclusive().unwrap();
    assert!(matches!(lib.sync(), Err(Error::Busy)));
    FileExt::unlock(&held).unwrap();
    assert_eq!(lib.sync().unwrap().unchanged, 1);
}

#[test]
fn explicit_python_writer_session_allows_nested_native_work_but_excludes_other_threads() {
    use llmbase_core::store::WriterSession;
    let (dir, mut lib) = library("writer-session");
    let config = lib.config.clone();
    let session = WriterSession::acquire(&config).unwrap();
    let inner = WriterSession::acquire(&config).unwrap();
    lib.write_source(
        "raw",
        "callback.md",
        b"accepted from callback".to_vec(),
        None,
    )
    .unwrap();
    let other = config.clone();
    assert!(
        std::thread::spawn(move || matches!(Library::open(other), Err(Error::Busy)))
            .join()
            .unwrap()
    );
    drop(session);
    let other = config.clone();
    assert!(
        std::thread::spawn(move || matches!(Library::open(other), Err(Error::Busy)))
            .join()
            .unwrap()
    );
    drop(inner);
    assert!(
        std::thread::spawn(move || Library::open(config).is_ok())
            .join()
            .unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("raw/callback.md")).unwrap(),
        "accepted from callback"
    );
}
