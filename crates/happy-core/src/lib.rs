pub mod global_index;
pub mod graph;
pub mod indexer;
pub mod parser;
pub mod store;
pub mod utils;
pub mod vector;
pub mod watcher;

#[cfg(feature = "python")]
mod py;

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[pymodule]
fn happy_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    py::register(m)
}
