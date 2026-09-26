//! Operator configuration for native transports. Credentials are read only from
//! process environment; callers cannot choose arbitrary outbound endpoints.
use llmbase_core::{Error, ask::AskModels};
use llmbase_model::{Client, ModelConfig, SecretKey};
use std::time::Duration;

fn value(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

fn number(name: &str, default: u64) -> Result<u64, Error> {
    match value(name) {
        Some(value) => value
            .parse()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::Invalid(format!("{name} must be a positive integer"))),
        None => Ok(default),
    }
}

pub fn from_environment() -> Result<Option<AskModels>, Error> {
    let base = value("LLMBASE_BASE_URL");
    let model = value("LLMBASE_MODEL");
    if base.is_none() && model.is_none() {
        return Ok(None);
    }
    let base = base.ok_or_else(|| {
        Error::Invalid("LLMBASE_BASE_URL is required when configuring a generation model".into())
    })?;
    let model = model.ok_or_else(|| {
        Error::Invalid("LLMBASE_MODEL is required when configuring a generation endpoint".into())
    })?;
    let key = value("LLMBASE_API_KEY").or_else(|| value("OPENAI_API_KEY"));
    let mut config = ModelConfig::new(&base, model);
    config.api_key = key.clone().map(SecretKey::new);
    config.fallback_models = value("LLMBASE_FALLBACK_MODELS")
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    config.retry.max_attempts = u32::try_from(number("LLMBASE_PRIMARY_RETRIES", 3)?)
        .map_err(|_| Error::Invalid("retry count too large".into()))?;
    config.retry.fallback_attempts = u32::try_from(number("LLMBASE_FALLBACK_RETRIES", 1)?)
        .map_err(|_| Error::Invalid("retry count too large".into()))?;
    config.connect_timeout = Duration::from_secs(number("LLMBASE_CONNECT_TIMEOUT_SECONDS", 30)?);
    config.request_timeout = Duration::from_secs(number("LLMBASE_REQUEST_TIMEOUT_SECONDS", 300)?);
    config.read_timeout = Duration::from_secs(number("LLMBASE_READ_TIMEOUT_SECONDS", 60)?);
    config.supports_streaming =
        !matches!(value("LLMBASE_STREAMING").as_deref(), Some("false" | "0"));
    config.reasoning_content_fallback = false;
    let generation = Client::new(config)?;
    let embedding = embedding_from_environment()?;
    Ok(Some(AskModels {
        generation,
        embedding,
    }))
}

/// Embedding-only runners need no generation model or generation credentials.
pub fn embedding_from_environment() -> Result<Option<Client>, Error> {
    let Some(model) = value("LLMBASE_EMBEDDING_MODEL") else {
        return Ok(None);
    };
    let base = value("LLMBASE_EMBEDDING_BASE_URL")
        .or_else(|| value("LLMBASE_BASE_URL"))
        .ok_or_else(|| Error::Invalid("an embedding endpoint is required".into()))?;
    let mut config = ModelConfig::new(base, model);
    config.api_key = value("LLMBASE_EMBEDDING_API_KEY")
        .or_else(|| value("LLMBASE_API_KEY"))
        .or_else(|| value("OPENAI_API_KEY"))
        .map(SecretKey::new);
    config.retry.max_attempts = u32::try_from(number("LLMBASE_PRIMARY_RETRIES", 3)?)
        .map_err(|_| Error::Invalid("retry count too large".into()))?;
    config.connect_timeout = Duration::from_secs(number("LLMBASE_CONNECT_TIMEOUT_SECONDS", 30)?);
    config.request_timeout = Duration::from_secs(number("LLMBASE_REQUEST_TIMEOUT_SECONDS", 300)?);
    config.read_timeout = Duration::from_secs(number("LLMBASE_READ_TIMEOUT_SECONDS", 60)?);
    Ok(Some(Client::new(config)?))
}
