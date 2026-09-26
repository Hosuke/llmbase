use llmbase_core::{
    Error, Library, LibraryConfig,
    compat_compile::{execute, transform},
    document::{Document, hash},
};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, LibraryConfig) {
    let dir = TempDir::new().unwrap();
    let cfg = LibraryConfig::load(dir.path(), "compat-compile").unwrap();
    fs::create_dir_all(&cfg.concepts).unwrap();
    fs::create_dir_all(cfg.raw.join("book")).unwrap();
    (dir, cfg)
}
fn headers() -> Value {
    json!([["文言", ""]])
}
fn article(slug: &str, content: &str) -> Value {
    json!({"slug":slug,"title":slug,"content":content,"summary":"summary","tags":["topic"]})
}
fn write(cfg: &LibraryConfig, article: Value) -> Value {
    execute(
        cfg,
        "compat_compile_write",
        &json!({"article":article,"headers":headers()}),
    )
    .unwrap()["value"]
        .clone()
}

#[test]
fn pure_tagged_parsing_preserves_legacy_order_fields_and_bare_sections() {
    let response = "===UPDATE===\nslug: old\nappend: |\n  新註𠮷\n===END===\n===ARTICLE===\nslug: https://host/a?b=1\ntitle: Source / 原典\ntags: a, b\n---\n文。\n===END===";
    let parsed = transform("parse_response", &json!({"response":response})).unwrap();
    assert_eq!(parsed[0]["type"], "new");
    assert_eq!(parsed[0]["content"], "文。");
    assert_eq!(
        parsed[1],
        json!({"slug":"old","content":"新註𠮷","type":"update"})
    );
    assert_eq!(
        transform("sanitize_slug", &json!({"slug":"https://host/a?b=1"})).unwrap(),
        "https-host-a-b=1"
    );
    let split = transform(
        "split",
        &json!({"content":"原典𠮷\n\n尾。","headers":headers()}),
    )
    .unwrap();
    assert_eq!(split, json!({"_preamble":"","文言":"原典𠮷\n\n尾。"}));
    assert_eq!(
        transform("assemble", &json!({"sections":split,"headers":headers()})).unwrap(),
        "原典𠮷\n\n尾。"
    );
    assert_eq!(
        transform("parse_article", &json!({"block":"title: no slug"})).unwrap(),
        Value::Null
    );
}

#[test]
fn read_helpers_keep_legacy_paths_without_creating_index_state() {
    let (_dir, cfg) = fixture();
    fs::write(
        cfg.raw.join("book/index.md"),
        "---\ntitle: Book\n---\nSource.",
    )
    .unwrap();
    fs::write(
        cfg.concepts.join("known.md"),
        "---\ntitle: Known / 知\n---\nArticle.",
    )
    .unwrap();
    let files = execute(&cfg, "compat_compile_find", &json!({})).unwrap();
    assert_eq!(files["value"], json!([cfg.raw.join("book/index.md")]));
    assert_eq!(
        execute(&cfg, "compat_compile_existing", &json!({})).unwrap()["value"],
        json!(["known (Known / 知)"])
    );
    assert!(!cfg.database().exists());
    assert!(!cfg.meta.exists());
}

#[test]
fn native_merge_preserves_exact_unchanged_bytes_and_legacy_sources_tags_rules() {
    let (_dir, cfg) = fixture();
    let source = json!({"plugin":"book","url":"https://example.test","title":"Source","extra":42});
    let first = write(
        &cfg,
        json!({"slug":"article","title":"Title","content":"abcde","sources":[source],"tags":["old"]}),
    );
    let path = cfg.concepts.join("article.md");
    let original = fs::read(&path).unwrap();
    write(
        &cfg,
        json!({"slug":"article","content":"abcdef","tags":["ignored"]}),
    );
    assert_eq!(fs::read(&path).unwrap(), original); // exactly 1.2 does not replace
    write(
        &cfg,
        json!({"slug":"article","content":"abcdefg𠮷","tags":["new"],"sources":[source]}),
    );
    let updated = Document::parse(fs::read(&path).unwrap()).unwrap();
    assert!(updated.original.ends_with("abcdefg𠮷"));
    assert_eq!(updated.metadata["tags"], json!(["new", "old"]));
    assert_eq!(updated.metadata["sources"].as_array().unwrap().len(), 1);
    assert_eq!(updated.metadata["sources"][0]["extra"], 42);
    assert_eq!(first["path"], json!(path));
}

#[test]
fn native_alias_and_cjk_resolution_reuses_existing_article() {
    let (_dir, cfg) = fixture();
    write(
        &cfg,
        json!({"slug":"ren","title":"Benevolence / 仁","content":"仁者愛人。","tags":[]}),
    );
    let alias = write(
        &cfg,
        json!({"slug":"another-name","title":"仁","content":"仁者愛人，推己及人，所論甚詳。","tags":[]}),
    );
    assert_eq!(alias["path"], json!(cfg.concepts.join("ren.md")));
    assert!(!cfg.concepts.join("another-name.md").exists());
    assert_eq!(fs::read_dir(&cfg.concepts).unwrap().count(), 1);
}

#[test]
fn legacy_raw_metadata_is_journaled_while_body_bytes_and_original_revision_are_retained() {
    let (_dir, cfg) = fixture();
    let raw = "---\r\ntitle: 原典\r\nunknown: 42\r\n---\r\n \r\n原典𠮷 café\r\n尾 \t\r\n";
    let path = cfg.raw.join("book/index.md");
    fs::write(&path, raw).unwrap();
    let old = Document::parse(raw.as_bytes().to_vec()).unwrap();
    execute(&cfg,"compat_compile_mark",&json!({"relative":"book/index.md","content_hash":hash(raw.as_bytes()),"compiled":true,"with_timestamp":true,"source_key":"source-key"})).unwrap();
    let new = Document::parse(fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        &new.original[new.body_start..],
        &old.original[old.body_start..]
    );
    assert_eq!(new.metadata["compiled"], true);
    assert_eq!(new.metadata["unknown"], 42);
    assert!(new.metadata["compiled_at"].is_string());
    assert_eq!(
        execute(&cfg, "compat_compile_sources", &json!({})).unwrap()["value"],
        json!(["source-key"])
    );
    let library = Library::open(cfg.clone()).unwrap();
    let source = &library.list(Some("raw"), true).unwrap()[0];
    assert_eq!(
        library
            .get(&source.source_id, Some(1), true)
            .unwrap()
            .content,
        raw
    );
    drop(library);
    execute(&cfg, "compat_compile_reset", &json!({})).unwrap();
    let reset = Document::parse(fs::read(path).unwrap()).unwrap();
    assert_eq!(reset.metadata["compiled"], false);
    assert_eq!(
        &reset.original[reset.body_start..],
        &old.original[old.body_start..]
    );
}

#[test]
fn raw_guard_rejects_changed_source_and_carries_private_visibility_into_article() {
    let (_dir, cfg) = fixture();
    let raw = "---\nprivate: true\n---\nConfidential source.";
    fs::write(cfg.raw.join("book/index.md"), raw).unwrap();
    let guard = json!({"relative":"book/index.md","content_hash":hash(raw.as_bytes())});
    execute(&cfg,"compat_compile_write",&json!({"article":article("private-result","Derived article."),"headers":headers(),"raw_source":guard})).unwrap();
    assert_eq!(
        Document::parse(fs::read(cfg.concepts.join("private-result.md")).unwrap())
            .unwrap()
            .metadata["visibility"],
        "private"
    );
    fs::write(cfg.raw.join("book/index.md"), "Changed source.").unwrap();
    assert!(matches!(
        execute(
            &cfg,
            "compat_compile_write",
            &json!({"article":article("blocked","Changed."),"headers":headers(),"raw_source":guard})
        ),
        Err(Error::Conflict(_))
    ));
    assert!(!cfg.concepts.join("blocked.md").exists());
    assert!(matches!(
        execute(
            &cfg,
            "compat_compile_mark",
            &json!({"relative":"book/index.md","content_hash":hash(raw.as_bytes())})
        ),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn rebuild_returns_legacy_shape_with_public_projection_and_unknown_backlinks() {
    let (_dir, cfg) = fixture();
    write(&cfg, article("public", "Public [[missing target]]."));
    fs::write(
        cfg.concepts.join("secret.md"),
        "---\nprivate: true\ntitle: Secret\n---\nRestricted.",
    )
    .unwrap();
    let entries = execute(&cfg, "compat_compile_rebuild", &json!({})).unwrap()["value"].clone();
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["slug"], "public");
    let backlinks: Value =
        serde_json::from_slice(&fs::read(cfg.meta.join("backlinks.json")).unwrap()).unwrap();
    assert_eq!(backlinks["missing-target"], json!(["public"]));
}

#[test]
fn malformed_raw_private_and_visibility_flags_fail_closed() {
    let (_dir, cfg) = fixture();
    let cases = [
        ("quoted-false", "private: \"false\"\n", "private"),
        ("quoted-yes", "private: \"yes\"\n", "private"),
        ("zero", "private: 0\n", "private"),
        ("empty", "private: \"\"\n", "private"),
        ("list", "private: []\n", "private"),
        ("map", "private: {}\n", "private"),
        ("yaml-on", "private: on\n", "private"),
        ("vis-zero", "visibility: 0\n", "private"),
        ("vis-caps", "visibility: PUBLIC\n", "private"),
        ("internal", "visibility: internal\n", "internal"),
        ("ctl-false", "private: false\n", "public"),
        ("ctl-null", "private:\n", "public"),
        ("ctl-public", "visibility: public\n", "public"),
        ("ctl-absent", "", "public"),
    ];
    for (slug, flags, expected) in cases {
        let raw = format!("---\ntitle: Raw {slug}\n{flags}---\nRaw body {slug}.");
        fs::write(cfg.raw.join("book/index.md"), &raw).unwrap();
        let guard = json!({"relative":"book/index.md","content_hash":hash(raw.as_bytes())});
        let result = execute(
            &cfg,
            "compat_compile_write",
            &json!({"article":article(&format!("derived-{slug}"),"Derived body."),"headers":headers(),"raw_source":guard}),
        )
        .unwrap();
        assert_eq!(result["value"]["changed"], json!(true), "{slug}");
        let doc =
            Document::parse(fs::read(cfg.concepts.join(format!("derived-{slug}.md"))).unwrap())
                .unwrap();
        assert_eq!(
            llmbase_core::visibility::visibility(&doc.metadata),
            expected,
            "{slug}"
        );
        if expected == "public" {
            assert!(doc.metadata.get("visibility").is_none(), "{slug}");
        } else {
            assert_eq!(doc.metadata["visibility"], json!(expected), "{slug}");
        }
    }
}

#[test]
fn raw_inheritance_never_downgrades_existing_target_restrictions() {
    let (_dir, cfg) = fixture();
    // Malformed visibility on the live target classifies as private; an internal
    // raw source must not downgrade it to internal.
    fs::write(
        cfg.concepts.join("target.md"),
        "---\ntitle: Target\nvisibility: 0\n---\nOld body.",
    )
    .unwrap();
    let raw = "---\nvisibility: internal\n---\nInternal source.";
    fs::write(cfg.raw.join("book/index.md"), raw).unwrap();
    let guard = json!({"relative":"book/index.md","content_hash":hash(raw.as_bytes())});
    execute(
        &cfg,
        "compat_compile_write",
        &json!({"article":article("target","New expanded body."),"headers":headers(),"raw_source":guard}),
    )
    .unwrap();
    let doc = Document::parse(fs::read(cfg.concepts.join("target.md")).unwrap()).unwrap();
    assert_eq!(doc.metadata["visibility"], json!(0));
    assert_eq!(
        llmbase_core::visibility::visibility(&doc.metadata),
        "private"
    );
    // Upgrade control: internal target + private raw becomes private.
    fs::write(
        cfg.concepts.join("up.md"),
        "---\ntitle: Up\nvisibility: internal\n---\nOld body.",
    )
    .unwrap();
    let raw = "---\nprivate: true\n---\nPrivate source.";
    fs::write(cfg.raw.join("book/index.md"), raw).unwrap();
    let guard = json!({"relative":"book/index.md","content_hash":hash(raw.as_bytes())});
    execute(
        &cfg,
        "compat_compile_write",
        &json!({"article":article("up","New expanded body."),"headers":headers(),"raw_source":guard}),
    )
    .unwrap();
    let doc = Document::parse(fs::read(cfg.concepts.join("up.md")).unwrap()).unwrap();
    assert_eq!(doc.metadata["visibility"], json!("private"));
    // Retention control: internal target + public raw stays internal.
    fs::write(
        cfg.concepts.join("keep.md"),
        "---\ntitle: Keep\nvisibility: internal\n---\nOld body.",
    )
    .unwrap();
    let raw = "---\ntitle: Raw\n---\nPublic source.";
    fs::write(cfg.raw.join("book/index.md"), raw).unwrap();
    let guard = json!({"relative":"book/index.md","content_hash":hash(raw.as_bytes())});
    execute(
        &cfg,
        "compat_compile_write",
        &json!({"article":article("keep","New expanded body."),"headers":headers(),"raw_source":guard}),
    )
    .unwrap();
    let doc = Document::parse(fs::read(cfg.concepts.join("keep.md")).unwrap()).unwrap();
    assert_eq!(doc.metadata["visibility"], json!("internal"));
}
