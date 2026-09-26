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
    Library, LibraryConfig,
    application::{Application, Principal},
};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;
use tower::ServiceExt;

fn fixture() -> (TempDir, Router) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    fs::write(root.path().join("wiki/concepts/a.md"),"---\ntitle: Accepted article\ntags: [practice]\n---\nAn accepted article with enough content to be preserved during a deterministic cleanup pass.").unwrap();
    fs::write(
        root.path().join("wiki/concepts/stub.md"),
        "---\ntitle: Stub\n---\nShort",
    )
    .unwrap();
    let config = LibraryConfig::load(root.path(), "writes").unwrap();
    Library::open(config.clone()).unwrap().sync().unwrap();
    let grant = AuthGrant::new("fixture-owner-write-token", Principal::local("writes")).unwrap();
    let app = router(HttpState::new(
        Gateway::new(Application::new(vec![config]).unwrap(), "writes".into()),
        Surface::Web,
        true,
        vec![grant],
    ));
    (root, app)
}
async fn request(
    app: Router,
    method: &str,
    path: &str,
    payload: Value,
    owner: bool,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if owner {
        req = req.header("authorization", "Bearer fixture-owner-write-token");
    }
    let response = app
        .oneshot(req.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn real_legacy_deterministic_writes_share_auth_application_and_journals() {
    let (root, app) = fixture();
    for (method, path, payload) in [
        ("DELETE", "/api/articles/a", json!({})),
        ("POST", "/api/wiki/clean", json!({})),
        ("POST", "/api/taxonomy/update", json!({"categories":[]})),
        (
            "POST",
            "/api/trails",
            json!({"step":{"type":"article","slug":"a"}}),
        ),
    ] {
        assert_eq!(
            request(app.clone(), method, path, payload, false).await.0,
            StatusCode::FORBIDDEN
        );
    }
    assert!(root.path().join("wiki/concepts/a.md").exists());
    let (status, cleaned) = request(app.clone(), "POST", "/api/wiki/clean", json!({}), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleaned["removed"], 1);
    assert!(!root.path().join("wiki/concepts/stub.md").exists());
    let (status, updated) = request(
        app.clone(),
        "POST",
        "/api/taxonomy/update",
        json!({"categories":[{"id":"practice","label":"Practice","article_slugs":["a"]}]}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["locked"], true);
    let (status, saved) = request(
        app.clone(),
        "POST",
        "/api/trails",
        json!({"name":"Research","step":{"type":"article","slug":"a","ts":"forged"}}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(saved["trail"]["steps"][0]["ts"], "forged");
    let id = saved["trail"]["id"].as_str().unwrap();
    let (status, deleted) = request(
        app.clone(),
        "POST",
        &format!("/api/trails/{id}/delete"),
        json!({}),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["status"], "ok");
    let (status, deleted) =
        request(app.clone(), "DELETE", "/api/articles/a", json!({}), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["deleted"], "a");
    assert!(!root.path().join("wiki/concepts/a.md").exists());
    assert_eq!(
        request(app, "DELETE", "/api/articles/a", json!({}), true)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
