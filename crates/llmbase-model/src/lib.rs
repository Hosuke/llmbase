//! Native HTTP-compatible text generation and embeddings.
//!
//! Configuration is supplied by the application, not read from disk. HTTP
//! fixtures exercise the same client as production. Dropping a stream aborts its
//! request; a partial stream is never retried, replayed, or reported as success.

mod sse;
mod types;

pub use tokio_util::sync::CancellationToken;
pub use types::*;

use std::{
    fmt,
    future::Future,
    time::{Duration, SystemTime},
};

use futures_util::StreamExt;
use reqwest::{
    Response, Url,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Clone)]
pub struct Client {
    config: ModelConfig,
    http: reqwest::Client,
    base_url: Url,
    authorization: Option<HeaderValue>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

pub struct ChatStream {
    receiver: mpsc::Receiver<Result<StreamEvent, ModelError>>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl ChatStream {
    pub async fn recv(&mut self) -> Option<Result<StreamEvent, ModelError>> {
        self.receiver.recv().await
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for ChatStream {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

impl Client {
    /// Canonical, credential-free operator endpoint identity for index fingerprints.
    pub fn endpoint_identity(&self) -> &str {
        self.base_url.as_str().trim_end_matches('/')
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Per-call identity/credential override on the same operator-controlled endpoint.
    /// Model overrides cannot fall through to an operator's other configured models.
    pub fn scoped_overrides(
        &self,
        model: Option<String>,
        api_key: Option<SecretKey>,
    ) -> Result<Self, ModelError> {
        let mut config = self.config.clone();
        if let Some(model) = model {
            config.model = model;
            config.fallback_models.clear();
        }
        if let Some(api_key) = api_key {
            config.api_key = Some(api_key);
        }
        Self::new(config)
    }

    pub fn new(config: ModelConfig) -> Result<Self, ModelError> {
        validate_config(&config)?;
        let mut base_url = Url::parse(&config.base_url)
            .map_err(|_| ModelError::InvalidConfig("base_url must be an absolute HTTP URL"))?;
        if !matches!(base_url.scheme(), "https" | "http")
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(ModelError::InvalidConfig(
                "base_url must be HTTP(S) without credentials, query, or fragment",
            ));
        }
        let path = format!("{}/", base_url.path().trim_end_matches('/'));
        base_url.set_path(&path);
        let authorization = config
            .api_key
            .as_ref()
            .map(|key| {
                let mut header =
                    HeaderValue::from_str(&format!("Bearer {}", key.0)).map_err(|_| {
                        ModelError::InvalidConfig("API key is not a valid header value")
                    })?;
                header.set_sensitive(true);
                Ok(header)
            })
            .transpose()?;
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| ModelError::InvalidConfig("HTTP client initialization failed"))?;
        Ok(Self {
            config,
            http,
            base_url,
            authorization,
        })
    }

    /// Retries transient transport/status failures and empty responses. An
    /// exhausted empty response is returned with its final metadata, as in the
    /// legacy primitive; application compilation must reject empty content.
    pub async fn complete(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ChatResponse, ModelError> {
        self.validate_chat(&request)?;
        guarded(
            &cancel,
            self.config.request_timeout,
            self.complete_inner(&request),
        )
        .await
    }

    pub async fn embed(
        &self,
        request: EmbeddingRequest,
        cancel: CancellationToken,
    ) -> Result<EmbeddingResponse, ModelError> {
        if request.input.is_empty()
            || request.input.iter().any(|s| s.is_empty())
            || request.dimensions == Some(0)
        {
            return Err(ModelError::InvalidRequest(
                "embedding input and dimensions must be nonempty",
            ));
        }
        check_input_bytes(
            request.input.iter().map(String::len),
            self.config.max_request_bytes,
        )?;
        let model = request.model.as_deref().unwrap_or(&self.config.model);
        validate_model(model)?;
        let mut body =
            json!({ "model": model, "input": request.input, "encoding_format": "float" });
        if let Some(dimensions) = request.dimensions {
            body["dimensions"] = json!(dimensions);
        }
        let body = self.encode_body(&body)?;
        guarded(&cancel, self.config.request_timeout, async {
            let mut attempt = 0;
            loop {
                attempt += 1;
                match self.post("embeddings", &body, attempt).await {
                    Ok(response) => {
                        let raw: EmbeddingEnvelope = self.read_json(response).await?;
                        return parse_embeddings(
                            raw,
                            request.input.len(),
                            request.dimensions,
                            model,
                            attempt,
                        );
                    }
                    Err(failure)
                        if failure.error.retryable()
                            && attempt < self.config.retry.max_attempts =>
                    {
                        self.backoff(attempt, failure.retry_after).await;
                    }
                    Err(failure) => return Err(failure.error),
                }
            }
        })
        .await
    }

    /// Starts one supervised task on the caller's Tokio runtime. The bounded
    /// receiver applies backpressure; its Drop aborts all network work.
    pub fn stream(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ChatStream, ModelError> {
        self.validate_chat(&request)?;
        if !self.config.supports_streaming {
            return Err(ModelError::StreamingUnsupported);
        }
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| ModelError::InvalidConfig("stream requires a Tokio runtime"))?;
        let cancel = cancel.child_token();
        let task_cancel = cancel.clone();
        let client = self.clone();
        let (sender, receiver) = mpsc::channel(self.config.stream_buffer);
        let task = runtime.spawn(async move {
            let result = guarded(
                &task_cancel,
                client.config.request_timeout,
                client.stream_inner(&request, &sender),
            )
            .await;
            if let Err(error) = result {
                // Preserve buffered deltas followed by one terminal error.
                // Receiver Drop aborts this send even if its buffer is full.
                let _ = sender.send(Err(error)).await;
            }
        });
        Ok(ChatStream {
            receiver,
            cancel,
            task,
        })
    }

    fn validate_chat(&self, request: &ChatRequest) -> Result<(), ModelError> {
        if request.messages.is_empty() || request.max_tokens == 0 {
            return Err(ModelError::InvalidRequest(
                "messages and max_tokens must be nonempty",
            ));
        }
        if request
            .temperature
            .is_some_and(|t| !t.is_finite() || !(0.0..=2.0).contains(&t))
        {
            return Err(ModelError::InvalidRequest(
                "temperature must be between zero and two",
            ));
        }
        check_input_bytes(
            request.messages.iter().map(|m| m.content.len()),
            self.config.max_request_bytes,
        )?;
        validate_model(request.model.as_deref().unwrap_or(&self.config.model))
    }

    fn chat_body(
        &self,
        request: &ChatRequest,
        model: &str,
        stream: bool,
    ) -> Result<Vec<u8>, ModelError> {
        let mut body = json!({ "model": model, "messages": request.messages, "max_tokens": request.max_tokens, "stream": stream });
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if stream && self.config.stream_include_usage {
            body["stream_options"] = json!({"include_usage": true});
        }
        self.encode_body(&body)
    }

    fn encode_body(&self, body: &Value) -> Result<Vec<u8>, ModelError> {
        let bytes = serde_json::to_vec(body)
            .map_err(|_| ModelError::InvalidRequest("request is not valid JSON"))?;
        if bytes.len() > self.config.max_request_bytes {
            return Err(ModelError::InvalidRequest(
                "request exceeded configured byte limit",
            ));
        }
        Ok(bytes)
    }

    fn models<'a>(&'a self, request: &'a ChatRequest) -> impl Iterator<Item = &'a str> {
        std::iter::once(request.model.as_deref().unwrap_or(&self.config.model))
            .chain(self.config.fallback_models.iter().map(String::as_str))
    }

    async fn complete_inner(&self, request: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let mut attempts = 0;
        let mut last = Err(ModelError::Protocol("no response received"));
        for (index, model) in self.models(request).enumerate() {
            let limit = if index == 0 {
                self.config.retry.max_attempts
            } else {
                self.config.retry.fallback_attempts
            };
            let body = self.chat_body(request, model, false)?;
            for attempt in 1..=limit {
                attempts += 1;
                let (result, retry_after) =
                    match self.post("chat/completions", &body, attempts).await {
                        Ok(response) => {
                            let result =
                                self.read_json::<ChatEnvelope>(response)
                                    .await
                                    .and_then(|raw| {
                                        parse_complete(
                                            raw,
                                            model,
                                            attempts,
                                            self.config.reasoning_content_fallback,
                                        )
                                    });
                            (result, None)
                        }
                        Err(failure) => (Err(failure.error), failure.retry_after),
                    };
                let retryable = match &result {
                    Ok(response) if !response.content.is_empty() => return result,
                    Ok(_) => true,
                    Err(ModelError::Timeout | ModelError::Cancelled) => return result,
                    Err(error) => error.retryable(),
                };
                last = result;
                if !retryable {
                    break;
                }
                if attempt < limit {
                    self.backoff(attempt, retry_after).await;
                }
            }
        }
        last
    }

    async fn stream_inner(
        &self,
        request: &ChatRequest,
        sender: &mpsc::Sender<Result<StreamEvent, ModelError>>,
    ) -> Result<(), ModelError> {
        let mut attempts = 0;
        let mut last = ModelError::Protocol("no response received");
        for (index, model) in self.models(request).enumerate() {
            let limit = if index == 0 {
                self.config.retry.max_attempts
            } else {
                self.config.retry.fallback_attempts
            };
            let body = self.chat_body(request, model, true)?;
            for attempt in 1..=limit {
                attempts += 1;
                match self.post("chat/completions", &body, attempts).await {
                    Ok(response) => {
                        let is_sse = response
                            .headers()
                            .get(CONTENT_TYPE)
                            .and_then(|h| h.to_str().ok())
                            .and_then(|s| s.split(';').next())
                            .is_some_and(|s| s.trim().eq_ignore_ascii_case("text/event-stream"));
                        if !is_sse {
                            return Err(ModelError::StreamingUnsupported);
                        }
                        // No replay after headers accepted, even before first delta.
                        return self.consume_stream(response, model, attempts, sender).await;
                    }
                    Err(failure) => {
                        let retryable = failure.error.retryable();
                        if matches!(failure.error, ModelError::Timeout | ModelError::Cancelled) {
                            return Err(failure.error);
                        }
                        last = failure.error;
                        if !retryable {
                            break;
                        }
                        if attempt < limit {
                            self.backoff(attempt, failure.retry_after).await;
                        }
                    }
                }
            }
        }
        Err(last)
    }

    async fn consume_stream(
        &self,
        response: Response,
        model: &str,
        attempts: u32,
        sender: &mpsc::Sender<Result<StreamEvent, ModelError>>,
    ) -> Result<(), ModelError> {
        let mut bytes = response.bytes_stream();
        let mut decoder = sse::Decoder::new(self.config.max_event_bytes);
        let mut total = 0usize;
        let mut result = ChatResponse {
            content: String::new(),
            model: model.to_owned(),
            finish_reason: None,
            truncated: false,
            usage: Usage::default(),
            attempts,
        };
        let mut reasoning = String::new();
        let mut saw_choice = false;
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(transport_error)?;
            total = total
                .checked_add(chunk.len())
                .ok_or(ModelError::ResponseTooLarge)?;
            if total > self.config.max_response_bytes {
                return Err(ModelError::ResponseTooLarge);
            }
            for event in decoder.push(&chunk)? {
                if event.trim() == "[DONE]" {
                    if !saw_choice {
                        return Err(ModelError::Protocol("stream has no choice"));
                    }
                    if self.config.reasoning_content_fallback
                        && result.content.trim().is_empty()
                        && !reasoning.is_empty()
                    {
                        result.content = reasoning;
                    }
                    result.truncated = result.finish_reason.as_deref() == Some("length");
                    emit(sender, StreamEvent::Done { response: result }).await?;
                    return Ok(());
                }
                let raw: ChatEnvelope = serde_json::from_str(&event)
                    .map_err(|_| ModelError::Protocol("invalid SSE JSON"))?;
                if raw.error.is_some() {
                    return Err(ModelError::Protocol("provider stream error"));
                }
                if let Some(model) = raw.model {
                    result.model = model;
                }
                if let Some(usage) = raw.usage {
                    result.usage = parse_usage(usage);
                    emit(
                        sender,
                        StreamEvent::Usage {
                            usage: result.usage.clone(),
                        },
                    )
                    .await?;
                }
                for choice in raw.choices {
                    if choice.index != 0 {
                        continue;
                    }
                    saw_choice = true;
                    let delta = choice
                        .delta
                        .ok_or(ModelError::Protocol("stream choice has no delta"))?;
                    if let Some(content) = delta.content.filter(|c| !c.is_empty()) {
                        if result.finish_reason.is_some() {
                            return Err(ModelError::Protocol(
                                "content arrived after finish reason",
                            ));
                        }
                        result.content.push_str(&content);
                        emit(sender, StreamEvent::Delta { content }).await?;
                    }
                    if let Some(content) = delta.reasoning_content.filter(|c| !c.is_empty()) {
                        reasoning.push_str(&content);
                        emit(sender, StreamEvent::ReasoningDelta { content }).await?;
                    }
                    if let Some(finish) = choice.finish_reason {
                        result.finish_reason = Some(finish);
                    }
                }
            }
        }
        Err(ModelError::StreamInterrupted)
    }

    async fn post(&self, endpoint: &str, body: &[u8], attempts: u32) -> Result<Response, Failure> {
        let url = self
            .base_url
            .join(endpoint)
            .map_err(|_| Failure::from(ModelError::InvalidConfig("invalid endpoint")))?;
        let mut request = self
            .http
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_vec());
        if let Some(authorization) = &self.authorization {
            request = request.header(AUTHORIZATION, authorization.clone());
        }
        let response = request
            .send()
            .await
            .map_err(|e| Failure::from(transport_error(e)))?;
        if !response.status().is_success() {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|h| h.to_str().ok())
                .and_then(parse_retry_after);
            return Err(Failure {
                error: ModelError::Http {
                    status: response.status().as_u16(),
                    attempts,
                },
                retry_after,
            });
        }
        if response
            .content_length()
            .is_some_and(|n| n > self.config.max_response_bytes as u64)
        {
            return Err(ModelError::ResponseTooLarge.into());
        }
        Ok(response)
    }

    async fn read_json<T: for<'de> Deserialize<'de>>(
        &self,
        response: Response,
    ) -> Result<T, ModelError> {
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(transport_error)?;
            if chunk.len() > self.config.max_response_bytes.saturating_sub(body.len()) {
                return Err(ModelError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map_err(|_| ModelError::Protocol("invalid JSON or response schema"))
    }

    async fn backoff(&self, attempt: u32, retry_after: Option<Duration>) {
        let multiplier = 1u32
            .checked_shl(attempt.saturating_sub(1))
            .unwrap_or(u32::MAX);
        let exponential = self
            .config
            .retry
            .initial_backoff
            .saturating_mul(multiplier)
            .min(self.config.retry.max_backoff);
        let delay = retry_after
            .unwrap_or(exponential)
            .min(self.config.retry.max_backoff);
        tokio::time::sleep(delay).await;
    }
}

async fn guarded<T>(
    cancel: &CancellationToken,
    timeout: Duration,
    operation: impl Future<Output = Result<T, ModelError>>,
) -> Result<T, ModelError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(ModelError::Cancelled),
        result = tokio::time::timeout(timeout, operation) => result.unwrap_or(Err(ModelError::Timeout)),
    }
}

async fn emit(
    sender: &mpsc::Sender<Result<StreamEvent, ModelError>>,
    event: StreamEvent,
) -> Result<(), ModelError> {
    sender
        .send(Ok(event))
        .await
        .map_err(|_| ModelError::Cancelled)
}

fn transport_error(error: reqwest::Error) -> ModelError {
    if error.is_timeout() {
        ModelError::Timeout
    } else {
        ModelError::Transport
    }
}

struct Failure {
    error: ModelError,
    retry_after: Option<Duration>,
}

impl From<ModelError> for Failure {
    fn from(error: ModelError) -> Self {
        Self {
            error,
            retry_after: None,
        }
    }
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    httpdate::parse_http_date(value)
        .ok()
        .map(|date| date.duration_since(SystemTime::now()).unwrap_or_default())
}

fn validate_config(config: &ModelConfig) -> Result<(), ModelError> {
    if config.model.trim().is_empty() || config.fallback_models.iter().any(|m| m.trim().is_empty())
    {
        return Err(ModelError::InvalidConfig("model names must be nonempty"));
    }
    if config.connect_timeout.is_zero()
        || config.request_timeout.is_zero()
        || config.read_timeout.is_zero()
    {
        return Err(ModelError::InvalidConfig("timeouts must be positive"));
    }
    if config.max_request_bytes == 0
        || config.max_response_bytes == 0
        || config.max_event_bytes == 0
        || !(1..=1024).contains(&config.stream_buffer)
    {
        return Err(ModelError::InvalidConfig(
            "byte limits and stream buffer must be positive; buffer at most 1024",
        ));
    }
    if !(1..=32).contains(&config.retry.max_attempts)
        || !(1..=32).contains(&config.retry.fallback_attempts)
        || config.fallback_models.len() > 32
    {
        return Err(ModelError::InvalidConfig(
            "attempts and fallback models must be bounded by 32",
        ));
    }
    Ok(())
}

fn validate_model(model: &str) -> Result<(), ModelError> {
    if model.trim().is_empty() || model.len() > 1024 {
        return Err(ModelError::InvalidRequest(
            "model name is empty or too long",
        ));
    }
    Ok(())
}

fn check_input_bytes(lengths: impl Iterator<Item = usize>, limit: usize) -> Result<(), ModelError> {
    let mut total = 0usize;
    for length in lengths {
        // Include overhead so unbounded empty-message/input arrays are rejected.
        total = total
            .checked_add(length)
            .and_then(|n| n.checked_add(64))
            .ok_or(ModelError::InvalidRequest(
                "request exceeded configured byte limit",
            ))?;
        if total > limit {
            return Err(ModelError::InvalidRequest(
                "request exceeded configured byte limit",
            ));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct ChatEnvelope {
    #[serde(default)]
    choices: Vec<Choice>,
    model: Option<String>,
    usage: Option<ProviderUsage>,
    error: Option<Value>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    index: usize,
    message: Option<TextFields>,
    delta: Option<TextFields>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct TextFields {
    content: Option<String>,
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct ProviderUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    completion_tokens_details: Option<CompletionDetails>,
}

#[derive(Deserialize)]
struct CompletionDetails {
    reasoning_tokens: Option<u64>,
}

fn parse_usage(usage: ProviderUsage) -> Usage {
    Usage {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        reasoning_tokens: usage
            .completion_tokens_details
            .and_then(|d| d.reasoning_tokens),
    }
}

fn parse_complete(
    raw: ChatEnvelope,
    model: &str,
    attempts: u32,
    reasoning_fallback: bool,
) -> Result<ChatResponse, ModelError> {
    if raw.error.is_some() {
        return Err(ModelError::Protocol("provider returned an error"));
    }
    let choice = raw
        .choices
        .into_iter()
        .find(|choice| choice.index == 0)
        .ok_or(ModelError::Protocol("response has no choice zero"))?;
    let message = choice
        .message
        .ok_or(ModelError::Protocol("response has no message"))?;
    let mut content = message.content.unwrap_or_default();
    if reasoning_fallback
        && content.trim().is_empty()
        && let Some(reasoning) = message.reasoning_content.filter(|s| !s.is_empty())
    {
        content = reasoning;
    }
    Ok(ChatResponse {
        content,
        model: raw.model.unwrap_or_else(|| model.to_owned()),
        truncated: choice.finish_reason.as_deref() == Some("length"),
        finish_reason: choice.finish_reason,
        usage: raw.usage.map(parse_usage).unwrap_or_default(),
        attempts,
    })
}

#[derive(Deserialize)]
struct EmbeddingEnvelope {
    data: Vec<EmbeddingItem>,
    model: Option<String>,
    usage: Option<ProviderUsage>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

fn parse_embeddings(
    raw: EmbeddingEnvelope,
    count: usize,
    expected_dimensions: Option<usize>,
    model: &str,
    attempts: u32,
) -> Result<EmbeddingResponse, ModelError> {
    if raw.data.len() != count {
        return Err(ModelError::Protocol("embedding count differs from input"));
    }
    let dimensions = raw.data.first().map(|v| v.embedding.len()).unwrap_or(0);
    if dimensions == 0 || expected_dimensions.is_some_and(|d| d != dimensions) {
        return Err(ModelError::Protocol(
            "embedding dimensions differ from request",
        ));
    }
    let mut ordered = vec![None; count];
    for item in raw.data {
        if item.index >= count
            || ordered[item.index].is_some()
            || item.embedding.len() != dimensions
            || item.embedding.iter().any(|n| !n.is_finite())
        {
            return Err(ModelError::Protocol(
                "invalid embedding indices, dimensions, or values",
            ));
        }
        ordered[item.index] = Some(item.embedding);
    }
    let embeddings = ordered
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ModelError::Protocol("missing embedding index"))?;
    Ok(EmbeddingResponse {
        embeddings,
        model: raw.model.unwrap_or_else(|| model.to_owned()),
        dimensions,
        usage: raw.usage.map(parse_usage).unwrap_or_default(),
        attempts,
    })
}
