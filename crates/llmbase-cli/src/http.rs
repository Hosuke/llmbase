use crate::{Gateway, article, error_payload, native_tool_available, operation_catalog};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, OriginalUri, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{delete, get, post},
};
use llmbase_core::{
    Error, Source,
    application::{Capability, Principal, Request},
    ask::{AskEventKind, AskModels, AskRequest, AskStream},
};
use llmbase_model::CancellationToken;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Web,
    Agent,
}

#[derive(Clone)]
pub struct AuthGrant {
    token_hash: [u8; 32],
    principal: Principal,
}
impl AuthGrant {
    pub fn new(token: &str, principal: Principal) -> Result<Self, Error> {
        if token.len() < 16 || token.len() > 4096 {
            return Err(Error::Invalid(
                "server bearer tokens must contain 16..4096 bytes".into(),
            ));
        }
        Ok(Self {
            token_hash: Sha256::digest(token.as_bytes()).into(),
            principal,
        })
    }
}

#[derive(Deserialize)]
struct EnvGrant {
    token_env: String,
    libraries: HashSet<String>,
    capabilities: HashSet<Capability>,
}

/// Credentials stay in environment variables, never command-line arguments or
/// configuration files. LLMBASE_AUTH_GRANTS names token variables and scopes.
pub fn grants_from_environment(default_library: &str) -> Result<Vec<AuthGrant>, Error> {
    let mut grants = Vec::new();
    if let Ok(token) = std::env::var("LLMBASE_API_SECRET")
        && !token.is_empty()
    {
        let mut principal = Principal::local(default_library);
        principal.id = "configured-server-owner".into();
        grants.push(AuthGrant::new(&token, principal)?);
    }
    if let Ok(raw) = std::env::var("LLMBASE_AUTH_GRANTS") {
        let configs: Vec<EnvGrant> = serde_json::from_str(&raw).map_err(|_| {
            Error::Invalid(
                "LLMBASE_AUTH_GRANTS must be an array of token_env/libraries/capabilities objects"
                    .into(),
            )
        })?;
        for (index, config) in configs.into_iter().enumerate() {
            let token = std::env::var(&config.token_env).map_err(|_| {
                Error::Invalid("an auth grant token environment variable is missing".into())
            })?;
            if config.libraries.is_empty() || config.capabilities.is_empty() {
                return Err(Error::Invalid(
                    "auth grant must name libraries and capabilities".into(),
                ));
            }
            grants.push(AuthGrant::new(
                &token,
                Principal {
                    id: format!("grant-{index}"),
                    libraries: config.libraries,
                    capabilities: config.capabilities,
                },
            )?);
        }
    }
    Ok(grants)
}

#[derive(Clone)]
pub struct HttpState {
    pub gateway: Gateway,
    pub surface: Surface,
    pub public_read: bool,
    pub grants: Vec<AuthGrant>,
    concurrency: Arc<Semaphore>,
    models: Option<AskModels>,
    active: Arc<Mutex<HashMap<String, ActiveRequest>>>,
}

struct ActiveRequest {
    actor: String,
    library_id: String,
    cancel: CancellationToken,
}

/// Its lifetime is the HTTP body lifetime, including time under backpressure.
/// Disconnects drop both this lease and AskStream, cancelling upstream work.
struct AskLease {
    id: String,
    active: Arc<Mutex<HashMap<String, ActiveRequest>>>,
    _permit: OwnedSemaphorePermit,
}

impl Drop for AskLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock()
            && let Some(request) = active.remove(&self.id)
        {
            request.cancel.cancel();
        }
    }
}

impl HttpState {
    pub fn new(
        gateway: Gateway,
        surface: Surface,
        public_read: bool,
        grants: Vec<AuthGrant>,
    ) -> Self {
        Self {
            gateway,
            surface,
            public_read,
            grants,
            concurrency: Arc::new(Semaphore::new(16)),
            models: None,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_models(mut self, models: Option<AskModels>) -> Self {
        self.gateway.models = models.clone();
        self.models = models;
        self
    }

    fn start_ask(
        &self,
        headers: &HeaderMap,
        raw: Value,
    ) -> Result<(AskStream, AskLease), HttpError> {
        if contains_credential(&raw) {
            return Err(Error::Invalid(
                "credentials are not accepted in ask request bodies".into(),
            )
            .into());
        }
        let principal = self.principal(headers)?;
        let mut request: AskRequest = serde_json::from_value(raw)
            .map_err(|_| Error::Invalid("invalid native ask request".into()))?;
        if request.library_id.is_empty() {
            request.library_id = self.library(headers)?;
        }
        let id = request
            .request_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err(Error::Invalid(
                "request_id must contain 1..128 ASCII letters, digits, hyphens or underscores"
                    .into(),
            )
            .into());
        }
        request.request_id = Some(id.clone());
        let models = self
            .models
            .clone()
            .ok_or_else(|| Error::Capability("no generation endpoint configured".into()))?;
        let permit = self
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        let cancel = CancellationToken::new();
        let mut active = self.active.lock().map_err(|_| Error::Busy)?;
        if active.contains_key(&id) {
            return Err(Error::Conflict("request_id already active".into()).into());
        }
        // Authorization and request validation happen synchronously in the
        // application before any provider call or registry publication.
        let stream = self.gateway.application.start_ask(
            &principal,
            models,
            request.clone(),
            cancel.clone(),
        )?;
        active.insert(
            id.clone(),
            ActiveRequest {
                actor: principal.id,
                library_id: request.library_id,
                cancel,
            },
        );
        drop(active);
        Ok((
            stream,
            AskLease {
                id,
                active: self.active.clone(),
                _permit: permit,
            },
        ))
    }

    fn principal(&self, headers: &HeaderMap) -> Result<Principal, HttpError> {
        if let Some(raw) = headers.get("authorization") {
            let token = raw
                .to_str()
                .ok()
                .and_then(|v| v.strip_prefix("Bearer "))
                .filter(|v| !v.is_empty())
                .ok_or_else(HttpError::unauthorized)?;
            if token.len() > 4096 {
                return Err(HttpError::unauthorized());
            }
            let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
            for grant in &self.grants {
                if bool::from(digest.ct_eq(&grant.token_hash)) {
                    return Ok(grant.principal.clone());
                }
            }
            return Err(HttpError::unauthorized());
        }
        // Cookies, including the old SPA convenience cookie, grant no authority.
        if self.public_read {
            Ok(Principal::public(&self.gateway.default_library))
        } else {
            Err(HttpError::unauthorized())
        }
    }

    fn prepare_transcription(
        &self,
        headers: &HeaderMap,
        raw: Value,
    ) -> Result<llmbase_core::transcription::PreparedTranscription, HttpError> {
        if contains_credential(&raw) {
            return Err(Error::Invalid(
                "credentials are not accepted in transcription bodies".into(),
            )
            .into());
        }
        let principal = self.principal(headers)?;
        let request: llmbase_core::transcription::TranscriptionRequest =
            serde_json::from_value(raw)
                .map_err(|_| Error::Invalid("invalid transcription request".into()))?;
        if request.library_id != self.library(headers)? {
            return Err(Error::Scope.into());
        }
        Ok(self
            .gateway
            .application
            .prepare_transcription(&principal, request)?)
    }

    fn library(&self, headers: &HeaderMap) -> Result<String, HttpError> {
        match headers.get("x-library-id") {
            Some(value) => value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| HttpError::from(Error::Invalid("invalid library header".into()))),
            None => Ok(self.gateway.default_library.clone()),
        }
    }

    async fn execute(
        &self,
        headers: HeaderMap,
        mut request: Request,
        legacy: bool,
    ) -> Result<Value, HttpError> {
        if contains_credential(&request.args) {
            return Err(Error::Invalid("LLM credentials are not accepted in request bodies; configure the server environment".into()).into());
        }
        let principal = self.principal(&headers)?;
        if request.library_id.is_empty() {
            request.library_id = self.library(&headers)?;
        }
        if matches!(request.operation.as_str(), "kb_ask" | "legacy_query") {
            if let Some(model) = request.args.get_mut("model")
                && !model.is_null()
            {
                let text = model
                    .as_str()
                    .ok_or_else(|| Error::Invalid("model must be a string".into()))?
                    .trim()
                    .to_owned();
                if text.len() > 200 {
                    return Err(Error::Invalid("model too long".into()).into());
                }
                if !text.is_empty() {
                    let allowed = std::env::var("LLMBASE_MODEL_ALLOWLIST").unwrap_or_default();
                    if !allowed.trim().is_empty()
                        && !allowed.split(',').any(|name| name.trim() == text)
                    {
                        return Err(Error::Invalid(
                            "model is not in LLMBASE_MODEL_ALLOWLIST".into(),
                        )
                        .into());
                    }
                    *model = json!(text);
                } else {
                    *model = Value::Null
                }
            }
            if let Some(value) = headers.get("x-llm-key") {
                let key = value
                    .to_str()
                    .map_err(|_| Error::Invalid("invalid X-LLM-Key header".into()))?
                    .trim();
                if key.len() > 500 {
                    return Err(Error::Invalid("X-LLM-Key too long".into()).into());
                }
                if !key.is_empty() {
                    request.args["api_key"] = json!(key);
                }
            }
        }
        let _permit = self
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| HttpError::from(Error::Busy))?;
        if legacy {
            self.gateway
                .legacy(
                    principal,
                    request.library_id,
                    &request.operation,
                    request.args,
                )
                .await
                .map_err(Into::into)
        } else {
            self.gateway
                .execute(principal, request)
                .await
                .map_err(Into::into)
        }
    }

    async fn legacy(
        &self,
        headers: HeaderMap,
        operation: &str,
        args: Value,
    ) -> Result<Value, HttpError> {
        self.execute(
            headers,
            Request {
                library_id: String::new(),
                operation: operation.into(),
                args,
            },
            true,
        )
        .await
    }

    async fn read(
        &self,
        headers: HeaderMap,
        operation: &str,
        args: Value,
    ) -> Result<Value, HttpError> {
        self.execute(
            headers,
            Request {
                library_id: String::new(),
                operation: operation.into(),
                args,
            },
            false,
        )
        .await
    }

    fn success(&self, mut payload: Value) -> Json<Value> {
        if self.surface == Surface::Agent {
            payload["status"] = json!("ok");
        }
        Json(payload)
    }
}

pub struct HttpError {
    status: StatusCode,
    payload: Value,
}
impl HttpError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            payload: json!({"code":"unauthorized","message":"bearer authentication required"}),
        }
    }
}
impl From<Error> for HttpError {
    fn from(error: Error) -> Self {
        let status = match &error {
            Error::Invalid(_) | Error::Json(_) | Error::Yaml(_) => StatusCode::BAD_REQUEST,
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Scope => StatusCode::FORBIDDEN,
            Error::Conflict(_) | Error::Busy => StatusCode::CONFLICT,
            Error::Capability(_) => StatusCode::NOT_IMPLEMENTED,
            Error::Model(_) => StatusCode::BAD_GATEWAY,
            Error::Io(_) | Error::Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            status,
            payload: error_payload(&error),
        }
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"status":"error","error":self.payload})),
        )
            .into_response()
    }
}

fn contains_credential(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let canonical = key.to_lowercase().replace(['-', '_'], "");
            ["apikey", "xllmkey", "llmkey"]
                .iter()
                .any(|fragment| canonical.contains(fragment))
                || contains_credential(value)
        }),
        Value::Array(items) => items.iter().any(contains_credential),
        _ => false,
    }
}

pub fn router(state: HttpState) -> Router {
    let routes = Router::new()
        .route(
            "/api/healthz",
            get(|| async { Json(json!({"status":"ok"})) }),
        )
        .route("/api/v1/op", post(native_op))
        .route("/api/v1/evidence/resolve", post(resolve_evidence))
        .route("/api/v1/ask", post(ask))
        .route("/api/v1/ask/stream", post(ask_stream))
        .route(
            "/api/v1/transcriptions/prepare",
            post(transcription_prepare),
        )
        .route("/api/v1/transcriptions/ask", post(transcription_ask))
        .route(
            "/api/v1/transcriptions/ask/stream",
            post(transcription_stream),
        )
        .route("/api/v1/requests/{request_id}/cancel", post(cancel_ask))
        .route("/api/op", get(list_operations))
        .route("/api/op/{name}", post(legacy_op))
        .route("/api/search", get(search))
        .route("/api/articles", get(articles))
        .route("/api/articles/{*slug}", get(get_article))
        .route("/api/sources", get(sources))
        .route("/api/stats", get(stats))
        .route("/api/health", get(health))
        .route("/api/index/rebuild", post(rebuild))
        .route("/api/ask", post(legacy_ask))
        .route("/api/compile", post(legacy_compile))
        .route("/api/lint", post(lint))
        .route("/api/lint/fix", post(legacy_lint_fix))
        .route("/api/ingest", post(legacy_ingest))
        .layer(DefaultBodyLimit::max(1_048_576));
    let routes = if state.surface == Surface::Web {
        routes
            .route("/api/branding", get(branding))
            .route("/api/languages", get(languages))
            .route("/api/collections", get(collections))
            .route("/api/articles/lite", get(articles_lite))
            .route("/api/aliases", get(aliases))
            .route("/api/taxonomy", get(taxonomy))
            .route("/api/entities", get(entities))
            .route("/api/xici", get(xici))
            .route("/api/tones", get(tones))
            .route("/api/refs/plugins", get(ref_plugins))
            .route("/api/sources/{*slug}", get(source_detail))
            .route("/api/export/article/{*slug}", get(export_article))
            .route("/api/export/tag/{tag}", get(export_tag))
            .route("/api/export/graph/{*slug}", get(export_graph))
            .route("/api/wiki/export", get(wiki_export))
            .route("/api/trails", get(trails).post(trail_save))
            .route("/api/trails/{id}/delete", post(trail_delete))
            .route("/api/articles/{*slug}", delete(delete_article))
            .route("/api/worker/status", get(worker_status))
            .route(
                "/api/upload",
                post(upload).layer(DefaultBodyLimit::max(32 * 1024 * 1024 + 65536)),
            )
            .route("/api/wiki/clean", post(wiki_clean))
            .route("/api/taxonomy/update", post(taxonomy_update))
            .route("/api/entities/extract", post(entities_extract))
            .route("/api/xici/generate", post(xici_generate))
    } else {
        routes
    };
    routes
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(state)
}

/// Historical lightweight search surface, sharing the same authorized core read.
pub fn search_router(state: HttpState) -> Router {
    Router::new()
        .route("/", get(crate::static_ui::search_page))
        .route("/api/search", get(search))
        .with_state(state)
}

async fn ask(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    let (stream, _lease) = state.start_ask(&headers, request)?;
    let response = stream.collect().await?;
    Ok(Json(
        json!({"schema_version":1,"request_id":response.request_id,"result":response}),
    ))
}

async fn ask_stream(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, HttpError> {
    let (stream, lease) = state.start_ask(&headers, request)?;
    let events = futures_util::stream::unfold((stream, lease), |(mut stream, lease)| async move {
        let event = stream.recv().await?;
        let name = match &event.event {
            AskEventKind::Progress { .. } => "progress",
            AskEventKind::Evidence { .. } => "evidence",
            AskEventKind::Delta { .. } => "delta",
            AskEventKind::Usage { .. } => "usage",
            AskEventKind::Done { .. } => "done",
            AskEventKind::Error { .. } => "error",
        };
        let frame = Event::default()
            .event(name)
            .id(event.sequence.to_string())
            .json_data(event)
            .expect("AskEvent contains only serializable native types");
        Some((Ok(frame), (stream, lease)))
    });
    Ok(Sse::new(events).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

async fn cancel_ask(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
) -> Result<Json<Value>, HttpError> {
    let principal = state.principal(&headers)?;
    let library_id = state.library(&headers)?;
    state.gateway.application.authorize(
        &principal,
        &Request {
            library_id: library_id.clone(),
            operation: "doctor".into(),
            args: json!({}),
        },
    )?;
    if !principal.capabilities.contains(&Capability::Model) {
        return Err(Error::Scope.into());
    }
    let active = state.active.lock().map_err(|_| Error::Busy)?;
    let request = active
        .get(&request_id)
        .filter(|r| r.actor == principal.id && r.library_id == library_id)
        .ok_or_else(|| Error::NotFound("request".into()))?;
    request.cancel.cancel();
    Ok(Json(
        json!({"schema_version":1,"request_id":request_id,"cancel_requested":true}),
    ))
}

async fn transcription_prepare(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        serde_json::to_value(state.prepare_transcription(&headers, request)?)
            .map_err(Error::from)?,
    ))
}

async fn transcription_ask(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    let prepared = state.prepare_transcription(&headers, request)?;
    let (stream, _lease) = state.start_ask(
        &headers,
        serde_json::to_value(prepared.question).map_err(Error::from)?,
    )?;
    let answer = stream.collect().await?;
    Ok(Json(
        json!({"schema_version":1,"request_id":answer.request_id,
        "result":{"transcription":prepared.transcription,"answer":answer}}),
    ))
}

async fn transcription_stream(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, HttpError> {
    let prepared = state.prepare_transcription(&headers, request)?;
    let (stream, lease) = state.start_ask(
        &headers,
        serde_json::to_value(prepared.question).map_err(Error::from)?,
    )?;
    let events = futures_util::stream::unfold(
        (stream, lease, prepared.transcription),
        |(mut stream, lease, receipt)| async move {
            let event = stream.recv().await?;
            let name = match &event.event {
                AskEventKind::Progress { .. } => "progress",
                AskEventKind::Evidence { .. } => "evidence",
                AskEventKind::Delta { .. } => "delta",
                AskEventKind::Usage { .. } => "usage",
                AskEventKind::Done { .. } => "done",
                AskEventKind::Error { .. } => "error",
            };
            let sequence = event.sequence;
            let event = llmbase_core::transcription::TranscriptionEvent {
                transcription_id: receipt.transcription_id.clone(),
                ask: event,
            };
            let mut payload = serde_json::to_value(event).expect("native event is serializable");
            if sequence == 0 {
                payload["transcription"] =
                    serde_json::to_value(&receipt).expect("native receipt is serializable");
            }
            let frame = Event::default()
                .event(name)
                .id(sequence.to_string())
                .json_data(payload)
                .expect("native event is serializable");
            Some((Ok(frame), (stream, lease, receipt)))
        },
    );
    Ok(Sse::new(events).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

async fn native_op(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Result<Json<Value>, HttpError> {
    let value = state.execute(headers, request, false).await?;
    Ok(Json(
        json!({"schema_version":1,"request_id":uuid::Uuid::new_v4(),"result":value}),
    ))
}

async fn resolve_evidence(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(args): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    let value = state
        .execute(
            headers,
            Request {
                library_id: String::new(),
                operation: "evidence".into(),
                args,
            },
            false,
        )
        .await?;
    Ok(Json(json!({"schema_version":1,"result":value})))
}

async fn legacy_op(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(args): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    let extension = state.gateway.application.has_extension(&name);
    if !extension && !operation_catalog().iter().any(|op| op["name"] == name) {
        return Err(Error::NotFound("operation".into()).into());
    }
    let value = if extension {
        state.read(headers, &name, args).await?
    } else {
        state.legacy(headers, &name, args).await?
    };
    Ok(Json(json!({"status":"ok","result":value})))
}

async fn list_operations(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    let principal = state.principal(&headers)?;
    let library_id = state.library(&headers)?;
    let mut operations: Vec<_> = operation_catalog()
        .iter()
        .filter(|op| native_tool_available(op["name"].as_str().unwrap_or_default()))
        .cloned()
        .collect();
    for descriptor in state
        .gateway
        .application
        .extension_descriptors(&principal, &library_id)?
    {
        let mut operation = serde_json::to_value(&descriptor).map_err(Error::from)?;
        operation["params"] = descriptor.input_schema;
        operations.push(operation);
    }
    Ok(Json(json!({"status":"ok","operations":operations})))
}

async fn search(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, HttpError> {
    let query = params.get("q").cloned().unwrap_or_default();
    let top_k = params
        .get("top_k")
        .map(|n| n.parse::<u64>())
        .transpose()
        .map_err(|_| Error::Invalid("top_k must be an integer".into()))?
        .unwrap_or(10);
    if state.surface == Surface::Web {
        return Ok(Json(
            state
                .read(headers, "web_search", json!({"query":query,"top_k":top_k}))
                .await?,
        ));
    }
    let value = state
        .legacy(headers, "kb_search", json!({"query":query,"top_k":top_k}))
        .await?;
    Ok(state.success(value))
}

async fn articles(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, HttpError> {
    if state.surface == Surface::Web {
        let args = article_args(params)?;
        let value = state.read(headers.clone(), "web_articles", args).await?;
        return cache_response(&headers, uri.to_string(), value);
    }
    let value = state
        .legacy(headers, "kb_list", json!({"tag":params.get("tag")}))
        .await?;
    Ok(state.success(value).into_response())
}

async fn get_article(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<Value>, HttpError> {
    if state.surface == Surface::Web {
        let (slug, sections) = slug
            .strip_suffix("/sections")
            .map(|s| (s.to_owned(), true))
            .unwrap_or((slug, false));
        return Ok(Json(
            state
                .read(
                    headers,
                    "web_article",
                    json!({"slug":slug,"sections":sections}),
                )
                .await?,
        ));
    }
    let value = state
        .execute(
            headers,
            Request {
                library_id: String::new(),
                operation: "get".into(),
                args: json!({"slug":slug}),
            },
            false,
        )
        .await?;
    let source: Source = serde_json::from_value(value).map_err(Error::from)?;
    let mut value = article(&source, false);
    value["status"] = json!("ok");
    value.as_object_mut().unwrap().remove("sources");
    Ok(Json(value))
}

async fn sources(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    let value = state.read(headers, "web_sources", json!({})).await?;
    Ok(state.success(value))
}

async fn stats(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    if state.surface == Surface::Web {
        return Ok(Json(state.read(headers, "web_stats", json!({})).await?));
    }
    let value = state.legacy(headers, "kb_stats", json!({})).await?;
    Ok(state.success(value))
}

async fn rebuild(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    let mut value = state.legacy(headers, "kb_rebuild_index", json!({})).await?;
    value["status"] = json!("ok");
    Ok(Json(value))
}

fn article_args(params: HashMap<String, String>) -> Result<Value, HttpError> {
    let mut args = json!({});
    for key in ["cursor", "tag", "q", "fields"] {
        if let Some(value) = params.get(key) {
            args[key] = json!(value);
        }
    }
    if let Some(limit) = params.get("limit") {
        args["limit"] = json!(
            limit
                .parse::<u64>()
                .map_err(|_| Error::Invalid("limit must be 1..1000".into()))?
        );
    }
    Ok(args)
}

/// Payload hashing includes the URL slice. Scope checks and validation execute
/// before conditional responses; private responses cannot enter a public cache.
fn cache_response(headers: &HeaderMap, key: String, payload: Value) -> Result<Response, HttpError> {
    let bytes = serde_json::to_vec(&payload).map_err(Error::from)?;
    let mut digest = Sha256::new();
    digest.update(key.as_bytes());
    digest.update(&bytes);
    let etag = format!("W/\"{:x}\"", digest.finalize());
    let hit = headers
        .get("if-none-match")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            h.split(',').any(|v| {
                v.trim() == "*"
                    || v.trim().trim_start_matches("W/") == etag.trim_start_matches("W/")
            })
        });
    let mut response = if hit {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        Json(payload).into_response()
    };
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&etag)
            .map_err(|_| Error::Invalid("invalid cache validator".into()))?,
    );
    response.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("private, no-cache"),
    );
    response.headers_mut().insert(
        "vary",
        HeaderValue::from_static("Authorization, X-Library-ID"),
    );
    Ok(response)
}

async fn branding(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    let value = state
        .read(headers, "web_config", json!({"field":"branding"}))
        .await?;
    Ok(Json(value["branding"].clone()))
}
async fn languages(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let value = state
        .read(headers, "web_config", json!({"field":"languages"}))
        .await
        .map_err(config_error)?;
    Ok(Json(value["languages"].clone()).into_response())
}
fn config_error(mut error: HttpError) -> HttpError {
    if error.status == StatusCode::BAD_REQUEST {
        error.status = StatusCode::INTERNAL_SERVER_ERROR;
    }
    error
}

macro_rules! plain_read {
    ($name:ident,$operation:literal) => {
        async fn $name(
            State(state): State<HttpState>,
            headers: HeaderMap,
        ) -> Result<Json<Value>, HttpError> {
            Ok(Json(state.read(headers, $operation, json!({})).await?))
        }
    };
}
plain_read!(collections, "web_collections");
plain_read!(aliases, "web_aliases");
plain_read!(entities, "web_entities");
plain_read!(wiki_export, "web_wiki_export");
plain_read!(trails, "web_trails");
plain_read!(worker_status, "web_worker_status");

async fn health(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    let mut value = state.read(headers, "web_health", json!({})).await?;
    if state.surface == Surface::Agent && value["report"].is_null() {
        value["message"] = json!("No health check has run yet");
    }
    Ok(state.success(value))
}
async fn articles_lite(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, HttpError> {
    let mut args = article_args(params)?;
    args["lite"] = json!(true);
    let value = state.read(headers.clone(), "web_articles", args).await?;
    cache_response(&headers, uri.to_string(), value)
}

async fn language_read(
    state: HttpState,
    headers: HeaderMap,
    params: HashMap<String, String>,
    uri: String,
    operation: &str,
    cache: bool,
) -> Result<Response, HttpError> {
    let config = state
        .read(headers.clone(), "web_config", json!({"field":"languages"}))
        .await
        .map_err(config_error)?;
    let profile = &config["languages"];
    let lang = params
        .get("lang")
        .filter(|s| !s.is_empty())
        .map(String::as_str)
        .unwrap_or(profile["api_default_lang"].as_str().unwrap_or("zh"));
    if !profile["codes"]
        .as_array()
        .is_some_and(|codes| codes.iter().any(|v| v == lang))
    {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid lang","valid":profile["codes"]})),
        )
            .into_response());
    }
    let value = state
        .read(headers.clone(), operation, json!({"lang":lang}))
        .await?;
    if cache {
        cache_response(&headers, uri, value)
    } else {
        Ok(Json(value).into_response())
    }
}
async fn taxonomy(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, HttpError> {
    language_read(
        state,
        headers,
        params,
        uri.to_string(),
        "web_taxonomy",
        true,
    )
    .await
}
async fn xici(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, HttpError> {
    language_read(state, headers, params, uri.to_string(), "web_xici", false).await
}

async fn source_detail(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state
            .read(headers, "web_source", json!({"slug":slug}))
            .await?,
    ))
}
async fn export_article(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state
            .read(headers, "web_export_article", json!({"slug":slug}))
            .await?,
    ))
}
async fn export_tag(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(tag): Path<String>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state
            .read(headers, "web_export_tag", json!({"tag":tag}))
            .await?,
    ))
}
async fn export_graph(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, HttpError> {
    let depth = params
        .get("depth")
        .map(|v| v.parse::<u64>())
        .transpose()
        .map_err(|_| Error::Invalid("depth must be an integer 0..4".into()))?
        .unwrap_or(2);
    Ok(Json(
        state
            .read(
                headers,
                "web_export_graph",
                json!({"slug":slug,"depth":depth}),
            )
            .await?,
    ))
}
async fn lint(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(args): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    validate_body(&args)?;
    if bool_field(&args, "deep", false)? {
        let profile = state
            .read(headers.clone(), "web_config", json!({"field":"languages"}))
            .await?;
        let value = state
            .read(
                headers,
                "maintenance_repair",
                json!({"mode":"deep_lint","profile":profile["languages"]}),
            )
            .await?;
        let report = value
            .pointer("/stages/0/value/report")
            .cloned()
            .ok_or_else(|| Error::Invalid("deep lint returned no report".into()))?;
        return Ok(state.success(json!({"report":report})));
    }
    let value=state.read(headers,"web_lint",json!({"allow_cjk_slugs":args.get("allow_cjk_slugs").and_then(Value::as_bool).unwrap_or(false)})).await?;
    Ok(state.success(json!({"results":value})))
}
fn validate_body(value: &Value) -> Result<(), HttpError> {
    if !value.is_object() {
        return Err(Error::Invalid("request must be a JSON object".into()).into());
    }
    if contains_credential(value) {
        return Err(Error::Invalid(
            "credentials are not accepted in request bodies; use X-LLM-Key".into(),
        )
        .into());
    }
    Ok(())
}
fn bool_field(value: &Value, key: &str, default: bool) -> Result<bool, HttpError> {
    value
        .get(key)
        .map(|v| {
            v.as_bool()
                .ok_or_else(|| HttpError::from(Error::Invalid(format!("{key} must be a boolean"))))
        })
        .unwrap_or(Ok(default))
}
async fn legacy_ask(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    validate_body(&body)?;
    let question = body
        .get("question")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("question must be a string".into()))?;
    let deep = bool_field(&body, "deep", false)?;
    let file_back = bool_field(&body, "file_back", true)?;
    let promote = bool_field(&body, "promote", false)?;
    let tone = body
        .get("tone")
        .map(|v| {
            v.as_str()
                .ok_or_else(|| Error::Invalid("tone must be a string".into()))
        })
        .transpose()?
        .unwrap_or("default");
    let mut args = json!({"question":question,"deep":deep,"file_back":file_back,"promote":promote,"tone":tone});
    if let Some(model) = body.get("model") {
        args["model"] = model.clone();
    }
    let value = if state.surface == Surface::Web && !deep {
        args.as_object_mut().unwrap().remove("deep");
        args["mode"] = json!("basic");
        args["return_path"] = json!(true);
        state.read(headers, "legacy_query", args).await?
    } else {
        state.legacy(headers, "kb_ask", args).await?
    };
    if state.surface == Surface::Agent {
        return Ok(state.success(value));
    }
    let mut payload = json!({"answer":value["answer"]});
    if deep {
        payload["consulted"] = value.get("consulted").cloned().unwrap_or(json!([]));
    }
    for key in ["output_path", "promotion"] {
        if let Some(value) = value.get(key).filter(|v| !v.is_null()) {
            payload[key] = value.clone();
        }
    }
    Ok(Json(payload))
}
async fn legacy_compile(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, HttpError> {
    let body = body.map(|b| b.0).unwrap_or(json!({}));
    validate_body(&body)?;
    let full = if state.surface == Surface::Agent {
        bool_field(&body, "full", false)?
    } else {
        false
    };
    let value = state
        .legacy(headers, "kb_compile", json!({"full":full}))
        .await?;
    if state.surface == Surface::Web {
        Ok(Json(
            json!({"status":"ok","articles_created":value["articles_created"]}),
        ))
    } else {
        Ok(state.success(value))
    }
}
async fn tones(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(state.read(headers, "web_tones", json!({})).await?))
}
async fn entities_extract(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, HttpError> {
    validate_body(&body.map(|b| b.0).unwrap_or(json!({})))?;
    let value = state.read(headers, "entities_generate", json!({})).await?;
    Ok(Json(value.get("result").cloned().ok_or_else(|| {
        Error::Invalid("entity generation returned no result".into())
    })?))
}
async fn xici_generate(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, HttpError> {
    validate_body(&body)?;
    let config = state
        .read(headers.clone(), "web_config", json!({"field":"languages"}))
        .await
        .map_err(config_error)?;
    let profile = &config["languages"];
    let requested = body
        .get("lang")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_str()
                .ok_or_else(|| Error::Invalid("lang must be a string".into()))
        })
        .transpose()?;
    let lang = requested
        .filter(|s| !s.is_empty())
        .unwrap_or(profile["api_default_lang"].as_str().unwrap_or("zh"));
    if !profile["codes"]
        .as_array()
        .is_some_and(|codes| codes.iter().any(|v| v == lang))
    {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid lang","valid":profile["codes"]})),
        )
            .into_response());
    }
    let value = state
        .read(
            headers,
            "xici_generate",
            json!({"lang":lang,"strategy":"legacy_zh_base"}),
        )
        .await?;
    Ok(Json(
        value
            .get("result")
            .cloned()
            .ok_or_else(|| Error::Invalid("introduction generation returned no result".into()))?,
    )
    .into_response())
}
async fn legacy_lint_fix(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, HttpError> {
    validate_body(&body.map(|b| b.0).unwrap_or(json!({})))?;
    let profile = state
        .read(headers.clone(), "web_config", json!({"field":"languages"}))
        .await?;
    let args = json!({"mode":if state.surface==Surface::Web{"heal"}else{"auto_fix"},"persist_last_fix":state.surface==Surface::Web,"allow_cjk_slugs":false,"confirm_duplicates":false,"profile":profile["languages"],"taxonomy":{"label_languages":["en","zh","ja"],"sync_category_tags":true}});
    if state.surface == Surface::Agent {
        let value = state.read(headers, "maintenance_repair", args).await?;
        let mut payload =
            json!({"status":"ok","fixes":value["fixes"],"fix_count":value["fix_count"]});
        if value["status"] == "partial" {
            payload["status"] = json!("partial");
            payload["failures"] = value["failures"].clone();
        }
        return Ok(Json(payload));
    }
    let principal = state.principal(&headers)?;
    let library_id = state.library(&headers)?;
    state.gateway.application.authorize(
        &principal,
        &Request {
            library_id: library_id.clone(),
            operation: "job_run_one".into(),
            args: json!({}),
        },
    )?;
    let permit = state
        .concurrency
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::Busy)?;
    let job = state
        .read(
            headers,
            "job_enqueue",
            json!({"operation":"maintenance_repair","args":args}),
        )
        .await?;
    let id = job
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("queue returned no job ID".into()))?
        .to_owned();
    let gateway = state.gateway.clone();
    let job_id = id.clone();
    tokio::spawn(async move {
        let _permit = permit;
        // A simultaneous status read may briefly own the recovery writer lock.
        // Retry only an unclaimed Busy job, always retaining the exact job ID.
        for attempt in 0..100 {
            let result = gateway
                .execute(
                    principal.clone(),
                    Request {
                        library_id: library_id.clone(),
                        operation: "job_run_one".into(),
                        args: json!({"job_id":job_id,"worker_id":"http-lint-fix"}),
                    },
                )
                .await;
            if matches!(result, Err(Error::Busy)) && attempt < 99 {
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            break;
        }
    });
    Ok(Json(
        json!({"status":"started","message":"Auto-fix pipeline running in background. Check /api/health for results.","job_id":id}),
    ))
}
async fn ref_plugins(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state.read(headers, "web_refs_plugins", json!({})).await?,
    ))
}

async fn legacy_ingest(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, HttpError> {
    let body = body.map(|v| v.0).unwrap_or(json!({}));
    validate_body(&body)?;
    let source = body
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("source must be a string".into()))?;
    let mut result = state
        .legacy(headers, "kb_ingest", json!({"source":source}))
        .await?;
    result["status"] = json!("ok");
    Ok(Json(result))
}

async fn upload(
    State(state): State<HttpState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, HttpError> {
    let principal = state.principal(&headers)?;
    let library_id = state.library(&headers)?;
    state.gateway.application.authorize(
        &principal,
        &Request {
            library_id,
            operation: "ingest_text".into(),
            args: json!({}),
        },
    )?;
    let mut uploaded = None;
    while let Some(mut field) = multipart.next_field().await.map_err(|e| HttpError {
        status: e.status(), payload: json!({"code":"invalid_multipart","message":"invalid or oversized multipart upload"}),
    })? {
        if field.name() != Some("file") { continue; }
        if uploaded.is_some() {
            return Err(Error::Invalid("upload accepts exactly one file".into()).into());
        }
        let filename = field.file_name().unwrap_or("").to_owned();
        if filename.is_empty() {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"status":"error","message":"Empty filename"}))).into_response());
        }
        if filename.len() > 255 || filename.contains(['/', '\\', '\0']) || filename.chars().any(char::is_control) {
            return Err(Error::Invalid("upload requires a bounded single filename".into()).into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = field.chunk().await.map_err(|e| HttpError {
            status: e.status(), payload: json!({"code":"invalid_multipart","message":"invalid or oversized multipart upload"}),
        })? {
            if bytes.len().saturating_add(chunk.len()) > 32 * 1024 * 1024 {
                return Ok((StatusCode::PAYLOAD_TOO_LARGE, Json(json!({"status":"error","message":"File exceeds 32 MiB"}))).into_response());
            }
            bytes.extend_from_slice(&chunk);
        }
        uploaded = Some((filename, bytes));
    }
    let Some((filename, bytes)) = uploaded else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","message":"No file provided"})),
        )
            .into_response());
    };
    if filename.to_ascii_lowercase().ends_with(".pdf") {
        let result = state.read(headers, "plugin_ingest", json!({
            "adapter":"pdf", "input":{"filename":filename,"content_bytes":bytes,"chunk_pages":20}
        })).await?;
        let accepted = result
            .get("accepted")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .ok_or_else(|| Error::Conflict("PDF adapter accepted no source".into()))?;
        Ok(
            Json(json!({"status":"ok","chunks":accepted.len(),"filename":filename}))
                .into_response(),
        )
    } else {
        let content = String::from_utf8(bytes).map_err(|_| {
            Error::Invalid(
                "text upload must be valid UTF-8; binary formats require a parser adapter".into(),
            )
        })?;
        let result = state
            .read(
                headers,
                "ingest_text",
                json!({"filename":filename,"content":content}),
            )
            .await?;
        Ok(Json(json!({"status":"ok","path":result["path"],"filename":filename})).into_response())
    }
}

async fn delete_article(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state
            .read(headers, "web_write_article_delete", json!({"slug":slug}))
            .await?,
    ))
}
async fn wiki_clean(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state
            .read(headers, "web_write_wiki_clean", json!({}))
            .await?,
    ))
}
async fn taxonomy_update(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(document): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state
            .read(
                headers,
                "web_write_taxonomy_update",
                json!({"document":document}),
            )
            .await?,
    ))
}
async fn trail_save(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(args): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state.read(headers, "web_write_trail_save", args).await?,
    ))
}
async fn trail_delete(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, HttpError> {
    Ok(Json(
        state
            .read(headers, "web_write_trail_delete", json!({"trail_id":id}))
            .await?,
    ))
}

pub async fn serve(state: HttpState, address: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!("llmbase HTTP listening on {}", listener.local_addr()?);
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
