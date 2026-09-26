//! In-process config format host. Python documents never cross JSON: dates,
//! arbitrary keys, shared mappings and recursive aliases retain their identity.
use llmbase_core::compat_config::{self, Failure, ReadStep};
use pyo3::exceptions::{PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::json;
use std::path::PathBuf;

enum BridgeError {
    Native(Failure),
    Python(PyErr),
}
impl From<Failure> for BridgeError {
    fn from(value: Failure) -> Self {
        Self::Native(value)
    }
}
impl From<PyErr> for BridgeError {
    fn from(value: PyErr) -> Self {
        Self::Python(value)
    }
}
impl BridgeError {
    fn into_py(self) -> PyErr {
        match self {
            Self::Python(error) => error,
            Self::Native(Failure::Io { path, source }) => PyOSError::new_err((
                source.raw_os_error().unwrap_or(5),
                source.to_string(),
                path.into_os_string(),
            )),
            Self::Native(Failure::SymlinkLoop(path)) => {
                PyRuntimeError::new_err(format!("Symlink loop from '{}'", path.display()))
            }
            Self::Native(Failure::Invalid(message)) => PyValueError::new_err(message),
        }
    }
}
fn native_error(error: Failure) -> PyErr {
    BridgeError::Native(error).into_py()
}

#[pyfunction]
fn config_read_run(
    py: Python<'_>,
    base: PathBuf,
    decoder: Py<PyAny>,
    defaults: Py<PyAny>,
    path_coercer: Py<PyAny>,
) -> PyResult<(Py<PyAny>, String)> {
    py.detach(move || {
        let mut document: Option<Py<PyAny>> = None;
        let parsed = compat_config::read_with::<BridgeError>(&base, |step| {
            Python::attach(|py| -> PyResult<Option<PathBuf>> {
                match step {
                    ReadStep::Defaults => document = Some(defaults.call0(py)?),
                    ReadStep::Decode(bytes) => {
                        document = Some(decoder.call1(py, (PyBytes::new(py, bytes),))?)
                    }
                    ReadStep::Path(key) => {
                        let cfg = document.as_ref().expect("initialized document").bind(py);
                        let paths = cfg.call_method1("get", ("paths", PyDict::new(py)))?;
                        let path = paths.call_method1("get", (key,))?;
                        if path.is_truthy()? {
                            return Ok(Some(path_coercer.call1(py, (path,))?.extract(py)?));
                        }
                    }
                    ReadStep::ResolvedPath(key, path) => {
                        document
                            .as_ref()
                            .expect("initialized document")
                            .bind(py)
                            .get_item("paths")?
                            .set_item(key, path.as_os_str())?;
                    }
                    ReadStep::Base(path) => {
                        document
                            .as_ref()
                            .expect("initialized document")
                            .bind(py)
                            .set_item("base_dir", path.as_os_str())?;
                    }
                }
                Ok(None)
            })
            .map_err(BridgeError::from)
        })
        .map_err(BridgeError::into_py)?;
        Ok((
            document.expect("successful read initialized document"),
            json!({
                "core":"rust", "operation":"config_read", "storage":"filesystem",
                "adapter":"llmbase_core::compat_config::read_with",
                "parser":if parsed {"python_yaml"} else {"defaults"}
            })
            .to_string(),
        ))
    })
}

#[pyfunction]
fn config_default_paths(py: Python<'_>, base: PathBuf) -> PyResult<(Py<PyDict>, String)> {
    let paths = py
        .detach(move || compat_config::default_paths(&base))
        .map_err(native_error)?;
    let document = PyDict::new(py);
    for (key, path) in paths {
        document.set_item(key, path.as_os_str())?;
    }
    Ok((
        document.unbind(),
        json!({"core":"rust","operation":"config_default_paths",
        "storage":"none","adapter":"llmbase_core::compat_config::default_paths"})
        .to_string(),
    ))
}

#[pyfunction]
fn config_ensure_dirs(py: Python<'_>, cfg: Py<PyAny>, path_coercer: Py<PyAny>) -> PyResult<String> {
    py.detach(move || {
        compat_config::ensure_dirs_with::<BridgeError>(|key| {
            Python::attach(|py| {
                let path = cfg.bind(py).get_item("paths")?.get_item(key)?;
                path_coercer.call1(py, (path,))?.extract::<PathBuf>(py)
            })
            .map_err(BridgeError::from)
        })
        .map_err(BridgeError::into_py)?;
        Ok(
            json!({"core":"rust","operation":"config_ensure_dirs","storage":"filesystem",
            "adapter":"llmbase_core::compat_config::ensure_dirs_with"})
            .to_string(),
        )
    })
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(config_read_run, module)?)?;
    module.add_function(wrap_pyfunction!(config_default_paths, module)?)?;
    module.add_function(wrap_pyfunction!(config_ensure_dirs, module)?)?;
    Ok(())
}
