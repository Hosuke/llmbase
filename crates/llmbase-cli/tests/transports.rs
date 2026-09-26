use axum::{
    Router,
    body::Body,
    http::{Request as HttpRequest, StatusCode},
};
use http_body_util::BodyExt;
use llmbase_cli::{
    Gateway,
    http::{AuthGrant, HttpState, Surface, router},
};
use llmbase_core::{
    LibraryConfig,
    application::{Application, Principal, Request},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower::ServiceExt;

fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("raw")).unwrap();
    std::fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    std::fs::write(root.path().join("raw/classic.md"), "---\ntitle: 經典\nsource: https://example.invalid/classic\n---\n學而時習之，不亦說乎。𠮷野春風。\n").unwrap();
    std::fs::write(
        root.path().join("wiki/concepts/learning.md"),
        "---\ntitle: 學習\naliases: [學問]\ntags: [practice]\n---\n## 中文\n學而時習之。\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("wiki/concepts/private.md"),
        "---\ntitle: 內部手札\nprivate: true\n---\n不可公開的學習密記。\n",
    )
    .unwrap();
    root
}

fn application(base: &Path, second: Option<&Path>) -> Application {
    let mut configs = vec![LibraryConfig::load(base, "evaluation").unwrap()];
    if let Some(second) = second {
        configs.push(LibraryConfig::load(second, "other").unwrap());
    }
    let app = Application::new(configs).unwrap();
    app.execute(
        &Principal::local("evaluation"),
        Request {
            library_id: "evaluation".into(),
            operation: "index".into(),
            args: json!({}),
        },
    )
    .unwrap();
    if second.is_some() {
        app.execute(
            &Principal::local("other"),
            Request {
                library_id: "other".into(),
                operation: "index".into(),
                args: json!({}),
            },
        )
        .unwrap();
    }
    app
}

fn cli(base: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_llmbase"))
        .env_clear() // No Python, model credentials or inherited site packages.
        .arg("--base-dir")
        .arg(base)
        .args(["--library-id", "evaluation", "--json"])
        .args(args)
        .output()
        .unwrap()
}

#[tokio::test]
async fn legacy_exports_sections_and_taxonomy_share_scoped_native_reads() {
    let root = fixture();
    let gateway = Gateway::new(application(root.path(), None), "evaluation".into());
    let principal = Principal::public("evaluation");
    let sections = gateway
        .legacy(
            principal.clone(),
            "evaluation".into(),
            "kb_get_sections",
            json!({"slug":"學問"}),
        )
        .await
        .unwrap();
    assert_eq!(sections["found"], true);
    assert_eq!(sections["slug"], "learning");
    assert_eq!(sections["sections"][0]["title"], "中文");
    let hidden = gateway
        .legacy(
            principal.clone(),
            "evaluation".into(),
            "kb_get_sections",
            json!({"slug":"private"}),
        )
        .await
        .unwrap();
    assert_eq!(hidden, json!({"found":false,"slug":"private"}));
    let exported = gateway
        .legacy(
            principal.clone(),
            "evaluation".into(),
            "kb_export_article",
            json!({"slug":"learning"}),
        )
        .await
        .unwrap();
    assert_eq!(exported["content"]["zh"], "學而時習之。");
    let tag = gateway
        .legacy(
            principal.clone(),
            "evaluation".into(),
            "kb_export",
            json!({"type":"tag","slug":"practice"}),
        )
        .await
        .unwrap();
    assert_eq!(tag["count"], 1);
    let taxonomy = gateway
        .legacy(principal, "evaluation".into(), "kb_taxonomy", json!({}))
        .await
        .unwrap();
    assert!(taxonomy["categories"].is_array());
    assert_eq!(taxonomy.as_object().unwrap().len(), 1);
    assert!(!taxonomy.to_string().contains("內部手札"));
}

#[test]
fn cli_exports_and_finite_jobs_run_without_interpreter_environment() {
    let root = fixture();
    assert!(cli(root.path(), &["index"]).status.success());
    let output = cli(root.path(), &["export", "article", "learning"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["slug"],
        "learning"
    );
    let output = cli(root.path(), &["jobs", "enqueue", "index"]);
    assert!(output.status.success());
    let queued: Value = serde_json::from_slice(&output.stdout).unwrap();
    let output = cli(root.path(), &["jobs", "run-one", "--worker-id", "cli-test"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let completed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(completed["id"], queued["id"]);
    assert_eq!(completed["status"], "succeeded");
    let output = cli(root.path(), &["lint", "check"]);
    assert!(output.status.success());
}

#[test]
fn migration_cli_preserves_original_and_verifies_copy_backup_restore_and_rollback() {
    let original = fixture();
    let targets = tempfile::tempdir().unwrap();
    let copy = targets.path().join("copy");
    let backup = targets.path().join("backup");
    let restored = targets.path().join("restored");
    let original_bytes = std::fs::read(original.path().join("raw/classic.md")).unwrap();
    let dry_run = cli(original.path(), &["migration", "dry-run"]);
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let scan: Value = serde_json::from_slice(&dry_run.stdout).unwrap();
    assert_eq!(scan["native_database"], false);
    assert!(
        !original.path().join("wiki/_meta").exists(),
        "dry-run mutated original"
    );
    for args in [
        vec!["migration", "copy", copy.to_str().unwrap()],
        vec!["migration", "verify", copy.to_str().unwrap()],
        vec!["migration", "backup", backup.to_str().unwrap()],
        vec![
            "migration",
            "restore",
            backup.to_str().unwrap(),
            restored.to_str().unwrap(),
        ],
        vec!["migration", "verify", restored.to_str().unwrap()],
        vec!["migration", "rollback", backup.to_str().unwrap()],
    ] {
        let output = cli(original.path(), &args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            serde_json::from_slice::<Value>(&output.stdout)
                .unwrap()
                .is_object()
        );
    }
    assert!(
        !original.path().join("wiki/_meta").exists(),
        "copy administration mutated original"
    );
    assert_eq!(
        std::fs::read(original.path().join("raw/classic.md")).unwrap(),
        original_bytes
    );
    assert_eq!(
        std::fs::read(restored.join("raw/classic.md")).unwrap(),
        original_bytes
    );
    std::fs::write(restored.join("raw/classic.md"), "tampered").unwrap();
    let failed = cli(
        original.path(),
        &["migration", "verify", restored.to_str().unwrap()],
    );
    assert!(
        !failed.status.success(),
        "verification accepted changed source bytes"
    );
    assert!(failed.stdout.is_empty());
}

#[test]
fn standalone_cli_indexes_searches_and_resolves_original_evidence_without_python() {
    let dir = fixture();
    let original = std::fs::read(dir.path().join("raw/classic.md")).unwrap();
    let indexed = cli(dir.path(), &["op", "index", "--args", "{}"]);
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    let index: Value = serde_json::from_slice(&indexed.stdout).unwrap();
    assert_eq!(index["added"], 3);
    let searched = cli(
        dir.path(),
        &[
            "op",
            "search",
            "--args",
            r#"{"query":"學而時習之","kind":"raw","top_k":10}"#,
        ],
    );
    assert!(
        searched.status.success(),
        "{}",
        String::from_utf8_lossy(&searched.stderr)
    );
    let result: Value = serde_json::from_slice(&searched.stdout).unwrap();
    assert_eq!(result["library_id"], "evaluation");
    let evidence = &result["results"][0]["evidence"];
    assert_eq!(evidence["offset_unit"], "utf8_byte");
    let start = evidence["start"].as_u64().unwrap() as usize;
    let end = evidence["end"].as_u64().unwrap() as usize;
    assert_eq!(
        &original[start..end],
        evidence["exact_quote"].as_str().unwrap().as_bytes()
    );
    let args = json!({"evidence":evidence}).to_string();
    let resolved = cli(dir.path(), &["op", "evidence", "--args", &args]);
    assert!(resolved.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&resolved.stdout).unwrap()["stale"],
        false
    );
    assert_eq!(
        std::fs::read(dir.path().join("raw/classic.md")).unwrap(),
        original
    );
    let again = cli(dir.path(), &["op", "index", "--args", "{}"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&again.stdout).unwrap()["unchanged"],
        3
    );
}

#[test]
fn cli_errors_use_stderr_and_nonzero_exit_without_secret_echo() {
    let dir = fixture();
    let output = cli(dir.path(), &["op", "search", "--args", "[]"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"]["code"],
        "invalid_request"
    );
    let output = cli(dir.path(), &["ops", "call", "does_not_exist"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

async fn send(
    app: Router,
    method: &str,
    uri: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .oneshot(
            builder
                .body(if method == "GET" {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)})),
    )
}

#[tokio::test]
async fn http_scope_and_capability_checks_apply_to_native_and_legacy_routes() {
    let first = fixture();
    let second = fixture();
    let gateway = Gateway::new(
        application(first.path(), Some(second.path())),
        "evaluation".into(),
    );
    let grant = AuthGrant::new(
        "fixture-explicit-bearer-token",
        Principal::local("evaluation"),
    )
    .unwrap();
    let app = router(HttpState::new(gateway, Surface::Web, true, vec![grant]));
    let (status, value) = send(
        app.clone(),
        "GET",
        "/api/search?q=學而時習之",
        json!({}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["query"], "學而時習之");
    assert!(value.get("status").is_none());
    assert!(!value.to_string().contains("private.md"));
    let (status, _) = send(
        app.clone(),
        "POST",
        "/api/index/rebuild",
        json!({}),
        &[("cookie", "llmbase_auth=visitor")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        app.clone(),
        "POST",
        "/api/index/rebuild",
        json!({}),
        &[("authorization", "Bearer fixture-explicit-bearer-token")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        app.clone(),
        "POST",
        "/api/v1/op",
        json!({"library_id":"other","operation":"list","args":{}}),
        &[("authorization", "Bearer fixture-explicit-bearer-token")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        app.clone(),
        "GET",
        "/api/articles",
        json!({}),
        &[("x-library-id", "other")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        app.clone(),
        "GET",
        "/api/articles",
        json!({}),
        &[("authorization", "fixture-explicit-bearer-token")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status,value)=send(app,"POST","/api/v1/op",json!({"library_id":"evaluation","operation":"search","args":{"query":"x","nested":{"apiKey":"DO-NOT-REPEAT"}}}),&[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!value.to_string().contains("DO-NOT-REPEAT"));
}

#[tokio::test]
async fn factories_keep_distinct_envelopes_and_missing_capabilities_do_not_succeed() {
    let dir = fixture();
    let gateway = Gateway::new(application(dir.path(), None), "evaluation".into());
    let agent = router(HttpState::new(
        gateway.clone(),
        Surface::Agent,
        true,
        vec![AuthGrant::new("test-operator-token", Principal::local("evaluation")).unwrap()],
    ));
    let (_, value) = send(
        agent.clone(),
        "GET",
        "/api/search?q=學而時習之",
        json!({}),
        &[],
    )
    .await;
    assert_eq!(value["status"], "ok");
    assert!(value.get("query").is_none());
    let (_, value) = send(agent.clone(), "POST", "/api/op/kb_list", json!({}), &[]).await;
    assert_eq!(value["status"], "ok");
    assert!(value["result"]["articles"].is_array());
    let (status, value) = send(agent.clone(), "POST", "/api/op/kb_compile", json!({}), &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(value["error"]["code"], "forbidden");
    let (status, value) = send(
        agent,
        "POST",
        "/api/op/kb_compile",
        json!({}),
        &[("authorization", "Bearer test-operator-token")],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(value["error"]["code"], "capability_unavailable");
    let web = router(HttpState::new(gateway, Surface::Web, true, vec![]));
    let (_, value) = send(web, "GET", "/api/articles", json!({}), &[]).await;
    assert!(value.get("status").is_none());
    assert_eq!(value["articles"].as_array().unwrap().len(), 1);
}

async fn mcp_line(
    input: &mut tokio::process::ChildStdin,
    output: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    message: Value,
) -> Value {
    input
        .write_all(format!("{message}\n").as_bytes())
        .await
        .unwrap();
    input.flush().await.unwrap();
    let line = tokio::time::timeout(Duration::from_secs(10), output.next_line())
        .await
        .expect("MCP response timeout")
        .unwrap()
        .expect("MCP stdout closed");
    serde_json::from_str(&line).expect("stdout must only contain valid MCP JSON, not banners/logs")
}

#[tokio::test]
async fn stdio_initialization_tool_discovery_execution_errors_and_eof() {
    let dir = fixture();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_llmbase"))
        .env_clear()
        .arg("--base-dir")
        .arg(dir.path())
        .args(["--library-id", "evaluation", "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
    let init=mcp_line(&mut input,&mut output,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"contract-test","version":"1"}}})).await;
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "llmbase");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    input
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    let listed = mcp_line(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "kb_search")
        .unwrap();
    assert_eq!(search["inputSchema"]["required"], json!(["query"]));
    assert_eq!(search["inputSchema"]["properties"]["top_k"]["default"], 10);
    let index=mcp_line(&mut input,&mut output,json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kb_rebuild_index","arguments":{}}})).await;
    assert_ne!(index["result"]["isError"], true);
    let result: Value =
        serde_json::from_str(index["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["article_count"], 2);
    let searched=mcp_line(&mut input,&mut output,json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"kb_search_raw","arguments":{"query":"學而時習之"}}})).await;
    assert_ne!(searched["result"]["isError"], true);
    let result: Value =
        serde_json::from_str(searched["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["results"][0]["evidence"]["library_id"], "evaluation");
    let unknown=mcp_line(&mut input,&mut output,json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"unknown_tool","arguments":{}}})).await;
    assert!(unknown["error"].is_object());
    let unavailable=mcp_line(&mut input,&mut output,json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"kb_compile","arguments":{}}})).await;
    assert_eq!(unavailable["result"]["isError"], true);
    assert!(
        unavailable["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("capability_unavailable")
    );
    drop(input);
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());
}
