use llmbase_core::compat_taxonomy::transform;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn oracle() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/rust_migration/fixtures/taxonomy-read.json"
    ))
    .unwrap()
}

#[test]
fn frozen_pure_cases_execute_in_rust() {
    let oracle = oracle();
    let cases = oracle["pure"].as_array().unwrap();
    assert_eq!(cases.len(), 76);
    for case in cases {
        let result = transform(case["action"].as_str().unwrap(), &case["args"])
            .unwrap_or_else(|e| panic!("{} failed: {e}", case["action"].as_str().unwrap()));
        assert_eq!(result, case["expected"], "{}", case["args"]);
    }
}

fn write_library(base: &Path, files: &Value) {
    for (relative, data) in files.as_object().unwrap() {
        let path = base.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, data.as_str().unwrap()).unwrap();
    }
}

fn inventory(base: &Path) -> (BTreeMap<String, String>, Vec<PathBuf>) {
    let mut files = BTreeMap::new();
    let mut dirs = Vec::new();
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path.clone());
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/");
                files.insert(
                    relative,
                    format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
                );
            }
        }
    }
    (files, dirs)
}

#[test]
fn frozen_libraries_execute_without_writes() {
    let oracle = oracle();
    let libraries = oracle["libraries"].as_array().unwrap();
    assert_eq!(libraries.len(), 3);
    for library in libraries {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        write_library(base, &library["files"]);
        let concepts = base.join("knowledge/entries");
        let meta = base.join("knowledge/metadata");
        let results = &library["results"];
        let load = transform("load", &json!({"meta_dir": meta.to_str().unwrap()})).unwrap();
        assert_eq!(&load, &results["load"], "{} load", library["name"]);
        let articles = transform(
            "load_articles",
            &json!({"concepts_dir": concepts.to_str().unwrap()}),
        )
        .unwrap();
        assert_eq!(
            &articles, &results["articles"],
            "{} articles",
            library["name"]
        );
        let pairs = transform(
            "title_map",
            &json!({"concepts_dir": concepts.to_str().unwrap()}),
        )
        .unwrap();
        let titles: serde_json::Map<String, Value> = pairs
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
            .collect();
        assert_eq!(
            &Value::Object(titles),
            &results["titles"],
            "{} titles",
            library["name"]
        );
        for (lang, expected) in results["build"].as_object().unwrap() {
            let build = transform(
                "build",
                &json!({"concepts_dir": concepts.to_str().unwrap(),
                    "meta_dir": meta.to_str().unwrap(), "lang": lang}),
            )
            .unwrap();
            assert_eq!(&build, expected, "{} build {lang}", library["name"]);
        }
        // Reads leave the original byte inventory untouched: no new files or
        // directories, no rewrites of the cache, articles, or config.
        let (files, dirs) = inventory(base);
        let expected: BTreeMap<String, String> = library["inventory"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(files, expected, "{} inventory", library["name"]);
        for dir in dirs {
            let relative = dir
                .strip_prefix(base)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            assert!(
                expected
                    .keys()
                    .any(|f| f.starts_with(&format!("{relative}/"))),
                "{} unexpected empty directory {relative}",
                library["name"]
            );
        }
    }
}

#[test]
fn cache_means_no_fallback_and_invalid_cache_fails_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let concepts = base.join("entries");
    let meta = base.join("meta");
    fs::create_dir_all(&concepts).unwrap();
    fs::create_dir_all(&meta).unwrap();
    fs::write(
        concepts.join("a.md"),
        "---\ntitle: A / 甲\ntags: [shared]\n---\nbody\n",
    )
    .unwrap();
    fs::write(
        concepts.join("b.md"),
        "---\ntitle: B / 乙\ntags: [shared]\n---\nbody\n",
    )
    .unwrap();
    // An empty cached tree wins over the non-empty tag fallback.
    fs::write(meta.join("taxonomy.json"), "{\"categories\": []}").unwrap();
    let build = transform(
        "build",
        &json!({"concepts_dir": concepts.to_str().unwrap(),
            "meta_dir": meta.to_str().unwrap(), "lang": "en"}),
    )
    .unwrap();
    assert_eq!(build, json!([]));
    // An invalid cache fails clearly and keeps its original bytes.
    fs::write(meta.join("taxonomy.json"), "not json {").unwrap();
    for action in ["build", "load"] {
        let args = if action == "build" {
            json!({"concepts_dir": concepts.to_str().unwrap(),
                "meta_dir": meta.to_str().unwrap(), "lang": "en"})
        } else {
            json!({"meta_dir": meta.to_str().unwrap()})
        };
        assert!(transform(action, &args).is_err(), "{action} must fail");
    }
    assert_eq!(
        fs::read_to_string(meta.join("taxonomy.json")).unwrap(),
        "not json {"
    );
    // A cache beyond the bound fails before parsing.
    fs::write(meta.join("taxonomy.json"), " ".repeat(4 * 1024 * 1024 + 8)).unwrap();
    assert!(transform("load", &json!({"meta_dir": meta.to_str().unwrap()})).is_err());
}

#[test]
fn missing_directories_return_empty_values() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let missing = base.join("absent");
    assert_eq!(
        transform(
            "load_articles",
            &json!({"concepts_dir": missing.to_str().unwrap()})
        )
        .unwrap(),
        json!([])
    );
    assert_eq!(
        transform(
            "title_map",
            &json!({"concepts_dir": missing.to_str().unwrap()})
        )
        .unwrap(),
        json!([])
    );
    assert_eq!(
        transform("load", &json!({"meta_dir": missing.to_str().unwrap()})).unwrap(),
        json!({"categories": []})
    );
    assert_eq!(
        transform(
            "build",
            &json!({"concepts_dir": missing.to_str().unwrap(),
                "meta_dir": missing.to_str().unwrap(), "lang": "zh"})
        )
        .unwrap(),
        json!([])
    );
    assert!(!missing.exists());
}

#[test]
fn explicit_null_and_missing_titles_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let concepts = temp.path().join("entries");
    fs::create_dir_all(&concepts).unwrap();
    fs::write(
        concepts.join("null-title.md"),
        "---\ntitle:\ntags: []\n---\nx\n",
    )
    .unwrap();
    fs::write(concepts.join("missing-title.md"), "---\ntags: []\n---\nx\n").unwrap();
    let pairs = transform(
        "title_map",
        &json!({"concepts_dir": concepts.to_str().unwrap()}),
    )
    .unwrap();
    let titles: serde_json::Map<String, Value> = pairs
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
        .collect();
    assert_eq!(
        Value::Object(titles),
        json!({"null-title": null, "missing-title": "missing-title"})
    );
    let articles = transform(
        "load_articles",
        &json!({"concepts_dir": concepts.to_str().unwrap()}),
    )
    .unwrap();
    assert_eq!(
        articles,
        json!([
            {"slug": "missing-title", "title": "missing-title", "tags": [], "summary": ""},
            {"slug": "null-title", "title": null, "tags": [], "summary": ""},
        ])
    );
}

fn deep_tree(depth: usize) -> Value {
    let mut node = json!({"id": "leaf"});
    for _ in 1..depth {
        node = json!({"id": "n", "children": [node]});
    }
    node
}

#[test]
fn deep_and_malformed_trees_fail_clearly_without_panic() {
    let args = |node| json!({"tree": [node], "lang": "en", "titles": {}});
    assert!(transform("localize_tree", &args(deep_tree(64))).is_ok());
    let err = transform("localize_tree", &args(deep_tree(65))).unwrap_err();
    assert!(err.to_string().contains("depth"), "{err}");
    assert!(transform("localize_tree", &args(deep_tree(400))).is_err());
    assert!(transform(
        "build",
        &json!({"concepts_dir": "/nonexistent-b13", "meta_dir": "/nonexistent-b13", "lang": "en"})
    )
    .is_ok());
}

#[test]
fn invalid_arguments_and_unknown_action_fail() {
    assert!(transform("fallback", &json!({})).is_err());
    assert!(transform("fallback", &json!({"articles": {}})).is_err());
    assert!(transform("fallback", &json!({"articles": [null]})).is_err());
    assert!(transform("fallback", &json!({"articles": [{"tags": []}]})).is_err());
    assert!(transform("fallback", &json!({"articles": [{"slug": 1, "tags": []}]})).is_err());
    assert!(
        transform(
            "fallback",
            &json!({"articles": [{"slug": "a", "tags": "x"}]})
        )
        .is_err()
    );
    assert!(
        transform(
            "fallback",
            &json!({"articles": [{"slug": "a", "tags": null}]})
        )
        .is_err()
    );
    assert!(
        transform(
            "fallback",
            &json!({"articles": [{"slug": "a", "tags": [1]}]})
        )
        .is_err()
    );
    let many: Vec<Value> = (0..20_001)
        .map(|i| json!({"slug": format!("s{i}")}))
        .collect();
    assert!(transform("fallback", &json!({"articles": many})).is_err());

    let tree = json!([{"id": "n", "article_slugs": ["a"]}]);
    assert!(
        transform(
            "localize_tree",
            &json!({"tree": {}, "lang": "en", "titles": {}})
        )
        .is_err()
    );
    assert!(transform("localize_tree", &json!({"tree": tree, "lang": "en"})).is_err());
    assert!(
        transform(
            "localize_tree",
            &json!({"tree": tree, "lang": "en", "titles": []})
        )
        .is_err()
    );
    assert!(
        transform(
            "localize_tree",
            &json!({"tree": [null], "lang": "en", "titles": {}})
        )
        .is_err()
    );
    assert!(
        transform(
            "localize_tree",
            &json!({"tree": [{"label": "x"}], "lang": "en", "titles": {}})
        )
        .is_err()
    );
    assert!(
        transform(
            "localize_tree",
            &json!({"tree": [{"id": "n", "label": 1}], "lang": "en", "titles": {}})
        )
        .is_err()
    );
    assert!(
        transform(
            "localize_tree",
            &json!({"tree": [{"id": "n", "article_slugs": "a"}], "lang": "en", "titles": {}})
        )
        .is_err()
    );
    assert!(
        transform(
            "localize_tree",
            &json!({"tree": [{"id": "n", "children": {}}], "lang": "en", "titles": {}})
        )
        .is_err()
    );

    assert!(transform("localize_title", &json!({"lang": "en"})).is_err());
    assert!(transform("localize_title", &json!({"title": 5, "lang": "en"})).is_err());
    assert!(transform("localize_title", &json!({"title": "a / b"})).is_err());
    assert_eq!(
        transform("localize_title", &json!({"title": null, "lang": "en"})).unwrap(),
        json!(null)
    );
    assert_eq!(
        transform("localize_title", &json!({"title": 0, "lang": "en"})).unwrap(),
        json!(0)
    );

    assert!(transform("load", &json!({})).is_err());
    assert!(transform("load_articles", &json!({"concepts_dir": 1})).is_err());
    assert!(transform("unknown", &json!({})).is_err());
}

#[test]
fn uncached_build_rejects_malformed_file_tags_without_panic_or_writes() {
    let temp = tempfile::tempdir().unwrap();
    let concepts = temp.path().join("entries");
    let meta = temp.path().join("meta");
    fs::create_dir_all(&concepts).unwrap();
    for tags in [json!([42]), Value::Null, json!({"bad": true}), json!(7)] {
        let source = format!("---\n{}\n---\n原文\n", json!({"tags": tags}));
        let path = concepts.join("a.md");
        fs::write(&path, &source).unwrap();
        let loaded = transform("load_articles", &json!({"concepts_dir": concepts})).unwrap();
        assert_eq!(
            loaded[0]["tags"], tags,
            "read DTO preserves original metadata"
        );
        let result = std::panic::catch_unwind(|| {
            transform(
                "build",
                &json!({"concepts_dir": concepts, "meta_dir": meta, "lang": "en"}),
            )
        });
        assert!(result.is_ok(), "malformed tags must not panic: {tags}");
        let error = result
            .unwrap()
            .expect_err("malformed tags must fail clearly");
        assert!(error.to_string().contains("tags"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        assert!(!meta.exists());
    }
}

#[test]
fn aggregate_article_read_budget_applies_to_every_filesystem_action() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let concepts = temp.path().join("entries");
    let meta = temp.path().join("meta");
    fs::create_dir_all(&concepts).unwrap();
    let chunk = vec![b'x'; 1024 * 1024];
    for name in ["a.md", "b.md"] {
        let mut file = fs::File::create(concepts.join(name)).unwrap();
        file.write_all(b"---\n{\"tags\":[\"shared\"]}\n---\n")
            .unwrap();
        for _ in 0..33 {
            file.write_all(&chunk).unwrap();
        }
    }
    for action in ["load_articles", "title_map", "build"] {
        let error = transform(
            action,
            &json!({"concepts_dir": concepts, "meta_dir": meta, "lang": "en"}),
        )
        .expect_err("aggregate document reads must be bounded");
        assert!(error.to_string().contains("64 MiB"), "{error}");
    }
    assert!(!meta.exists());
}
