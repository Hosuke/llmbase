use pyo3::prelude::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
static CALLS: AtomicU64 = AtomicU64::new(0);

#[pyfunction]
fn utility_transform(py: Python<'_>, action: String, args_json: String) -> PyResult<String> {
    py.detach(move||{
        let args:Value=serde_json::from_str(&args_json).map_err(llmbase_core::Error::from).map_err(super::python_error)?;
        let value=llmbase_core::compat_util::transform(&action,&args).map_err(super::python_error)?;
        Ok(json!({"value":value,"trace":{"core":"rust","operation":format!("utility_{action}"),
            "adapter":"llmbase_core::compat_util::transform","offset_unit":"unicode_scalars","sequence":CALLS.fetch_add(1,Ordering::Relaxed)+1}}).to_string())
    })
}
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(utility_transform, module)?)?;
    module.add_function(wrap_pyfunction!(atomic_write_at, module)?)?;
    module.add_function(wrap_pyfunction!(normalize_heads_run, module)?)?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature=(body,callback=None))]
fn normalize_heads_run(
    py: Python<'_>,
    body: String,
    callback: Option<Py<PyAny>>,
) -> PyResult<String> {
    py.detach(move || {
        let enabled = callback.is_some();
        let mut callback_error = None;
        let mut callback_count = 0_u64;
        let result = llmbase_core::compat_util::normalize_heads_with(&body, enabled, |title| {
            callback_count += 1;
            let called = Python::attach(|py| {
                callback
                    .as_ref()
                    .expect("enabled classifier")
                    .call1(py, (title,))
                    .and_then(|value| value.extract::<Option<u8>>(py))
            });
            match called {
                Ok(value) => Ok(value),
                Err(error) => {
                    callback_error = Some(error);
                    Err(llmbase_core::Error::Capability(
                        "Python heading matcher failed".into(),
                    ))
                }
            }
        });
        match result {
            Ok(value) => Ok(json!({"value":value,"trace":{
                "core":"rust","operation":"utility_normalize_heads","storage":"none",
                "adapter":"llmbase_core::compat_util::normalize_heads_with",
                "matcher":if enabled {"python_regex"} else {"none"},
                "callback_count":callback_count,
                "sequence":CALLS.fetch_add(1,Ordering::Relaxed)+1
            }})
            .to_string()),
            Err(error) => Err(callback_error.unwrap_or_else(|| super::python_error(error))),
        }
    })
}

#[pyfunction]
#[pyo3(signature=(path,content,base_dir=None,library_id="default",paths_json=None))]
fn atomic_write_at(
    py: Python<'_>,
    path: String,
    content: Vec<u8>,
    base_dir: Option<String>,
    library_id: &str,
    paths_json: Option<String>,
) -> PyResult<String> {
    let library_id = library_id.to_owned();
    py.detach(move || {
        let config = base_dir
            .as_ref()
            .map(|base| super::configured_library(base, &library_id, paths_json.as_deref()))
            .transpose()?;
        let value = llmbase_core::compat_io::atomic_write(
            std::path::Path::new(&path),
            &content,
            config.as_ref(),
        )
        .map_err(super::python_error)?;
        Ok(
            json!({"value":value,"trace":{"core":"rust","operation":"atomic_write",
            "adapter":"llmbase_core::compat_io::atomic_write","coordination":value["coordination"],
            "journal":value["journal"],"library_id":config.as_ref().map(|c|&c.library_id),
            "sequence":CALLS.fetch_add(1,Ordering::Relaxed)+1}})
            .to_string(),
        )
    })
}
