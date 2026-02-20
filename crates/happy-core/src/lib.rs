pub mod parser;
pub mod indexer;
pub mod graph;
pub mod global_index;
pub mod vector;
pub mod store;
pub mod watcher;
pub mod utils;

#[cfg(feature = "python")]
mod py;

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[pymodule]
fn happy_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    py::register(m)
}
