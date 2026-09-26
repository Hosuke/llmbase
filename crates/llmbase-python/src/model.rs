//! In-process compatibility transport; never registered as a remote operation.
//! Provider credentials are separate ephemeral arguments and never enter storage.
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use llmbase_model::{CancellationToken, ChatRequest, Client, ModelConfig, SecretKey};
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
};
use serde::Deserialize;
use serde_json::json;

static COMPLETIONS: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    base_url: String,
    model: String,
    request_timeout_seconds: f64,
    connect_timeout_seconds: f64,
}

fn duration(value: f64) -> PyResult<Duration> {
    if !value.is_finite() || value <= 0.0 {
        return Err(PyValueError::new_err(
            "model timeout must be finite and positive",
        ));
    }
    Duration::try_from_secs_f64(value)
        .map_err(|_| PyValueError::new_err("model timeout is outside the supported range"))
}

#[pyfunction]
#[pyo3(signature = (config_json, request_json, api_key=None))]
fn model_complete(
    py: Python<'_>,
    config_json: String,
    request_json: String,
    api_key: Option<String>,
) -> PyResult<String> {
    // Moving owned arguments through detach releases the GIL during HTTP, so a
    // caller's Python threads and local fixture server can continue to progress.
    py.detach(move || {
        if config_json.len() > 16384 || request_json.len() > 8 * 1024 * 1024 {
            return Err(PyValueError::new_err(
                "model bridge input exceeds byte limit",
            ));
        }
        let supplied: Config = serde_json::from_str(&config_json)
            .map_err(|_| PyValueError::new_err("invalid model bridge configuration"))?;
        let request: ChatRequest = serde_json::from_str(&request_json)
            .map_err(|_| PyValueError::new_err("invalid model bridge request"))?;
        let mut config = ModelConfig::new(supplied.base_url, supplied.model);
        config.request_timeout = duration(supplied.request_timeout_seconds)?;
        config.read_timeout = config.request_timeout;
        config.connect_timeout = duration(supplied.connect_timeout_seconds)?;
        config.api_key = api_key.map(SecretKey::new);
        // The compatibility facade counts each _call_llm invocation. Its live
        // Python retry/callback contract owns the outer attempts and fallback.
        config.retry.max_attempts = 1;
        config.retry.fallback_attempts = 1;
        let client = Client::new(config).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| PyRuntimeError::new_err("model runtime could not start"))?;
        let response = runtime
            .block_on(client.complete(request, CancellationToken::new()))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let sequence = COMPLETIONS.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(json!({
            "trace": {"core":"rust","operation":"model_complete",
                "adapter":"llmbase_model::Client::complete",
                "http_attempts":response.attempts,"sequence":sequence},
            "response":response
        })
        .to_string())
    })
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(model_complete, module)?)?;
    Ok(())
}
