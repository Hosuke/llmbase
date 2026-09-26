//! Focused integration coverage for the native frontmatter parser, exercised
//! through `Document::parse` / `parse_metadata` with synthetic fixtures only.
//! These contracts cover YAML 1.1 safe-scalar resolution, JSON metadata, byte
//! offsets and bounded alias handling; they are NOT a full YAML conformance
//! or privacy-equivalence acceptance suite.
use llmbase_core::document::{Document, parse_metadata};

fn doc(text: &str) -> Document {
    Document::parse(text.as_bytes().to_vec()).expect("synthetic document must parse")
}

#[test]
fn yaml11_plain_booleans_but_quoted_scalars_stay_strings() {
    let text = "---\n\
                plain_yes: yes\n\
                plain_on: on\n\
                plain_no: no\n\
                plain_off: off\n\
                quoted_yes: \"yes\"\n\
                single_on: 'on'\n\
                ---\n\
                Body.\n";
    let doc = doc(text);
    assert_eq!(doc.metadata["plain_yes"], true);
    assert_eq!(doc.metadata["plain_on"], true);
    assert_eq!(doc.metadata["plain_no"], false);
    assert_eq!(doc.metadata["plain_off"], false);
    assert_eq!(doc.metadata["quoted_yes"], "yes");
    assert_eq!(doc.metadata["single_on"], "on");

    let value = parse_metadata("enabled: ON\nname: \"OFF\"\n").expect("valid YAML 1.1 metadata");
    assert_eq!(value["enabled"], true);
    assert_eq!(value["name"], "OFF");
}

#[test]
fn json_flow_metadata_is_accepted() {
    let text = "---\n{\"title\": \"JSON Doc\", \"count\": 42, \"flag\": false}\n---\nBody.\n";
    let doc = doc(text);
    assert_eq!(doc.title("fallback"), "JSON Doc");
    assert_eq!(doc.metadata["count"], 42);
    assert_eq!(doc.metadata["flag"], false);
}

#[test]
fn utf8_original_and_body_offsets_are_byte_exact() {
    let frontmatter = "title: café 中文\nsummary: naïve — smoke\n";
    let body = "Body 中文 é.\nSecond line 🚀.\n";
    let text = format!("---\n{frontmatter}---\n{body}");
    let doc = doc(&text);
    assert_eq!(doc.original, text, "original must be preserved verbatim");
    let expected = "---\n".len() + frontmatter.len() + "---\n".len();
    assert_eq!(doc.body_start, expected);
    assert_eq!(&doc.original[doc.body_start..], body);
    assert_eq!(doc.metadata["title"], "café 中文");
    assert_eq!(doc.hash, llmbase_core::document::hash(text.as_bytes()));
}

#[test]
fn document_without_frontmatter_starts_body_at_zero() {
    let doc = doc("# Heading 中文\nPlain body, no frontmatter.\n");
    assert_eq!(doc.body_start, 0);
    assert!(doc.metadata.is_object());
    assert_eq!(doc.metadata.as_object().unwrap().len(), 0);
}

#[test]
fn malformed_yaml_is_rejected() {
    let bad_flow = "---\ntitle: [unclosed\n---\nbody\n";
    assert!(Document::parse(bad_flow.as_bytes().to_vec()).is_err());
    assert!(parse_metadata("a: [1, 2").is_err());

    let unterminated = "---\ntitle: ok\nbody without a closing fence\n";
    assert!(Document::parse(unterminated.as_bytes().to_vec()).is_err());

    let non_mapping = "---\n- 1\n- 2\n---\nbody\n";
    assert!(Document::parse(non_mapping.as_bytes().to_vec()).is_err());
}

#[test]
fn cyclic_and_unresolved_aliases_are_rejected() {
    let cyclic = "---\na: &x [*x]\n---\nbody\n";
    assert!(Document::parse(cyclic.as_bytes().to_vec()).is_err());
    assert!(parse_metadata("a: *missing").is_err());
}

#[test]
fn alias_expansion_bomb_is_bounded() {
    let bomb = "a: &a [\"0123456789\"]\n\
                b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a]\n\
                c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b]\n\
                d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c]\n\
                e: &e [*d, *d, *d, *d, *d, *d, *d, *d, *d]\n\
                f: [*e, *e, *e, *e, *e, *e, *e, *e, *e]\n";
    assert!(parse_metadata(bomb).is_err());
}

#[test]
fn plain_null_mapping_key_is_the_json_null_string_key() {
    // serde_yaml compatibility boundary exercised by the migration contract:
    // a plain null mapping key becomes the JSON object key "null" while the
    // typed Null key stays available via parse_typed for Python adapters.
    let text = "---\ntitle: 典\nunknown: {rare: 𠀀, null: null}\n---\n學而時習之。\n";
    let doc = doc(text);
    assert_eq!(doc.original, text);
    assert_eq!(&doc.original[doc.body_start..], "學而時習之。\n");
    let unknown = doc.metadata["unknown"].as_object().unwrap();
    assert_eq!(unknown["rare"], "𠀀");
    assert!(unknown.contains_key("null"), "null key must not be dropped");
    assert_eq!(unknown["null"], serde_json::Value::Null);

    let typed = llmbase_core::compat_yaml::parse_typed("null: null\n").unwrap();
    let llmbase_core::compat_yaml::Node::Mapping(pairs) = typed else {
        panic!("typed parse must keep the mapping");
    };
    assert!(matches!(pairs[0].0, llmbase_core::compat_yaml::Node::Null));

    // A quoted "null" key colliding with a plain null key is rejected in
    // either order rather than silently losing a metadata entry.
    assert!(parse_metadata("null: 1\n\"null\": 2\n").is_err());
    assert!(parse_metadata("\"null\": 1\nnull: 2\n").is_err());

    // No broad stringification: other non-string keys still need an adapter.
    assert!(parse_metadata("true: x\n").is_err());
}
