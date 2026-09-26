//! Trusted local Python taxonomy ABI; no remote path selection is exposed here.
use llmbase_core::compat_taxonomy;
use pyo3::{exceptions::PyValueError, prelude::*};
use serde_json::{Value, json};

#[pyfunction]
fn taxonomy_transform(py: Python<'_>, action: String, args_json: String) -> PyResult<String> {
    if args_json.len() > 16 * 1024 * 1024 {
        return Err(PyValueError::new_err("taxonomy helper exceeds 16 MiB"));
    }
    py.detach(move || {
        let args: Value = serde_json::from_str(&args_json).map_err(|_| PyValueError::new_err("invalid taxonomy arguments"))?;
        let value = compat_taxonomy::transform(&action, &args).map_err(crate::python_error)?;
        Ok(json!({"value":value,"trace":{"core":"rust","operation":"taxonomy_transform","action":action,"storage":if matches!(action.as_str(),"load_articles"|"title_map"|"load"|"build"){ "read_only_filesystem" }else{ "none" }}}).to_string())
    })
}
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(taxonomy_transform, module)?)?;
    Ok(())
}
