use llmbase_core::{
    Error, Evidence, Library, LibraryConfig,
    catalog::{GraphOptions, RelationInput},
};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("raw")).unwrap();
    fs::create_dir_all(dir.path().join("wiki/concepts")).unwrap();
    fs::write(dir.path().join("raw/one.md"),"---\ntitle: First source\nsource: https://example.invalid/one\n---\nFirst witness supports learning.\n").unwrap();
    fs::write(dir.path().join("raw/two.md"),"---\ntitle: Second source\nsource: https://example.invalid/two\n---\nSecond witness independently supports learning.\n").unwrap();
    fs::write(dir.path().join("wiki/concepts/a.md"),"---\ntitle: Learning / 學習\naliases: [學問]\nmerged_from: [old-a]\ntags: [practice]\nsources:\n  - url: https://example.invalid/one\n  - url: https://example.invalid/two\n---\n[[修養|self cultivation]] and [[concealed]].\n").unwrap();
    fs::write(dir.path().join("wiki/concepts/b.md"),"---\ntitle: Cultivation / 修養\ntags: [practice]\nsources:\n  - url: https://example.invalid/one\n  - url: https://example.invalid/two\n---\n[[學問]] gives reciprocal support.\n").unwrap();
    fs::write(
        dir.path().join("wiki/concepts/concealed.md"),
        "---\ntitle: SECRET TITLE\nprivate: true\n---\nSECRET BODY [[a]].\n",
    )
    .unwrap();
    let mut library =
        Library::open(LibraryConfig::load(dir.path(), "catalog-test").unwrap()).unwrap();
    library.sync().unwrap();
    (dir, library)
}
fn evidence(library: &Library, query: &str) -> Evidence {
    library
        .search(query, 10, Some("raw"), true)
        .unwrap()
        .results[0]
        .evidence
        .clone()
}
fn relation(kind: &str, supports: Vec<Evidence>) -> RelationInput {
    RelationInput {
        source: "a".into(),
        target: "b".into(),
        kind: kind.into(),
        supports,
        metadata: json!({"accepted_by":"reader","unknown":{"keep":true}}),
        visibility: "public".into(),
    }
}
fn edge<'a>(graph: &'a Value, kind: &str) -> &'a Value {
    graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|edge| edge["kind"] == kind)
        .unwrap()
}

#[test]
fn aliases_backlinks_and_graph_are_live_scoped_and_bounded() {
    let (dir, mut library) = fixture();
    let aliases = library.aliases(false).unwrap();
    assert_eq!(aliases["aliases"]["学问"], "a");
    assert_eq!(aliases["aliases"]["old-a"], "a");
    assert!(!aliases.to_string().contains("SECRET"));
    let incoming = library.backlinks("學問", false).unwrap();
    assert_eq!(incoming["backlinks"].as_array().unwrap().len(), 1);
    assert_eq!(incoming["backlinks"][0]["slug"], "b");
    assert!(library.backlinks("concealed", false).is_err());
    let graph = library.graph(GraphOptions::default(), false).unwrap();
    assert!(!graph.to_string().contains("SECRET"));
    let hidden_id = library.get("concealed", None, true).unwrap().source_id;
    assert!(
        !graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == hidden_id)
    );
    assert!(
        !graph["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["source"] == hidden_id || edge["target"] == hidden_id)
    );
    let shared = edge(&graph, "shared_source");
    assert_eq!(shared["supports"].as_array().unwrap().len(), 2);
    for item in shared["supports"].as_array().unwrap() {
        let evidence: Evidence = serde_json::from_value(item["evidence"].clone()).unwrap();
        assert!(!library.resolve_evidence(&evidence, false).unwrap().stale);
    }
    let root = library
        .graph(
            GraphOptions {
                root: Some("a".into()),
                depth: 0,
                ..Default::default()
            },
            false,
        )
        .unwrap();
    assert_eq!(root["nodes"].as_array().unwrap().len(), 1);
    assert!(root["edges"].as_array().unwrap().is_empty());
    let bounded = library
        .graph(
            GraphOptions {
                root: Some("a".into()),
                depth: 1,
                max_neighbors: 1,
                ..Default::default()
            },
            false,
        )
        .unwrap();
    assert!(bounded["nodes"].as_array().unwrap().len() <= 2);
    assert_eq!(bounded["truncated"], true);
    assert!(
        library
            .graph(
                GraphOptions {
                    max_nodes: 0,
                    ..Default::default()
                },
                false
            )
            .is_err()
    );
    fs::write(
        dir.path().join("wiki/concepts/duplicate.md"),
        "---\ntitle: Another\naliases: [學問]\n---\nA duplicate alias.\n",
    )
    .unwrap();
    library.sync().unwrap();
    assert!(library.aliases(false).unwrap()["ambiguous"]["学问"].is_array());
    assert!(matches!(
        library.backlinks("學問", false),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn real_evidence_supports_survive_one_deletion_preserve_staleness_and_distinct_kinds() {
    let (dir, mut library) = fixture();
    let first = evidence(&library, "First witness");
    let second = evidence(&library, "Second independently");
    assert_ne!(first.source_id, second.source_id);
    let created = library
        .add_relation(relation("supports", vec![first.clone()]), true)
        .unwrap();
    let updated = library
        .add_relation(relation("supports", vec![second.clone()]), true)
        .unwrap();
    assert_eq!(created["id"], updated["id"]);
    assert_eq!(updated["revision"], 2);
    library
        .add_relation(relation("contradicts", vec![second.clone()]), true)
        .unwrap();
    fs::remove_file(dir.path().join("raw/one.md")).unwrap();
    library.sync().unwrap();
    let graph = library.graph(GraphOptions::default(), false).unwrap();
    let support = edge(&graph, "supports");
    assert_eq!(support["supports"].as_array().unwrap().len(), 2);
    assert_eq!(
        support["supports"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["deleted"] == true)
            .count(),
        1
    );
    assert!(edge(&graph, "contradicts").is_object());
    // The shared-source projection also retains the other live raw witness.
    assert_eq!(
        edge(&graph, "shared_source")["supports"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let path = dir.path().join("raw/two.md");
    fs::write(&path,"---\ntitle: Second source\nsource: https://example.invalid/two\n---\nRevised witness now has a different claim.\n").unwrap();
    library.sync().unwrap();
    let graph = library.graph(GraphOptions::default(), false).unwrap();
    let supports = edge(&graph, "supports")["supports"].as_array().unwrap();
    let retained = supports
        .iter()
        .find(|s| s["evidence"]["source_id"] == second.source_id)
        .unwrap();
    assert_eq!(retained["stale"], true);
    assert_eq!(retained["deleted"], false);
    assert_eq!(retained["evidence"]["revision"], second.revision);
    assert_eq!(retained["evidence"]["exact_quote"], second.exact_quote);
    let id = created["id"].as_str().unwrap().to_owned();
    drop(library);
    let mut library =
        Library::open(LibraryConfig::load(dir.path(), "catalog-test").unwrap()).unwrap();
    assert!(
        edge(
            &library.graph(GraphOptions::default(), false).unwrap(),
            "supports"
        )
        .is_object()
    );
    library.remove_relation(&id, true).unwrap();
    let graph = library.graph(GraphOptions::default(), true).unwrap();
    assert!(
        !graph["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "supports")
    );
    assert!(edge(&graph, "contradicts").is_object());
    let db = rusqlite::Connection::open(library.config.database()).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM catalog_relation_revisions WHERE id=?1",
            [&id],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        3
    );
}

#[test]
fn relation_evidence_cannot_be_forged_cross_library_or_exposed_through_public_projection() {
    let (_dir, mut library) = fixture();
    let first = evidence(&library, "First witness");
    let mut foreign = first.clone();
    foreign.library_id = "other".into();
    assert!(matches!(
        library.add_relation(relation("claims", vec![foreign]), true),
        Err(Error::Scope)
    ));
    let mut forged = first.clone();
    forged.exact_quote.push_str(" invented");
    assert!(matches!(
        library.add_relation(relation("claims", vec![forged]), true),
        Err(Error::Conflict(_))
    ));
    let private = library
        .search("SECRET BODY", 10, Some("compiled"), true)
        .unwrap()
        .results[0]
        .evidence
        .clone();
    assert!(
        library
            .add_relation(relation("private-proof", vec![private.clone()]), true)
            .is_err()
    );
    let mut input = relation("private-proof", vec![private]);
    input.visibility = "private".into();
    library.add_relation(input, true).unwrap();
    assert!(
        !library
            .graph(GraphOptions::default(), false)
            .unwrap()
            .to_string()
            .contains("private-proof")
    );
    assert!(
        edge(
            &library.graph(GraphOptions::default(), true).unwrap(),
            "private-proof"
        )
        .is_object()
    );
    let manual = library
        .add_relation(relation("accepted-without-source", Vec::new()), false)
        .unwrap();
    assert_eq!(manual["relation"]["unconditional"], true);
}

#[test]
fn taxonomy_preserves_accepted_json_and_projects_children_first_without_private_records() {
    let (dir, mut library) = fixture();
    let original = json!({"generated":true,"future":{"untouched":[1,null,"文言"]},"categories":[
        {"id":"study","label":{"en":"Study","zh":"學問","wenyan":"學"},"unknown":{"nested":true},"article_slugs":["a"],"children":[
            {"id":"practice","label":"Practice","article_slugs":["a","b"],"children":[]}]},
        {"id":"private","label":"SECRET CATEGORY","article_slugs":["concealed"],"children":[]}
    ]});
    let cache = dir.path().join("wiki/_meta/taxonomy.json");
    let bytes = serde_json::to_vec(&original).unwrap();
    fs::write(&cache, &bytes).unwrap();
    let projected = library.taxonomy("zh", false).unwrap();
    assert_eq!(projected["origin"], "legacy_cache");
    assert_eq!(projected["categories"].as_array().unwrap().len(), 1);
    assert_eq!(projected["categories"][0]["count"], 0);
    assert_eq!(projected["categories"][0]["total"], 2);
    assert_eq!(
        projected["categories"][0]["children"][0]["articles"][0]["title"],
        "學習"
    );
    assert!(!projected.to_string().contains("SECRET"));
    let public_replacement =
        json!({"categories":[{"id":"all","label":"All","article_slugs":["a","b"],"children":[]}]});
    assert!(matches!(
        library.set_taxonomy(public_replacement, false),
        Err(Error::Scope)
    ));
    assert_eq!(
        library.set_taxonomy(original.clone(), true).unwrap()["revision"],
        1
    );
    assert_eq!(
        fs::read(&cache).unwrap(),
        bytes,
        "legacy accepted JSON must not be overwritten as a projection"
    );
    drop(library);
    let library = Library::open(LibraryConfig::load(dir.path(), "catalog-test").unwrap()).unwrap();
    let projected = library.taxonomy("wenyan", true).unwrap();
    assert_eq!(projected["origin"], "accepted");
    assert_eq!(projected["categories"][0]["label"], "學");
    assert_eq!(projected["locked"], true);
    let db = rusqlite::Connection::open(library.config.database()).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM catalog_documents WHERE kind='taxonomy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let stored: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(stored["future"], original["future"]);
    assert_eq!(
        stored["categories"][0]["unknown"],
        original["categories"][0]["unknown"]
    );
}

#[test]
fn taxonomy_fallback_and_entity_cache_reads_are_real_and_corruption_is_not_empty_success() {
    let (dir, library) = fixture();
    let fallback = library.taxonomy("en", false).unwrap();
    assert_eq!(fallback["origin"], "tag_fallback");
    assert_eq!(fallback["categories"][0]["id"], "practice");
    assert_eq!(fallback["categories"][0]["total"], 2);
    assert_eq!(library.entities(false).unwrap()["origin"], "not_generated");
    let path = dir.path().join("wiki/_meta/entities.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({"people":[
        {"name":"Public author","articles":["a"],"dates":"1800","custom":"retained"},
        {"name":"SECRET AGGREGATE","articles":["a","concealed"]},
        {"name":"Unknown origin","articles":[]}
    ],"events":[],"places":[],"article_count":999,"unknown":"PRIVATE TOP LEVEL"}))
        .unwrap(),
    )
    .unwrap();
    let public = library.entities(false).unwrap();
    assert_eq!(public["people"].as_array().unwrap().len(), 1);
    assert_eq!(public["people"][0]["custom"], "retained");
    assert_eq!(public["article_count"], 2);
    assert!(!public.to_string().contains("SECRET"));
    assert!(!public.to_string().contains("PRIVATE TOP LEVEL"));
    assert_eq!(
        library.entities(true).unwrap()["people"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    fs::write(&path, "{broken").unwrap();
    assert!(matches!(library.entities(false), Err(Error::Json(_))));
    fs::write(dir.path().join("wiki/_meta/taxonomy.json"), "{broken").unwrap();
    assert!(matches!(library.taxonomy("en", false), Err(Error::Json(_))));
}
