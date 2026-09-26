//! Explicit local Python callback host for the native compatibility query sequence.
use llmbase_core::{Error, compat_query};
use pyo3::{exceptions::PyValueError, prelude::*};
use serde_json::{Value, json};

#[pyfunction]
#[pyo3(signature=(base_dir,library_id,request_json,callback,paths_json=None))]
fn query_run(
    py: Python<'_>,
    base_dir: String,
    library_id: String,
    request_json: String,
    callback: Py<PyAny>,
    paths_json: Option<String>,
) -> PyResult<String> {
    if request_json.len() > 16 * 1024 * 1024 {
        return Err(PyValueError::new_err("query request exceeds 16 MiB"));
    }
    py.detach(move || {
        let config = crate::configured_library(&base_dir, &library_id, paths_json.as_deref())?;
        let args: Value = serde_json::from_str(&request_json)
            .map_err(Error::from)
            .map_err(crate::python_error)?;
        let mut callback_error = None;
        let result = compat_query::run(&config, &args, |stage, payload| {
            let called = Python::attach(|py| {
                callback
                    .call1(py, (stage, payload.to_string()))
                    .and_then(|value| value.extract::<String>(py))
            });
            match called {
                Ok(value) => serde_json::from_str(&value).map_err(Error::from),
                Err(error) => {
                    callback_error = Some(error);
                    Err(Error::Capability("Python query callback failed".into()))
                }
            }
        });
        match result {
            Ok(value) => Ok(value.to_string()),
            Err(error) => Err(callback_error.unwrap_or_else(|| crate::python_error(error))),
        }
    })
}
#[pyfunction]
fn query_transform(action: &str, args_json: &str) -> PyResult<String> {
    if args_json.len() > 16 * 1024 * 1024 {
        return Err(PyValueError::new_err("query transform exceeds 16 MiB"));
    }
    let args: Value = serde_json::from_str(args_json)
        .map_err(Error::from)
        .map_err(crate::python_error)?;
    let value = compat_query::transform(action, &args).map_err(crate::python_error)?;
    Ok(json!({"value":value,"trace":{"core":"rust","operation":"query_transform","action":action,"storage":"none"}}).to_string())
}
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(query_run, module)?)?;
    module.add_function(wrap_pyfunction!(query_transform, module)?)?;
    Ok(())
}
