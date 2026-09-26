use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use llmbase_cli::{
    Gateway,
    http::{AuthGrant, HttpState, Surface, router},
};
use llmbase_core::{
    LibraryConfig,
    application::{Application, Principal},
};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use tower::ServiceExt;

fn fixture(surface: Surface, pdf: bool) -> (TempDir, LibraryConfig, Router) {
    let root = TempDir::new().unwrap();
    let mut config = LibraryConfig::load(root.path(), "uploads").unwrap();
    if pdf {
        let reply = json!({"protocol_version":1,"documents":[{"path":"paper/chunk-001.md","content":"---\ntitle: PDF source\n---\nAccepted parser output.\n"}],"assets":[{"path":"paper/source.original","content_bytes":[37,80,68,70,0,255,13,10]}]}).to_string();
        let script = format!("cat > parser-input.json\nprintf '%s' '{reply}'");
        config.settings["plugins"] = json!({"process_adapters":{"pdf":{"command":["/bin/sh","-c",script],"timeout_seconds":2,"max_output_bytes":4096}}});
    }
    let grant = AuthGrant::new("fixture-upload-owner-token", Principal::local("uploads")).unwrap();
    let app = router(HttpState::new(
        Gateway::new(
            Application::new(vec![config.clone()]).unwrap(),
            "uploads".into(),
        ),
        surface,
        true,
        vec![grant],
    ));
    (root, config, app)
}

async fn send(
    app: Router,
    path: &str,
    content_type: &str,
    bytes: Vec<u8>,
    owner: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", content_type);
    if owner {
        request = request.header("authorization", "Bearer fixture-upload-owner-token");
    }
    let response = app
        .oneshot(request.body(Body::from(bytes)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&body)})),
    )
}

async fn upload(app: Router, filename: &str, bytes: &[u8], owner: bool) -> (StatusCode, Value) {
    let mut body = format!("--fixture-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--fixture-boundary--\r\n");
    send(
        app,
        "/api/upload",
        "multipart/form-data; boundary=fixture-boundary",
        body,
        owner,
    )
    .await
}

#[tokio::test]
async fn multipart_text_is_byte_exact_idempotent_and_can_exceed_json_body_limit() {
    let (_root, config, app) = fixture(Surface::Web, false);
    let mut original = b"---\r\ntitle: Original\r\nunknown: 42\r\n---\r\n".to_vec();
    original.extend_from_slice("原典𠮷。\r\n".as_bytes());
    original.extend(vec![b'a'; 1_050_000]);
    let (status, value) = upload(app.clone(), "Source.md", &original, true).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["filename"], "Source.md");
    let relative = value["path"].as_str().unwrap();
    assert_eq!(fs::read(config.raw.join(relative)).unwrap(), original);
    assert_eq!(
        upload(app, "Source.md", &original, true).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn multipart_denials_and_invalid_content_do_not_create_sources() {
    let (_root, config, app) = fixture(Surface::Web, false);
    assert_eq!(
        upload(app.clone(), "file.md", b"Unauthorized", false)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert!(!config.database().exists());
    for (filename, content) in [
        ("../escape.md", &b"bad"[..]),
        ("file.md", &[255, 0][..]),
        ("", &b"empty"[..]),
    ] {
        assert_eq!(
            upload(app.clone(), filename, content, true).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let (status, value) = send(
        app.clone(),
        "/api/upload",
        "multipart/form-data; boundary=fixture-boundary",
        b"--fixture-boundary--\r\n".to_vec(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(value["message"], "No file provided");
    let (status, value) = upload(app, "paper.pdf", b"%PDF no configured parser", true).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{value}");
    assert!(!config.raw.exists());
}

#[tokio::test]
async fn both_ingest_routes_delegate_scoped_existing_sources_and_reject_cross_library_paths() {
    for surface in [Surface::Web, Surface::Agent] {
        let (root, config, app) = fixture(surface, false);
        let bytes = "---\nunknown: 42\n---\nPreserved source 原典𠮷\n";
        fs::write(root.path().join("import.md"), bytes).unwrap();
        assert_eq!(
            send(
                app.clone(),
                "/api/ingest",
                "application/json",
                json!({"source":"import.md"}).to_string().into_bytes(),
                false
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let (status, value) = send(
            app.clone(),
            "/api/ingest",
            "application/json",
            json!({"source":"import.md"}).to_string().into_bytes(),
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["status"], "ok");
        assert_eq!(
            fs::read_to_string(config.raw.join(value["path"].as_str().unwrap())).unwrap(),
            bytes
        );
        assert_ne!(
            send(
                app,
                "/api/ingest",
                "application/json",
                json!({"source":"../outside.md"}).to_string().into_bytes(),
                true
            )
            .await
            .0,
            StatusCode::OK
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn pdf_upload_uses_operator_process_protocol_and_preserves_original_asset() {
    let (root, config, app) = fixture(Surface::Web, true);
    let bytes = [37, 80, 68, 70, 0, 255, 13, 10];
    let (status, value) = upload(app, "paper.pdf", &bytes, true).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(
        value,
        json!({"status":"ok","chunks":1,"filename":"paper.pdf"})
    );
    assert_eq!(
        fs::read(config.raw.join("paper/source.original")).unwrap(),
        bytes
    );
    assert!(
        fs::read_to_string(config.raw.join("paper/chunk-001.md"))
            .unwrap()
            .contains("Accepted parser output.")
    );
    let input: Value =
        serde_json::from_slice(&fs::read(root.path().join("parser-input.json")).unwrap()).unwrap();
    assert_eq!(input["input"]["content_bytes"], json!(bytes));
    assert_eq!(input["input"]["filename"], "paper.pdf");
    assert_eq!(input["input"]["chunk_pages"], 20);
}

#[cfg(unix)]
#[tokio::test]
async fn pdf_serialized_input_limit_fails_before_process_or_source_write() {
    let (root, config, app) = fixture(Surface::Web, true);
    let (status, value) = upload(app, "paper.pdf", &vec![255; 4_200_000], true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{value}");
    assert!(value.to_string().contains("adapter input exceeds 16 MiB"));
    assert!(!root.path().join("parser-input.json").exists());
    assert!(!config.raw.exists());
}
