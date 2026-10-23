//! Configured visibility capacity is distinct from a document's privacy.
use llmbase_core::{Error, LibraryConfig, Result, compat_kb};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

fn fixture() -> (TempDir, LibraryConfig) {
    let temp = TempDir::new().unwrap();
    for directory in ["wiki/concepts", "raw", "wiki/outputs"] {
        fs::create_dir_all(temp.path().join(directory)).unwrap();
    }
    let config = LibraryConfig::load(temp.path(), "visibility-limits").unwrap();
    (temp, config)
}

fn reload(temp: &TempDir, limits: Value) -> LibraryConfig {
    fs::write(
        temp.path().join("config.yaml"),
        serde_json::to_vec(&json!({"visibility_snapshot": limits})).unwrap(),
    )
    .unwrap();
    LibraryConfig::load(temp.path(), "visibility-limits").unwrap()
}

fn snapshot(config: &LibraryConfig, arguments: Value) -> Result<Value> {
    compat_kb::execute(config, "compat_kb_visibility", &arguments)
        .map(|value| value["result"].clone())
}

fn assert_limit(error: Error, expected_resource: &str, expected_limit: u64) -> u64 {
    assert!(!error.to_string().contains("PRIVATE_SENTINEL"));
    match error {
        Error::VisibilityLimit {
            resource,
            limit,
            observed,
        } => {
            assert_eq!(resource, expected_resource);
            assert_eq!(limit, expected_limit);
            assert!(observed > limit);
            observed
        }
        other => panic!("expected a typed visibility limit, got {other:?}"),
    }
}

#[test]
fn file_limit_uses_config_settings_and_cannot_be_overridden_by_operation_args() {
    let (temp, initial) = fixture();
    for path in [
        initial.concepts.join("a.md"),
        initial.raw.join("b.md"),
        initial.outputs.join("c.md"),
    ] {
        fs::write(path, "").unwrap();
    }
    let limited = reload(&temp, json!({"max_files": 2}));
    let forged = json!({
        "max_files": 100,
        "max_read_bytes": 100_000,
        "max_header_bytes": 100_000,
        "visibility_snapshot": {"max_files": 100, "max_read_bytes": 100_000, "max_header_bytes": 100_000}
    });
    assert_eq!(
        assert_limit(snapshot(&limited, forged).unwrap_err(), "max_files", 2),
        3
    );
    let raised = reload(&temp, json!({"max_files": 3}));
    let result = snapshot(&raised, json!({})).unwrap();
    assert_eq!(result["compiled"].as_object().unwrap().len(), 1);
    assert_eq!(result["raw"].as_object().unwrap().len(), 1);
    assert_eq!(result["note"].as_object().unwrap().len(), 1);
    assert_eq!(result["has_nonpublic"], false);
    assert!(!raised.database().exists());
}

#[test]
fn total_byte_limit_charges_read_ahead_even_when_the_header_is_small() {
    let (temp, initial) = fixture();
    let path = initial.raw.join("PRIVATE_SENTINEL.md");
    let header = "---\nprivate: false\n---\n";
    assert!(header.len() < 128);
    fs::write(&path, format!("{header}{}", "x".repeat(4096))).unwrap();
    let limited = reload(
        &temp,
        json!({"max_read_bytes": 128, "max_header_bytes": 128}),
    );
    assert_limit(
        snapshot(&limited, json!({})).unwrap_err(),
        "max_read_bytes",
        128,
    );
    let raised = reload(
        &temp,
        json!({"max_read_bytes": 8192, "max_header_bytes": 128}),
    );
    assert_eq!(
        snapshot(&raised, json!({})).unwrap()["raw"]["PRIVATE_SENTINEL"]["public"],
        true
    );
    fs::write(&path, "---\nprivate: true\n---\nPRIVATE_SENTINEL").unwrap();
    let private = snapshot(&raised, json!({})).unwrap();
    assert_eq!(private["raw"]["PRIVATE_SENTINEL"]["public"], false);
    assert_eq!(private["has_nonpublic"], true);
}

#[test]
fn header_capacity_failure_is_not_silently_reclassified_as_private() {
    let (temp, initial) = fixture();
    fs::write(
        initial.concepts.join("a.md"),
        format!(
            "---\nprivate: false\npadding: {}\n---\nbody",
            "x".repeat(160)
        ),
    )
    .unwrap();
    let limited = reload(
        &temp,
        json!({"max_read_bytes": 4096, "max_header_bytes": 64}),
    );
    assert_limit(
        snapshot(&limited, json!({})).unwrap_err(),
        "max_header_bytes",
        64,
    );
    let raised = reload(
        &temp,
        json!({"max_read_bytes": 4096, "max_header_bytes": 512}),
    );
    assert_eq!(
        snapshot(&raised, json!({})).unwrap()["compiled"]["a"]["public"],
        true
    );
}

#[test]
fn malformed_or_unterminated_headers_within_capacity_remain_nonpublic() {
    let (temp, initial) = fixture();
    fs::write(
        initial.concepts.join("malformed.md"),
        "---\nprivate: [bad\n---\n",
    )
    .unwrap();
    fs::write(initial.raw.join("unterminated.md"), "---\nprivate: false\n").unwrap();
    let config = reload(
        &temp,
        json!({"max_read_bytes": 4096, "max_header_bytes": 128}),
    );
    let result = snapshot(&config, json!({})).unwrap();
    assert_eq!(result["compiled"]["malformed"]["public"], false);
    assert_eq!(result["raw"]["unterminated"]["public"], false);
    assert_eq!(result["has_nonpublic"], true);
}

#[test]
fn settings_are_strict_positive_integers_and_do_not_echo_invalid_values() {
    let (temp, _) = fixture();
    let mut invalid = vec![
        Value::Null,
        json!(false),
        json!([]),
        json!(7),
        json!("PRIVATE_SENTINEL"),
        json!({"unknown": "PRIVATE_SENTINEL"}),
        json!({"max_read_bytes": 64, "max_header_bytes": 65}),
    ];
    for resource in ["max_files", "max_read_bytes", "max_header_bytes"] {
        for value in [
            json!(0),
            json!(true),
            json!(-1),
            json!(1.5),
            json!("PRIVATE_SENTINEL"),
            json!(u64::MAX),
        ] {
            let mut limits = json!({});
            limits[resource] = value;
            invalid.push(limits);
        }
    }
    for limits in invalid {
        let config = reload(&temp, limits.clone());
        let error = snapshot(&config, json!({})).unwrap_err();
        assert!(
            matches!(error, Error::VisibilityConfig(_)),
            "{limits}: {error:?}"
        );
        assert!(!error.to_string().contains("PRIVATE_SENTINEL"));
    }
    for limits in [
        json!({}),
        json!({"max_files": 1}),
        json!({"max_read_bytes": 1, "max_header_bytes": 1}),
        json!({"max_files": u64::MAX - 1, "max_read_bytes": u64::MAX - 1, "max_header_bytes": u64::MAX - 1}),
    ] {
        let config = reload(&temp, limits);
        assert_eq!(
            snapshot(&config, json!({})).unwrap()["has_nonpublic"],
            false
        );
    }
}

#[test]
fn default_file_limit_can_be_raised_for_20001_empty_documents() {
    // Filesystem-heavy case: execute on the remote test runner, not the laptop.
    let (temp, config) = fixture();
    for index in 0..20_001 {
        fs::File::create(config.concepts.join(format!("empty-{index:05}.md"))).unwrap();
    }
    assert_eq!(
        assert_limit(
            snapshot(&config, json!({})).unwrap_err(),
            "max_files",
            20_000
        ),
        20_001
    );
    // Empty files read zero bytes; increasing the count does not require a
    // larger byte budget or weaken the privacy policy.
    let raised = reload(
        &temp,
        json!({"max_files": 20_001, "max_read_bytes": 1, "max_header_bytes": 1}),
    );
    let result = snapshot(&raised, json!({})).unwrap();
    assert_eq!(result["compiled"].as_object().unwrap().len(), 20_001);
    assert_eq!(result["has_nonpublic"], false);
    assert!(!raised.database().exists());
}
