//! Explicit local pipeline roots may not exist yet; reads must not initialize a library.
use pyo3::prelude::*;
use std::path::Path;

#[pyclass]
struct StageLease {
    lease: Option<llmbase_core::compat_pipeline::StageLease>,
}

#[pymethods]
impl StageLease {
    fn release(&mut self) {
        self.lease.take();
    }

    fn owned_here(&self) -> bool {
        self.lease.as_ref().is_some_and(|lease| lease.owned_here())
    }

    fn receipt(&self) -> String {
        serde_json::json!({"core":"rust","operation":"stage_lease","storage":"kernel_flock",
            "held":self.owned_here()})
        .to_string()
    }
}

#[pyfunction]
fn stage_lease_acquire(
    py: Python<'_>,
    base_dir: String,
    stage: String,
    key: String,
) -> PyResult<Option<StageLease>> {
    py.detach(move || {
        llmbase_core::compat_pipeline::StageLease::acquire(Path::new(&base_dir), &stage, &key)
            .map(|lease| lease.map(|lease| StageLease { lease: Some(lease) }))
            .map_err(crate::python_error)
    })
}

#[pyfunction]
fn pipeline_dispatch(
    py: Python<'_>,
    base_dir: String,
    library_id: String,
    operation: String,
    args_json: String,
) -> PyResult<String> {
    py.detach(move || {
        llmbase_core::compat_pipeline::execute_json_at(
            Path::new(&base_dir),
            &library_id,
            &operation,
            &args_json,
        )
        .map_err(crate::python_error)
    })
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(pipeline_dispatch, module)?)?;
    module.add_function(wrap_pyfunction!(stage_lease_acquire, module)?)?;
    Ok(())
}
