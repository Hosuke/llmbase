//! Python transport only: every request enters the shared native application service.
mod compile;
mod config;
mod fetch;
mod maintenance;
mod model;
mod pipeline;
mod query;
mod resolve;
mod taxonomy;
mod util;
use llmbase_core::application::{Application, Principal, Request};
use llmbase_core::{Error, LibraryConfig};
use pyo3::exceptions::{
    PyFileNotFoundError, PyOSError, PyPermissionError, PyRuntimeError, PyValueError,
};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

static SUCCESSFUL_DISPATCHES: AtomicU64 = AtomicU64::new(0);
static WRITER_SESSIONS: AtomicU64 = AtomicU64::new(0);

fn configured_library(
    base_dir: &str,
    library_id: &str,
    paths_json: Option<&str>,
) -> PyResult<LibraryConfig> {
    let mut config = LibraryConfig::load(base_dir, library_id).map_err(python_error)?;
    // This in-process transport accepts live Python configuration. Public operation
    // arguments cannot override a library's configured roots.
    if let Some(paths) = paths_json {
        let paths: Value = serde_json::from_str(paths)
            .map_err(Error::from)
            .map_err(python_error)?;
        if !paths.is_object() {
            return Err(PyValueError::new_err("paths must be a mapping"));
        }
        for (key, target) in [
            ("raw", &mut config.raw),
            ("concepts", &mut config.concepts),
            ("outputs", &mut config.outputs),
            ("meta", &mut config.meta),
        ] {
            if let Some(value) = paths.get(key) {
                let path = value
                    .as_str()
                    .ok_or_else(|| PyValueError::new_err("configured paths must be strings"))?;
                *target = config.base.join(path);
            }
        }
    }
    Ok(config)
}

fn python_error(error: Error) -> PyErr {
    let message = error.to_string();
    match error {
        Error::Invalid(_) | Error::Json(_) | Error::Yaml(_) => PyValueError::new_err(message),
        Error::NotFound(_) => PyFileNotFoundError::new_err(message),
        Error::Scope => PyPermissionError::new_err(message),
        Error::Io(error) => match error.kind() {
            std::io::ErrorKind::NotFound => PyFileNotFoundError::new_err(error.to_string()),
            std::io::ErrorKind::PermissionDenied => PyPermissionError::new_err(error.to_string()),
            _ => PyOSError::new_err(error.to_string()),
        },
        _ => PyRuntimeError::new_err(message),
    }
}

#[pyfunction]
#[pyo3(signature = (base_dir, library_id, operation, args_json, paths_json=None))]
fn dispatch(
    py: Python<'_>,
    base_dir: String,
    library_id: String,
    operation: String,
    args_json: String,
    paths_json: Option<String>,
) -> PyResult<String> {
    // No borrowed Python values enter the closure; release the GIL during storage/model work.
    py.detach(move || {
        let args: Value = serde_json::from_str(&args_json)
            .map_err(Error::from)
            .map_err(python_error)?;
        let config = configured_library(&base_dir, &library_id, paths_json.as_deref())?;
        let application = Application::new(vec![config]).map_err(python_error)?;
        let request = Request {
            library_id: library_id.clone(),
            operation,
            args,
        };
        let result = application
            .execute(&Principal::local(&library_id), request)
            .map_err(python_error)?;
        SUCCESSFUL_DISPATCHES.fetch_add(1, Ordering::Relaxed);
        serde_json::to_string(&result)
            .map_err(Error::from)
            .map_err(python_error)
    })
}

#[pyfunction]
#[pyo3(signature = (base_dir, library_id, callback, paths_json=None))]
fn with_writer(
    py: Python<'_>,
    base_dir: String,
    library_id: String,
    callback: Py<PyAny>,
    paths_json: Option<String>,
) -> PyResult<Py<PyAny>> {
    let config = configured_library(&base_dir, &library_id, paths_json.as_deref())?;
    let _session = llmbase_core::store::WriterSession::acquire(&config).map_err(python_error)?;
    WRITER_SESSIONS.fetch_add(1, Ordering::Relaxed);
    // Preserve this OS thread for the core's lexical reentrancy guard. The Python
    // callable, including nested native dispatches, runs while the lease is alive.
    callback.call0(py)
}

#[pyfunction]
fn backend_info() -> String {
    json!({"core":"rust", "crate":"llmbase-python", "crate_version":env!("CARGO_PKG_VERSION"),
        "abi":"abi3-py311", "application":"llmbase_core::application::Application",
        "successful_dispatches":SUCCESSFUL_DISPATCHES.load(Ordering::Relaxed),
        "writer_sessions":WRITER_SESSIONS.load(Ordering::Relaxed)})
    .to_string()
}

#[pymodule]
fn _core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(dispatch, module)?)?;
    module.add_function(wrap_pyfunction!(backend_info, module)?)?;
    module.add_function(wrap_pyfunction!(with_writer, module)?)?;
    model::register(module)?;
    compile::register(module)?;
    config::register(module)?;
    pipeline::register(module)?;
    query::register(module)?;
    fetch::register(module)?;
    maintenance::register(module)?;
    util::register(module)?;
    resolve::register(module)?;
    taxonomy::register(module)?;
    Ok(())
}
