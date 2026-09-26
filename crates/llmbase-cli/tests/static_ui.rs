use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use llmbase_cli::{
    Gateway,
    http::{self, HttpState, Surface},
    static_ui,
};
use llmbase_core::{LibraryConfig, application::Application};
use std::fs;
use tower::ServiceExt;

async fn get(router: &Router, path: &str) -> (u16, String, axum::http::HeaderMap) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap(), headers)
}

#[tokio::test]
async fn existing_web_build_assets_spa_and_custom_favicon_are_served_without_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("static/dist/assets")).unwrap();
    fs::write(
        root.path().join("static/dist/index.html"),
        "<title>Existing React application</title>",
    )
    .unwrap();
    fs::write(
        root.path().join("static/dist/assets/app.js"),
        "console.log('asset')",
    )
    .unwrap();
    fs::write(root.path().join("static/favicon.svg"), "<svg>custom</svg>").unwrap();
    let cfg = LibraryConfig::load(root.path(), "ui").unwrap();
    let gateway = Gateway::new(Application::new(vec![cfg.clone()]).unwrap(), "ui".into());
    let router = static_ui::web_router(
        http::router(HttpState::new(gateway, Surface::Web, false, vec![])),
        &cfg,
    );
    for path in ["/", "/wiki/%E5%AD%B8%E8%80%8C", "/explore"] {
        let (status, body, headers) = get(&router, path).await;
        assert_eq!(status, 200);
        assert!(body.contains("Existing React application"));
        assert!(!headers.contains_key("set-cookie"));
        assert_eq!(headers["x-content-type-options"], "nosniff");
    }
    let (status, body, headers) = get(&router, "/assets/app.js").await;
    assert_eq!(status, 200);
    assert!(body.contains("console.log"));
    assert_eq!(headers["content-type"], "text/javascript; charset=utf-8");
    assert_eq!(get(&router, "/favicon.svg").await.1, "<svg>custom</svg>");
    assert_eq!(get(&router, "/api/search?q=secret").await.0, 401);
    assert_eq!(get(&router, "/api/unknown").await.0, 404);
}

#[tokio::test]
async fn custom_static_directory_is_scoped_and_filesystem_escapes_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("reader")).unwrap();
    fs::write(root.path().join("reader/index.html"), "custom build").unwrap();
    fs::write(root.path().join("reader/.env"), "must not serve").unwrap();
    fs::write(outside.path().join("outside.txt"), "outside secret").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("outside.txt"),
        root.path().join("reader/leak.txt"),
    )
    .unwrap();
    fs::write(
        root.path().join("config.yaml"),
        "web:\n  static_dir: reader\n",
    )
    .unwrap();
    let cfg = LibraryConfig::load(root.path(), "ui").unwrap();
    let router = static_ui::web_router(Router::new(), &cfg);
    assert_eq!(get(&router, "/").await.1, "custom build");
    for path in [
        "/%2e%2e/outside.txt",
        "/.env",
        "/a%2f..%2fleak.txt",
        "/a%5cleak.txt",
        "/%ff",
        "/%00",
    ] {
        assert_eq!(get(&router, path).await.0, 400, "{path}");
    }
    #[cfg(unix)]
    assert_eq!(get(&router, "/leak.txt").await.0, 404);
    fs::write(
        root.path().join("config.yaml"),
        format!("web:\n  static_dir: {}\n", outside.path().display()),
    )
    .unwrap();
    let cfg = LibraryConfig::load(root.path(), "ui").unwrap();
    assert_eq!(
        get(&static_ui::web_router(Router::new(), &cfg), "/outside.txt")
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn lightweight_reader_uses_legacy_search_envelope_and_scoped_core() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    fs::write(
        root.path().join("wiki/concepts/public.md"),
        "---\ntitle: Library\nsummary: Indexed public evidence\n---\npublic library source",
    )
    .unwrap();
    fs::write(
        root.path().join("wiki/concepts/private.md"),
        "---\ntitle: Private\nprivate: true\n---\nprivate library secret",
    )
    .unwrap();
    let cfg = LibraryConfig::load(root.path(), "ui").unwrap();
    let mut library = llmbase_core::Library::open(cfg.clone()).unwrap();
    library.sync().unwrap();
    let gateway = Gateway::new(Application::new(vec![cfg]).unwrap(), "ui".into());
    let router = http::search_router(HttpState::new(gateway, Surface::Web, true, vec![]));
    let (status, html, _) = get(&router, "/").await;
    assert_eq!(status, 200);
    assert!(html.contains("LLMBase Search"));
    assert!(html.contains("node.textContent=value"));
    assert!(!html.contains("innerHTML"));
    let (status, body, _) = get(&router, "/api/search?q=library&top_k=1").await;
    assert_eq!(status, 200, "{body}");
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["query"], "library");
    assert!(value.get("status").is_none());
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
    assert!(!body.contains("private library secret"));
    assert_eq!(get(&router, "/api/ask").await.0, 404);
}
