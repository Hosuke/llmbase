use llmbase_core::compat_maintenance::{check, load_articles, run_lint, transform};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn article(slug: &str, title: &str, content: &str) -> Value {
    json!({"slug":slug,"title":title,"summary":"","tags":[],"content":content,"metadata":{"title":title}})
}

#[test]
fn deterministic_checks_preserve_legacy_exact_messages_and_scalar_counts() {
    let articles = json!([
        article("仁", "仁", "𠮷\n"),
        article("missing", "...", "Short")
    ]);
    let args = json!({"articles":articles,"allow_cjk_slugs":false});
    assert_eq!(
        check("stubs", &args).unwrap(),
        json!([
            "CJK slug (should be pinyin): 仁",
            "Empty/garbage title: missing"
        ])
    );
    let mut args = args;
    args["allow_cjk_slugs"] = json!(true);
    assert_eq!(
        check("stubs", &args).unwrap()[0],
        "Near-empty article: 仁 (1 chars)"
    );
    assert_eq!(
        check("missing_metadata", &args)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        4
    );
    args["articles"][0]["tags"] = json!(["output bad", false, 42, "正常"]);
    assert_eq!(
        check("dirty_tags", &args).unwrap(),
        json!(["Dirty tags in 仁: ['output bad', 'False', '42']"])
    );
}

#[test]
fn legacy_alias_cascade_and_cached_backlink_key_semantics_are_preserved() {
    let args = json!({"articles":[article("a","A","[[四谛]] [[missing|label]] [[B]]")],"existing_slugs":["a","b","si-di"],"aliases":[["四諦","si-di"]],"backlinks":{"a":[]}});
    assert_eq!(
        check("broken_links", &args).unwrap(),
        json!(["Broken link in a: [[missing]]"])
    );
    assert_eq!(check("orphans", &args).unwrap(), json!([]));
}

#[test]
fn native_sequence_reads_snapshot_once_and_honors_live_custom_checks_and_emit() {
    let root = TempDir::new().unwrap();
    let concepts = root.path().join("concepts");
    fs::create_dir(&concepts).unwrap();
    let bytes = "---\ntitle: Accepted\nsummary: Kept\ntags: [alpha]\nunknown: {number: 42}\n---\nSubstantive accepted body, retained without any source modifications, including original 𠮷.";
    fs::write(concepts.join("a.md"), bytes).unwrap();
    let mut args = transform(
        "prepare",
        &json!({"concepts_dir":concepts,"meta_dir":root.path(),"allow_cjk_slugs":true}),
    )
    .unwrap();
    args["categories"] = json!([]);
    let mut calls = Vec::new();
    let value = run_lint(args, |stage, payload| {
        calls.push(stage.to_string());
        if stage == "stubs" {
            Ok(json!({"override":true,"value":["Custom live check"]}))
        } else if stage == "emit" {
            assert_eq!(payload["event"], "after_lint_check");
            Ok(Value::Null)
        } else {
            Ok(Value::Null)
        }
    })
    .unwrap();
    assert_eq!(value["stubs"], json!(["Custom live check"]));
    assert_eq!(
        calls,
        json!([
            "structural",
            "broken_links",
            "orphans",
            "missing_metadata",
            "dirty_tags",
            "duplicates",
            "stubs",
            "uncategorized",
            "emit"
        ])
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>()
    );
    assert_eq!(fs::read_to_string(concepts.join("a.md")).unwrap(), bytes);
    assert_eq!(value["total_issues"], 4);
}

#[test]
fn duplicate_candidates_use_cjk_conversion_and_preserve_unrelated_single_character_topics() {
    let args = json!({"articles":[{"slug":"a","title":"四諦","tags":[]},{"slug":"b","title":"四谛","tags":[]},{"slug":"c","title":"仁","tags":[]},{"slug":"d","title":"仁愛","tags":[]}]});
    assert_eq!(transform("duplicates", &args).unwrap(), json!([["a", "b"]]));
}

#[test]
fn public_only_load_fails_closed_on_malformed_private_and_non_public_visibility() {
    let root = TempDir::new().unwrap();
    let concepts = root.path().join("concepts");
    fs::create_dir(&concepts).unwrap();
    let fixtures = [
        (
            "01-default",
            "",
            "Default record without any visibility keys.",
        ),
        (
            "02-public-explicit",
            "visibility: public\n",
            "Public body keeps non-BMP bytes 𠮷 unchanged.",
        ),
        ("03-bool-private", "private: true\n", "boolean private"),
        (
            "04-quoted-private",
            "private: \"true\"\n",
            "quoted private flag",
        ),
        (
            "05-numeric-private",
            "private: 1\n",
            "malformed private flag",
        ),
        ("06-visibility-private", "visibility: private\n", "private"),
        (
            "07-visibility-internal",
            "visibility: internal\n",
            "internal",
        ),
        (
            "08-visibility-unknown",
            "visibility: classified\n",
            "unknown",
        ),
    ];
    let mut before_bytes = Vec::new();
    for (slug, front, body) in fixtures {
        let bytes = format!("---\ntitle: {slug}\n{front}---\n{body}\n");
        fs::write(concepts.join(format!("{slug}.md")), &bytes).unwrap();
        before_bytes.push((slug, bytes));
    }
    let articles = load_articles(&concepts, true).unwrap();
    let slugs: Vec<&str> = articles
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["01-default", "02-public-explicit"]);
    assert_eq!(
        articles[1]["content"].as_str().unwrap(),
        "Public body keeps non-BMP bytes 𠮷 unchanged."
    );
    // Explicit privileged (non public-only) loads retain every record.
    let all = transform(
        "load",
        &json!({"concepts_dir":concepts,"public_only":false}),
    )
    .unwrap();
    assert_eq!(all.as_array().unwrap().len(), fixtures.len());
    // Loading is read-only: each source file keeps its exact original bytes.
    for (slug, bytes) in before_bytes {
        assert_eq!(
            fs::read_to_string(concepts.join(format!("{slug}.md"))).unwrap(),
            bytes
        );
    }
}
