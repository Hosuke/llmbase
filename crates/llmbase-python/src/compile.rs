//! Local pure compatibility helpers share Rust domain functions without opening a library.
use llmbase_core::compat_compile;
use pyo3::{exceptions::PyValueError, prelude::*};
use serde_json::{Value, json};

#[pyfunction]
fn compile_transform(action: &str, args_json: &str) -> PyResult<String> {
    if args_json.len() > 16 * 1024 * 1024 {
        return Err(PyValueError::new_err(
            "compile transform exceeds byte limit",
        ));
    }
    let args: Value = serde_json::from_str(args_json)
        .map_err(|_| PyValueError::new_err("invalid compile transform arguments"))?;
    let value = compat_compile::transform(action, &args)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok(json!({"value":value,"trace":{"core":"rust","operation":"compile_transform","action":action,"storage":"none"}}).to_string())
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(compile_transform, module)?)?;
    Ok(())
}
