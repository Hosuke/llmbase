use llmbase_core::{
    Error, Library, LibraryConfig,
    compat_query::{gather, run, transform},
    document::Document,
};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, LibraryConfig) {
    let dir = TempDir::new().unwrap();
    let cfg = LibraryConfig::load(dir.path(), "query").unwrap();
    for p in [&cfg.concepts, &cfg.outputs, &cfg.meta] {
        fs::create_dir_all(p).unwrap();
    }
    (dir, cfg)
}
fn article(cfg: &LibraryConfig, slug: &str, title: &str, extra: &str) {
    fs::write(
        cfg.concepts.join(format!("{slug}.md")),
        format!(
            "---\ntitle: {title}\nsummary: careful source\n{extra}---\n\nknowledge evidence 原典𠮷"
        ),
    )
    .unwrap();
}
fn index(cfg: &LibraryConfig) {
    fs::write(cfg.meta.join("index.json"),json!([{"slug":"a","title":"Alpha","summary":"source"},{"slug":"b","title":"Beta","summary":"source"},{"slug":"c","title":"Gamma","summary":"source"}]).to_string()).unwrap();
}
fn defaults(stage: &str, _: &Value) -> llmbase_core::Result<Value> {
    match stage {
        "selector" => Ok(json!("Alpha\nBeta\nGamma")),
        "answer" => Ok(json!("actual answer")),
        _ => Ok(Value::Null),
    }
}

#[test]
fn shallow_sequence_and_journal_fileback_preserve_originals() {
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "Alpha", "");
    let before = fs::read(cfg.concepts.join("a.md")).unwrap();
    let mut stages = Vec::new();
    let result = run(
        &cfg,
        &json!({"question":"knowledge evidence","file_back":true,"return_path":true}),
        |stage, payload| {
            stages.push(stage.to_owned());
            if stage == "answer" {
                assert!(
                    payload["context"][0]["content"]
                        .as_str()
                        .unwrap()
                        .contains("原典𠮷")
                );
            }
            defaults(stage, payload)
        },
    )
    .unwrap();
    assert_eq!(stages, vec!["gather", "answer", "file_output"]);
    assert_eq!(result["value"]["answer"], "actual answer");
    let output = cfg
        .base
        .join(result["value"]["output_path"].as_str().unwrap());
    let doc = Document::parse(fs::read(output).unwrap()).unwrap();
    assert_eq!(doc.metadata["type"], "query_markdown");
    assert_eq!(
        doc.metadata["query_provenance"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(fs::read(cfg.concepts.join("a.md")).unwrap(), before);
    let lib = Library::open(cfg.clone()).unwrap();
    assert_eq!(lib.list(Some("note"), false).unwrap().len(), 1);
}

#[test]
fn deep_actual_sequence_and_consulted_titles() {
    let (_dir, cfg) = fixture();
    for (slug, title) in [("a", "Alpha"), ("b", "Beta"), ("c", "Gamma")] {
        article(&cfg, slug, title, "")
    }
    index(&cfg);
    let mut stages = Vec::new();
    let result = run(
        &cfg,
        &json!({"mode":"deep","question":"knowledge","return_context":true}),
        |stage, payload| {
            stages.push(stage.to_owned());
            defaults(stage, payload)
        },
    )
    .unwrap();
    assert_eq!(stages, vec!["index", "selector", "answer"]);
    assert_eq!(result["value"]["consulted"].as_array().unwrap().len(), 3);
    assert!(!cfg.database().exists());
    assert_eq!(result["trace"]["orchestrator"], "rust");
}

#[test]
fn private_and_stale_static_index_never_enter_default_context() {
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "Alpha", "");
    article(&cfg, "b", "Secret", "private: true\n");
    article(&cfg, "c", "Internal", "visibility: internal\n");
    article(&cfg, "d", "Unrecognized", "visibility: secret\n");
    index(&cfg);
    fs::write(cfg.meta.join("_index.md"), "SECRET old private details").unwrap();
    let context = gather(&cfg, "knowledge evidence").unwrap();
    let serialized = context.to_string();
    assert!(!serialized.contains("SECRET"));
    assert!(!serialized.contains("Secret"));
    assert!(!serialized.contains("Internal"));
    assert!(!serialized.contains("Unrecognized"));
    let rows = run(&cfg, &json!({"mode":"load_index"}), defaults).unwrap();
    assert_eq!(rows["value"].as_array().unwrap().len(), 1);
}

#[test]
fn source_edit_during_generation_prevents_filing() {
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "Alpha", "");
    let result = run(
        &cfg,
        &json!({"question":"knowledge","file_back":true}),
        |stage, payload| {
            if stage == "answer" {
                article(&cfg, "a", "Alpha", "private: true\n");
            }
            defaults(stage, payload)
        },
    );
    assert!(matches!(result, Err(Error::Conflict(_))));
    assert_eq!(fs::read_dir(&cfg.outputs).unwrap().count(), 0);
}

#[test]
fn accepted_promotion_uses_shared_merge_and_real_metadata_rebuild() {
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "Alpha", "");
    let decision = json!({"promote":true,"slug":"different","merge_into":"a","title":"Alpha","summary":"summary","tags":["qa"],"content":"A sufficiently longer body with new source support and detailed original knowledge."});
    let result = run(
        &cfg,
        &json!({"mode":"promote","question":"What?","answer":"answer","consulted":[],"index":[]}),
        |stage, _| {
            assert_eq!(stage, "judge");
            Ok(json!({"decision":decision,"headers":[["English",""]],"created":"2026-09-20"}))
        },
    )
    .unwrap();
    assert_eq!(result["value"]["promoted"], true);
    assert_eq!(result["value"]["slug"], "a");
    assert_eq!(result["value"]["merged"], true);
    let doc = Document::parse(fs::read(cfg.concepts.join("a.md")).unwrap()).unwrap();
    assert_eq!(doc.metadata["sources"][0]["plugin"], "qa");
    assert!(cfg.meta.join("index.json").exists());
    assert!(!cfg.concepts.join("different.md").exists());
}

#[test]
fn private_target_and_mid_judge_private_change_fail_closed() {
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "Alpha", "");
    let decision = json!({"promote":true,"slug":"new","title":"New","content":"new body"});
    let result = run(
        &cfg,
        &json!({"mode":"promote","question":"What?","answer":"answer","consulted":[],"index":[]}),
        |_, _| {
            article(&cfg, "a", "Alpha", "private: true\n");
            Ok(json!({"decision":decision,"headers":[["English",""]]}))
        },
    );
    assert!(matches!(result, Err(Error::Conflict(_))));
    assert!(!cfg.concepts.join("new.md").exists());
    let private = run(
        &cfg,
        &json!({"mode":"promote","question":"What?","answer":"answer","consulted":[],"index":[]}),
        |_, _| {
            Ok(
                json!({"decision":{"promote":true,"slug":"a","title":"Alpha","content":"leak"},"headers":[["English",""]]}),
            )
        },
    );
    assert!(matches!(private, Err(Error::Scope)));
}

#[test]
fn malformed_truncated_and_declined_judges_never_write() {
    let (_dir, cfg) = fixture();
    for judged in [
        json!({"decision":[]}),
        json!({"decision":{"promote":true,"slug":"new","title":"title","content":"body"},"truncated":true}),
        json!({"error":"judge returned invalid JSON"}),
        json!({"decision":{"promote":false,"reason":"decline"}}),
    ] {
        let out = run(
            &cfg,
            &json!({"mode":"promote","question":"q","answer":"a","consulted":[],"index":[]}),
            |_, _| Ok(judged.clone()),
        )
        .unwrap();
        assert_eq!(out["value"]["promoted"], false);
    }
    assert_eq!(fs::read_dir(&cfg.concepts).unwrap().count(), 0);
}

#[test]
fn prefilter_native_unicode_and_live_tokens_keep_ranking_and_fallback() {
    let index = json!([{"slug":"a","title":"unrelated"},{"slug":"b","title":"空性 原典"}]);
    let actual = transform(
        "prefilter",
        &json!({"question":"空性","index":index,"top_k":1}),
    )
    .unwrap();
    assert_eq!(actual[0]["slug"], "b");
    let tokens=transform("prefilter",&json!({"question":"custom","index":index,"top_k":1,"tokenized":{"query":["x"],"documents":[["x"],[]]}})).unwrap();
    assert_eq!(tokens[0]["slug"], "a");
    assert_eq!(
        transform(
            "prefilter",
            &json!({"question":"?","index":index,"top_k":0})
        )
        .unwrap(),
        json!([])
    );
}

#[test]
fn callback_failure_is_error_and_no_default_save_side_effect() {
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "Alpha", "");
    let result = run(&cfg, &json!({"question":"knowledge"}), |stage, _| {
        if stage == "answer" {
            Err(Error::Capability("provider unavailable".into()))
        } else {
            Ok(Value::Null)
        }
    });
    assert!(matches!(result, Err(Error::Capability(_))));
    assert!(!cfg.database().exists());
    assert_eq!(fs::read_dir(&cfg.outputs).unwrap().count(), 0);
}

#[test]
fn corrupt_legacy_surrogates_are_repaired_and_static_summary_is_current() {
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "原典𠮷", "");
    fs::write(cfg.meta.join("index.json"),r#"[{"slug":"a","title":"old\ud800","summary":"formerly private","other":"\\ud800","pair":"\ud842\udfb7"}]"#).unwrap();
    let rows = run(&cfg, &json!({"mode":"load_index"}), defaults).unwrap();
    assert_eq!(rows["value"][0]["title"], "原典𠮷");
    assert_eq!(rows["value"][0]["summary"], "careful source");
    assert_eq!(rows["value"][0]["other"], r"\ud800");
    assert_eq!(rows["value"][0]["pair"], "𠮷");
}

#[test]
fn malformed_private_and_visibility_flags_fail_closed_across_query_paths() {
    let (_dir, cfg) = fixture();
    let adversarial = [
        "private: yes\n",
        "private: on\n",
        "private: \"false\"\n",
        "private: \"yes\"\n",
        "private: \"\"\n",
        "private: 0\n",
        "private: []\n",
        "private: {}\n",
        "visibility: 0\n",
        "visibility: PUBLIC\n",
        "visibility: internal\n",
    ];
    let mut slugs = Vec::new();
    for (i, flags) in adversarial.iter().enumerate() {
        let slug = format!("adv{i}");
        fs::write(
            cfg.concepts.join(format!("{slug}.md")),
            format!("---\ntitle: LEAKTITLE{i}\nsummary: leak summary {i}\n{flags}---\n\nknowledge evidence SECRETTOKEN{i}"),
        )
        .unwrap();
        slugs.push(slug);
    }
    for (j, flags) in ["private: false\n", "private:\n", ""].iter().enumerate() {
        let slug = format!("ctl{j}");
        fs::write(
            cfg.concepts.join(format!("{slug}.md")),
            format!("---\ntitle: Control {j}\nsummary: control summary {j}\n{flags}---\n\nknowledge evidence CONTROLTOKEN{j}"),
        )
        .unwrap();
        slugs.push(slug);
    }
    // A stale static index claims every slug is public and retains leaked text.
    let stale: Vec<Value> = slugs
        .iter()
        .enumerate()
        .map(|(i, slug)| {
            json!({"slug":slug,"title":format!("STALETITLE{i}"),"summary":format!("STALESECRET{i}")})
        })
        .collect();
    fs::write(cfg.meta.join("index.json"), Value::Array(stale).to_string()).unwrap();

    // gather: only controls enter the default context payload.
    let serialized = gather(&cfg, "knowledge evidence").unwrap().to_string();
    assert!(!serialized.contains("SECRETTOKEN"));
    assert!(!serialized.contains("LEAKTITLE"));
    for j in 0..3 {
        assert!(serialized.contains(&format!("CONTROLTOKEN{j}")));
    }

    // load_index: stale public rows cannot override live non-public metadata.
    let rows = run(&cfg, &json!({"mode":"load_index"}), defaults).unwrap();
    let rows = rows["value"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|row| row["slug"].as_str().unwrap().starts_with("ctl"))
    );
    assert!(!Value::Array(rows.clone()).to_string().contains("STALE"));

    // deep run: selector/answer callback payloads carry no non-public text,
    // even when the host selector explicitly names an adversarial title.
    let mut payloads = Vec::new();
    let result = run(
        &cfg,
        &json!({"mode":"deep","question":"knowledge evidence","return_context":true}),
        |stage, payload| {
            payloads.push(payload.clone());
            match stage {
                "selector" => Ok(json!("Control 0\nControl 1\nControl 2\nLEAKTITLE0")),
                "answer" => Ok(json!("actual answer")),
                _ => Ok(Value::Null),
            }
        },
    )
    .unwrap();
    assert_eq!(result["value"]["answer"], "actual answer");
    let payloads = json!(payloads).to_string();
    assert!(!payloads.contains("SECRETTOKEN"));
    assert!(!payloads.contains("LEAKTITLE"));
    assert!(!payloads.contains("STALE"));
    for j in 0..3 {
        assert!(payloads.contains(&format!("CONTROLTOKEN{j}")));
    }
}

#[test]
fn public_only_promotion_rejects_non_public_target_before_mutation() {
    for flags in [
        "private: \"false\"\n",
        "private: 0\n",
        "private: []\n",
        "visibility: 0\n",
        "visibility: internal\n",
    ] {
        let (_dir, cfg) = fixture();
        article(&cfg, "a", "Alpha", flags);
        let before = fs::read(cfg.concepts.join("a.md")).unwrap();
        let result = run(
            &cfg,
            &json!({"mode":"promote","question":"What?","answer":"answer","consulted":[],"index":[]}),
            |_, _| {
                Ok(
                    json!({"decision":{"promote":true,"slug":"a","title":"Alpha","content":"leak"},"headers":[["English",""]]}),
                )
            },
        );
        assert!(matches!(result, Err(Error::Scope)), "{flags}");
        assert_eq!(
            fs::read(cfg.concepts.join("a.md")).unwrap(),
            before,
            "{flags}"
        );
    }
    // Control: an explicitly public target still accepts promotion merges.
    let (_dir, cfg) = fixture();
    article(&cfg, "a", "Alpha", "private: false\n");
    let result = run(
        &cfg,
        &json!({"mode":"promote","question":"What?","answer":"answer","consulted":[],"index":[]}),
        |_, _| {
            Ok(
                json!({"decision":{"promote":true,"slug":"a","title":"Alpha","content":"leak"},"headers":[["English",""]]}),
            )
        },
    )
    .unwrap();
    assert_eq!(result["value"]["promoted"], true);
}
