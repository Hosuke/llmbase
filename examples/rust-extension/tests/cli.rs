use llmbase_core::{Library, LibraryConfig};
use serde_json::Value;
use std::{fs, process::Command};
use tempfile::TempDir;

#[test]
fn standalone_cli_discovers_and_executes_the_public_extension() {
    const ORIGINAL: &str = "---\r\ntitle: Original notes\r\n---\r\n學而時習之。𠀀 e\u{301}\n";
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("raw")).unwrap();
    fs::write(root.path().join("raw/example.md"), ORIGINAL.as_bytes()).unwrap();
    let config = LibraryConfig::load(root.path(), "cli-example").unwrap();
    Library::open(config).unwrap().sync().unwrap();

    let invoke = |argument: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_llmbase-rust-extension-example"))
            .arg(root.path())
            .args(["cli-example", argument])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "example CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };

    let descriptors = invoke("--list");
    assert_eq!(descriptors.as_array().unwrap().len(), 1);
    let descriptor = &descriptors[0];
    assert_eq!(descriptor["name"], "example.inspect");
    assert_eq!(descriptor["input_schema"]["type"], "object");
    assert_eq!(
        descriptor["input_schema"]["properties"]["source_id"]["type"],
        "string"
    );
    assert_eq!(descriptor["input_schema"]["additionalProperties"], false);
    assert_eq!(descriptor["output_schema"]["type"], "object");
    assert_eq!(
        descriptor["output_schema"]["properties"]["default_lang"]["type"],
        "string"
    );
    assert!(
        descriptor["output_schema"]["properties"]
            .as_object()
            .unwrap()
            .contains_key("source")
    );

    let result = invoke("example");
    assert_eq!(result["library_id"], "cli-example");
    assert_eq!(result["default_lang"], "en");
    assert_eq!(result["bare_body"], true);
    assert_eq!(result["content_bytes"], ORIGINAL.len());
    assert_eq!(
        result["source"]["content"].as_str().unwrap().as_bytes(),
        ORIGINAL.as_bytes()
    );
    assert_eq!(
        fs::read(root.path().join("raw/example.md")).unwrap(),
        ORIGINAL.as_bytes()
    );
}
