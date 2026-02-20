use happy_core::graph::RepositoryGraph;
use happy_core::indexer::{self, CodeElement};
use happy_core::store;
use happy_core::vector::BM25Index;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Shared repository context holding all indexed data.
pub struct RepoContext {
    pub repo_path: String,
    pub elements: Vec<CodeElement>,
    pub graph: RepositoryGraph,
    pub bm25: BM25Index,
}

impl RepoContext {
    /// Load or index a repository. Tries cache first.
    pub fn load(repo_path: &str) -> anyhow::Result<Self> {
        let elements = try_load_cache(repo_path).unwrap_or_else(|| {
            let start = Instant::now();
            let elements = indexer::walk_and_index(repo_path);
            eprintln!(
                "Indexed {} elements in {:.2?}",
                elements.len(),
                start.elapsed()
            );
            elements
        });

        let start = Instant::now();
        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements);
        eprintln!("Built graph in {:.2?}", start.elapsed());

        let start = Instant::now();
        let mut bm25 = BM25Index::new();
        for elem in &elements {
            let text = format!(
                "{} {} {}",
                elem.name,
                elem.code,
                elem.docstring.as_deref().unwrap_or("")
            );
            bm25.add_document(&elem.id, &text);
        }
        eprintln!("Built BM25 index in {:.2?}", start.elapsed());

        Ok(Self {
            repo_path: repo_path.to_string(),
            elements,
            graph,
            bm25,
        })
    }

    pub fn happy_dir(&self) -> PathBuf {
        let dir = Path::new(&self.repo_path).join(".happy");
        if !dir.exists() {
            std::fs::create_dir_all(&dir).ok();
        }
        dir
    }

    pub fn save_cache(&self) -> anyhow::Result<()> {
        let dir = self.happy_dir();
        store::save_elements(&self.elements, &dir.join("elements.bin"))?;
        store::save_bm25(&self.bm25, &dir.join("bm25.bin"))?;
        eprintln!("Saved index to {}/", dir.display());
        Ok(())
    }

    pub fn list_files(&self) -> Vec<String> {
        let mut files = self.graph.file_paths();
        files.sort();
        files
    }
}

fn try_load_cache(repo_path: &str) -> Option<Vec<CodeElement>> {
    let dir = Path::new(repo_path).join(".happy");
    let elements_path = dir.join("elements.bin");
    if elements_path.exists() {
        match store::load_elements(&elements_path) {
            Ok(elements) => {
                eprintln!("Loaded {} cached elements from .happy/", elements.len());
                Some(elements)
            }
            Err(e) => {
                eprintln!("Cache load failed ({}), re-indexing...", e);
                None
            }
        }
    } else {
        None
    }
}
