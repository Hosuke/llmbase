use std::{fmt, time::Duration};

use serde::{Deserialize, Serialize};

/// Kept in memory only. Deliberately does not implement Serialize or Display.
#[derive(Clone)]
pub struct SecretKey(pub(crate) String);

impl SecretKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey([redacted])")
    }
}

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Total attempts, including the initial request; never zero.
    pub max_attempts: u32,
    pub fallback_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            fallback_attempts: 1,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
        }
    }
}

#[derive(Clone)]
pub struct ModelConfig {
    /// Operator controlled API prefix, e.g. https://models.example/v1.
    pub base_url: String,
    pub model: String,
    pub api_key: Option<SecretKey>,
    pub fallback_models: Vec<String>,
    pub connect_timeout: Duration,
    /// One wall-clock budget for all attempts, backoff, and response consumption.
    pub request_timeout: Duration,
    /// Maximum silence between provider network chunks.
    pub read_timeout: Duration,
    pub retry: RetryPolicy,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_event_bytes: usize,
    pub stream_buffer: usize,
    pub supports_streaming: bool,
    pub stream_include_usage: bool,
    /// Legacy llmwiki behavior; native callers may disable it.
    pub reasoning_content_fallback: bool,
}

impl ModelConfig {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            api_key: None,
            fallback_models: Vec::new(),
            connect_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(300),
            read_timeout: Duration::from_secs(60),
            retry: RetryPolicy::default(),
            max_request_bytes: 8 * 1024 * 1024,
            max_response_bytes: 16 * 1024 * 1024,
            max_event_bytes: 1024 * 1024,
            stream_buffer: 16,
            supports_streaming: true,
            stream_include_usage: true,
            reasoning_content_fallback: true,
        }
    }
}

impl fmt::Debug for ModelConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Even invalid pre-validation URLs/models can contain credentials.
        f.debug_struct("ModelConfig")
            .field("endpoint", &"[configured]")
            .field("api_key", &self.api_key)
            .field("request_timeout", &self.request_timeout)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub model: Option<String>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
}

impl ChatRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            messages: vec![Message {
                role: Role::User,
                content: prompt.into(),
            }],
            model: None,
            max_tokens: 16384,
            temperature: None,
        }
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.messages.insert(
            0,
            Message {
                role: Role::System,
                content: system.into(),
            },
        );
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub finish_reason: Option<String>,
    pub truncated: bool,
    pub usage: Usage,
    /// Actual HTTP attempts, including retries and explicit fallback models.
    pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    Delta {
        content: String,
    },
    /// Kept distinct: a reasoning delta must not be mistaken for answer text.
    ReasoningDelta {
        content: String,
    },
    Usage {
        usage: Usage,
    },
    Done {
        response: ChatResponse,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub input: Vec<String>,
    pub model: Option<String>,
    pub dimensions: Option<usize>,
}

impl EmbeddingRequest {
    pub fn new(input: Vec<String>) -> Self {
        Self {
            input,
            model: None,
            dimensions: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// Ordered like request.input, regardless of provider response ordering.
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub dimensions: usize,
    pub usage: Usage,
    pub attempts: u32,
}

/// No variant includes provider body, credentials, request text, or endpoint.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    #[error("invalid model configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("invalid model request: {0}")]
    InvalidRequest(&'static str),
    #[error("model operation cancelled")]
    Cancelled,
    #[error("model operation timed out")]
    Timeout,
    #[error("model transport failed")]
    Transport,
    #[error("model provider returned HTTP {status} after {attempts} attempt(s)")]
    Http { status: u16, attempts: u32 },
    #[error("invalid model response: {0}")]
    Protocol(&'static str),
    #[error("model response exceeded the configured byte limit")]
    ResponseTooLarge,
    #[error("model stream ended without a completion marker")]
    StreamInterrupted,
    #[error("configured model provider does not support streaming")]
    StreamingUnsupported,
}

impl ModelError {
    pub(crate) fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport
                | Self::Http {
                    status: 408 | 409 | 429 | 500..=599,
                    ..
                }
        )
    }
}
