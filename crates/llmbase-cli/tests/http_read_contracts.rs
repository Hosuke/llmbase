use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode},
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

fn fixture() -> (TempDir, Router, Router) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    fs::create_dir_all(root.path().join("raw/book")).unwrap();
    fs::write(root.path().join("config.yaml"),"branding:\n  name: Native Library\ncollections:\n  practice:\n    label: Practice\nllm:\n  api_key: NEVER-PUBLISH-THIS\n").unwrap();
    fs::write(root.path().join("wiki/concepts/a.md"),"---\ntitle: First / 學習\nsummary: First summary\ntags: [practice]\n---\n## English\nFirst links [[b]].\n## 中文\n𠮷學問。\n").unwrap();
    fs::write(
        root.path().join("wiki/concepts/b.md"),
        "---\ntitle: Second\nsummary: Second summary\ntags: [practice]\n---\nSecond links [[a]].",
    )
    .unwrap();
    fs::write(
        root.path().join("wiki/concepts/secret.md"),
        "---\ntitle: SECRET TITLE\nprivate: true\n---\nSECRET BODY",
    )
    .unwrap();
    fs::write(
        root.path().join("raw/book/index.md"),
        "---\ntitle: Original\ntype: book\ncompiled: false\n---\nOriginal raw body",
    )
    .unwrap();
    let config = LibraryConfig::load(root.path(), "web").unwrap();
    let mut library = Library::open(config.clone()).unwrap();
    library.rebuild_metadata().unwrap();
    let gateway = Gateway::new(Application::new(vec![config]).unwrap(), "web".into());
    let grant = AuthGrant::new("fixture-http-owner-secret", Principal::local("web")).unwrap();
    let web = router(HttpState::new(
        gateway.clone(),
        Surface::Web,
        true,
        vec![grant.clone()],
    ));
    let agent = router(HttpState::new(gateway, Surface::Agent, true, vec![grant]));
    (root, web, agent)
}
async fn get(app: Router, path: &str, headers: &[(&str, &str)]) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder().uri(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let response = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}

#[tokio::test]
async fn existing_frontend_read_sequence_has_real_web_envelopes_and_no_private_content() {
    let (_root, web, agent) = fixture();
    for path in [
        "/api/branding",
        "/api/languages",
        "/api/stats",
        "/api/collections",
        "/api/articles",
        "/api/articles/lite",
        "/api/taxonomy?lang=zh",
        "/api/aliases",
        "/api/entities",
        "/api/xici?lang=zh",
        "/api/sources",
        "/api/health",
        "/api/search?q=First",
    ] {
        let (status, _, body) = get(web.clone(), path, &[]).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert!(!body.to_string().contains("SECRET"), "{path}");
        assert!(!body.to_string().contains("NEVER-PUBLISH"));
    }
    let (_, _, stats) = get(web.clone(), "/api/stats", &[]).await;
    assert_eq!(stats["article_count"], 2);
    assert_eq!(stats["raw_count"], 1);
    assert_eq!(stats["link_count"], 2);
    assert!(stats.get("articles").is_none());
    let (_, _, agent_stats) = get(agent.clone(), "/api/stats", &[]).await;
    assert_eq!(agent_stats["articles"], 2);
    assert_eq!(agent_stats["status"], "ok");
    assert!(agent_stats.get("article_count").is_none());
    let (_, _, article) = get(web.clone(), "/api/articles/a", &[]).await;
    assert_eq!(article["backlinks"], json!([{"slug":"b","title":"Second"}]));
    assert!(article.get("backlinks_status").is_none());
    let (_, _, sections) = get(web.clone(), "/api/articles/a/sections", &[]).await;
    assert_eq!(sections["status"], "ok");
    assert!(
        sections["sections"][0]["anchor"]
            .as_str()
            .unwrap()
            .starts_with("h2-")
    );
    let (_, _, lite) = get(web.clone(), "/api/articles/lite", &[]).await;
    assert_eq!(lite["total"], 2);
    assert!(lite.get("content").is_none());
    let (_, _, raw) = get(web.clone(), "/api/sources/book", &[]).await;
    assert_eq!(raw["content"], "Original raw body");
    assert_eq!(raw["compiled"], false);
    let (_, _, export) = get(web.clone(), "/api/export/article/a", &[]).await;
    assert!(export["content"]["english"].is_string());
    let (_, _, graph) = get(web.clone(), "/api/export/graph/a?depth=1", &[]).await;
    assert_eq!(graph["count"], 2);
    assert_eq!(
        get(web.clone(), "/api/articles/secret", &[]).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(agent, "/api/languages", &[]).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn pagination_and_conditional_cache_validate_before_304_and_keep_scopes_separate() {
    let (_root, web, _) = fixture();
    let (status, headers, first) =
        get(web.clone(), "/api/articles?limit=1&fields=slug,title", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["next_cursor"], "a");
    assert_eq!(first["total"], 2);
    assert!(first["articles"][0].get("tags").is_none());
    let etag = headers["etag"].to_str().unwrap();
    let (status, headers, empty) = get(
        web.clone(),
        "/api/articles?limit=1&fields=slug,title",
        &[("if-none-match", etag)],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(empty.is_null());
    assert_eq!(headers["cache-control"], "private, no-cache");
    assert_eq!(
        get(
            web.clone(),
            "/api/articles?limit=0",
            &[("if-none-match", etag)]
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get(
            web.clone(),
            "/api/articles?limit=1",
            &[("if-none-match", etag), ("x-library-id", "other")]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, _, last) = get(web.clone(), "/api/articles?limit=1&cursor=a", &[]).await;
    assert_eq!(last["count"], 1);
    assert!(last["next_cursor"].is_null());
    let (_, _, filtered) = get(web.clone(), "/api/articles?q=second", &[]).await;
    assert_eq!(filtered["count"], 1);
    let (status, _, bad) = get(
        web.clone(),
        "/api/taxonomy?lang=bogus",
        &[("if-none-match", "*")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(bad["error"], "invalid lang");
    assert_eq!(bad["valid"], json!(["en", "zh", "ja", "zh-en"]));
    let (_, _, owner) = get(
        web,
        "/api/articles",
        &[("authorization", "Bearer fixture-http-owner-secret")],
    )
    .await;
    assert_eq!(owner["articles"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn privileged_read_routes_require_bearer_and_external_private_edits_invalidate_snapshot() {
    let (root, web, _) = fixture();
    fs::write(
        root.path().join("wiki/_meta/trails.json"),
        r#"{"trails":[{"id":"accepted","name":"Secret research"}]}"#,
    )
    .unwrap();
    for path in ["/api/trails", "/api/worker/status"] {
        assert_eq!(
            get(web.clone(), path, &[("cookie", "llmbase_auth=anything")])
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get(
                web.clone(),
                path,
                &[("authorization", "Bearer fixture-http-owner-secret")]
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    fs::write(
        root.path().join("wiki/concepts/a.md"),
        "---\nprivate: true\n---\nUpdated confidential content",
    )
    .unwrap();
    let (status, _, body) = get(web, "/api/articles", &[("if-none-match", "*")]).await;
    // A newly private source is hidden before the stale-revision check.
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.to_string().contains("confidential"));
}
