//! Native lint transforms and sequencing with explicit live Python callbacks.
use llmbase_core::compat_maintenance;
use pyo3::{exceptions::PyValueError, prelude::*};
use serde_json::{Value, json};

fn arguments(args_json: &str) -> PyResult<Value> {
    if args_json.len() > 64 * 1024 * 1024 {
        return Err(PyValueError::new_err("maintenance arguments exceed 64 MiB"));
    }
    serde_json::from_str(args_json)
        .map_err(|_| PyValueError::new_err("invalid maintenance arguments"))
}

fn receipt(operation: &str, action: &str, value: Value) -> String {
    let trace = json!({"core":"rust","operation":operation,"action":action});
    json!({"value":value,"trace":trace}).to_string()
}

#[pyfunction]
fn maintenance_transform(py: Python<'_>, action: String, args_json: String) -> PyResult<String> {
    let args = arguments(&args_json)?;
    py.detach(move || {
        let value = compat_maintenance::transform(&action, &args).map_err(crate::python_error)?;
        Ok(receipt("maintenance_transform", &action, value))
    })
}

#[pyfunction]
fn maintenance_run(
    py: Python<'_>,
    action: &str,
    args_json: &str,
    callback: Py<PyAny>,
) -> PyResult<String> {
    let args = arguments(args_json)?;
    if action != "lint" {
        return Err(PyValueError::new_err("unsupported maintenance sequence"));
    }
    let mut callback_error = None;
    let value = compat_maintenance::run_lint(args, |stage, payload| {
        match callback
            .call1(py, (stage, payload.to_string()))
            .and_then(|value| value.extract::<String>(py))
        {
            Ok(value) => serde_json::from_str(&value).map_err(Into::into),
            Err(error) => {
                callback_error = Some(error);
                Err(llmbase_core::Error::Capability(
                    "Python maintenance callback failed".into(),
                ))
            }
        }
    });
    // The original Python exception is preserved, never wrapped or retried.
    if let Some(error) = callback_error {
        return Err(error);
    }
    let value = value.map_err(crate::python_error)?;
    Ok(receipt("maintenance_run", action, value))
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(maintenance_transform, module)?)?;
    module.add_function(wrap_pyfunction!(maintenance_run, module)?)?;
    Ok(())
}
