//! Static readers contain no library data and confer no API authority.
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::{Method, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use llmbase_core::{Error, LibraryConfig, config::scoped_path};
use std::{io::Read, path::PathBuf};

pub async fn search_page() -> Html<&'static str> {
    Html(include_str!("search.html"))
}

#[derive(Clone)]
struct StaticFiles {
    root: PathBuf,
    favicon: PathBuf,
}

/// Keep API routing separate: missing `/api/*` must never become SPA HTML.
pub fn web_router(api: Router, config: &LibraryConfig) -> Router {
    let selected = config
        .settings
        .pointer("/web/static_dir")
        .and_then(|v| v.as_str());
    let fallback = config.base.join("static/dist");
    let candidate = selected
        .map(|path| config.base.join(path))
        .unwrap_or_else(|| fallback.clone());
    let root = candidate
        .canonicalize()
        .ok()
        .filter(|path| path.starts_with(&config.base))
        .unwrap_or(fallback);
    let files = StaticFiles {
        root,
        favicon: config.base.join("static"),
    };
    let static_routes = Router::new()
        .route("/", get(spa))
        .fallback(spa)
        .with_state(files);
    api.merge(static_routes)
}

async fn spa(
    State(files): State<StaticFiles>,
    method: Method,
    OriginalUri(uri): OriginalUri,
) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let path = match decode_path(uri.path()) {
        Some(path) => path,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };
    if path == "api" || path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let result = tokio::task::spawn_blocking(move || read_static(&files, &path)).await;
    match result {
        Ok(Ok((mime, bytes))) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            if method == Method::HEAD {
                Vec::new()
            } else {
                bytes
            },
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

fn decode_path(encoded: &str) -> Option<String> {
    let mut decoded = Vec::new();
    let mut bytes = encoded.trim_start_matches('/').bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(if byte == b'%' {
            let hi = (bytes.next()? as char).to_digit(16)?;
            let lo = (bytes.next()? as char).to_digit(16)?;
            (hi * 16 + lo) as u8
        } else {
            byte
        });
    }
    let path = String::from_utf8(decoded).ok()?;
    if path.contains('\\')
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| part.starts_with('.'))
    {
        return None;
    }
    Some(path)
}

fn read_static(files: &StaticFiles, relative: &str) -> Result<(&'static str, Vec<u8>), Error> {
    let mut root = &files.root;
    if relative == "favicon.svg" && files.favicon.join(relative).is_file() {
        root = &files.favicon;
    }
    let mut path = if relative.is_empty() {
        root.join("index.html")
    } else {
        scoped_path(root, relative)?
    };
    if !path.is_file() {
        path = scoped_path(&files.root, "index.html")?;
    }
    let root = root.canonicalize()?;
    if !path.canonicalize()?.starts_with(&root) {
        return Err(Error::Scope);
    }
    let mime = match path.extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("wasm") => "application/wasm",
        _ => return Err(Error::NotFound("static asset".into())),
    };
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(Error::Invalid("static asset exceeds 32 MiB".into()));
    }
    Ok((mime, bytes))
}

pub async fn serve(router: Router, address: std::net::SocketAddr) -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!("LLMBase HTTP listening on {}", listener.local_addr()?);
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
