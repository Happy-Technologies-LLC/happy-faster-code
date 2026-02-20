#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
use pyo3::types::PyDict;

#[cfg(feature = "python")]
use crate::graph::RepositoryGraph;
#[cfg(feature = "python")]
use crate::global_index::GlobalIndex;
#[cfg(feature = "python")]
use crate::vector::{BM25Index, VectorIndex};
#[cfg(feature = "python")]
use crate::indexer;

#[cfg(feature = "python")]
#[pyclass]
pub struct HappyRepo {
    graph: RepositoryGraph,
    global_index: GlobalIndex,
    bm25: BM25Index,
    vector: Option<VectorIndex>,
    repo_path: String,
}

#[cfg(feature = "python")]
#[pymethods]
impl HappyRepo {
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let elements = indexer::walk_and_index(path);

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements);

        let global_index = GlobalIndex::new();
        global_index.build(&elements, path);

        let mut bm25 = BM25Index::new();
        for elem in &elements {
            let text = format!("{} {} {}", elem.name, elem.code, elem.docstring.as_deref().unwrap_or(""));
            bm25.add_document(&elem.id, &text);
        }

        Ok(Self {
            graph,
            global_index,
            bm25,
            vector: None,
            repo_path: path.to_string(),
        })
    }

    fn find_callers(&self, symbol: &str) -> Vec<String> {
        self.graph.find_callers(symbol)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    fn find_callees(&self, symbol: &str) -> Vec<String> {
        self.graph.find_callees(symbol)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    fn get_dependencies(&self, file: &str) -> Vec<String> {
        self.graph.get_dependencies(file)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    fn get_dependents(&self, file: &str) -> Vec<String> {
        self.graph.get_dependents(file)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    fn get_subclasses(&self, class_name: &str) -> Vec<String> {
        self.graph.get_subclasses(class_name)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    fn get_superclasses(&self, class_name: &str) -> Vec<String> {
        self.graph.get_superclasses(class_name)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    fn find_path(&self, source: &str, target: &str) -> Option<Vec<String>> {
        self.graph.find_path(source, target, None)
    }

    fn get_related(&self, element: &str, max_hops: usize) -> Vec<String> {
        self.graph.get_related(element, max_hops)
            .into_iter()
            .map(|n| n.id.clone())
            .collect()
    }

    fn search(&self, query: &str, k: usize) -> Vec<(String, f64)> {
        self.bm25.search(query, k)
    }

    fn add_embeddings(&mut self, ids: Vec<String>, vectors: Vec<Vec<f32>>) {
        if vectors.is_empty() {
            return;
        }
        let dim = vectors[0].len();
        let mut vi = VectorIndex::new(dim);
        vi.add(&ids, &vectors);
        self.vector = Some(vi);
    }

    fn search_by_vector(&self, vector: Vec<f32>, k: usize) -> Vec<(String, f32)> {
        match &self.vector {
            Some(vi) => vi.search(&vector, k, 0.0),
            None => Vec::new(),
        }
    }

    fn get_source(&self, element_id: &str) -> Option<String> {
        self.graph.get_source(element_id)
    }

    fn stats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        let gs = self.graph.stats();
        dict.set_item("nodes", gs.node_count)?;
        dict.set_item("edges", gs.edge_count)?;
        dict.set_item("files", gs.file_count)?;
        dict.set_item("elements", gs.element_count)?;
        dict.set_item("bm25_docs", self.bm25.len())?;
        dict.set_item("has_vectors", self.vector.is_some())?;
        Ok(dict)
    }

    fn file_tree(&self) -> Vec<String> {
        let mut files: Vec<String> = self.graph.file_to_nodes
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        files.sort();
        files
    }

    #[getter]
    fn path(&self) -> &str {
        &self.repo_path
    }

    fn resolve_module(&self, module_name: &str) -> Option<String> {
        self.global_index.resolve_module(module_name)
    }

    fn resolve_symbol(&self, symbol_name: &str) -> Vec<(String, String)> {
        self.global_index.resolve_symbol(symbol_name)
    }
}

#[cfg(feature = "python")]
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<HappyRepo>()?;
    Ok(())
}
