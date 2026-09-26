//! In-process transport for the shared public-web fetcher; no Python HTTP work.
use llmbase_core::fetch::{self, CancellationToken, FetchOptions};
use pyo3::prelude::*;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

static FETCHES: AtomicU64 = AtomicU64::new(0);

#[pyclass(frozen, module = "llmwiki._core")]
struct FetchCancellation {
    token: CancellationToken,
}

#[pymethods]
impl FetchCancellation {
    #[new]
    fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }
    fn cancel(&self) {
        self.token.cancel();
    }
    fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

#[pyfunction]
#[pyo3(signature = (url, options_json, cancellation=None))]
fn fetch_url(
    py: Python<'_>,
    url: String,
    options_json: String,
    cancellation: Option<PyRef<'_, FetchCancellation>>,
) -> PyResult<String> {
    let cancellation = cancellation.map(|c| c.token.clone()).unwrap_or_default();
    py.detach(move || {
        let options: FetchOptions = serde_json::from_str(&options_json)
            .map_err(llmbase_core::Error::from).map_err(super::python_error)?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()
            .map_err(llmbase_core::Error::from).map_err(super::python_error)?;
        let result = runtime.block_on(fetch::fetch(&url, options, cancellation));
        // DNS may use a platform blocking resolver. Dropping its future cancels
        // use of the result; shutdown_background never waits for a stalled DNS call.
        runtime.shutdown_background();
        let response = result.map_err(super::python_error)?;
        let trace = json!({"core":"rust", "operation":"fetch_url", "adapter":"llmbase_core::fetch::fetch",
            "pinned_dns":true, "environment_proxy":false, "redirects":response.redirects,
            "http_attempts":response.http_attempts, "bytes":response.body_bytes.len(),
            "sequence": FETCHES.fetch_add(1, Ordering::Relaxed) + 1});
        serde_json::to_string(&json!({"response":response, "trace":trace}))
            .map_err(llmbase_core::Error::from).map_err(super::python_error)
    })
}

#[pyfunction]
fn validate_fetch_url(py: Python<'_>, url: String) -> PyResult<()> {
    py.detach(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(llmbase_core::Error::from)
            .map_err(super::python_error)?;
        let result = runtime.block_on(fetch::validate_url(&url, CancellationToken::new()));
        runtime.shutdown_background();
        result.map_err(super::python_error)
    })
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<FetchCancellation>()?;
    module.add_function(wrap_pyfunction!(fetch_url, module)?)?;
    module.add_function(wrap_pyfunction!(validate_fetch_url, module)?)?;
    Ok(())
}
