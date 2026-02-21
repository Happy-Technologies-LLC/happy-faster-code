pub mod module_resolver;
pub mod symbol_resolver;

use dashmap::DashMap;

/// Global index providing fast lookups across the entire repository.
pub struct GlobalIndex {
    /// file_path -> module path (e.g., "src/app/auth.py" -> "src.app.auth")
    pub file_map: DashMap<String, String>,
    /// module path -> file_path
    pub module_map: DashMap<String, String>,
    /// symbol name -> Vec<(file_path, element_id)>
    pub export_map: DashMap<String, Vec<(String, String)>>,
}

impl GlobalIndex {
    pub fn new() -> Self {
        Self {
            file_map: DashMap::new(),
            module_map: DashMap::new(),
            export_map: DashMap::new(),
        }
    }

    /// Build the index from code elements.
    pub fn build(&self, elements: &[crate::indexer::CodeElement], repo_root: &str) {
        for elem in elements {
            if elem.element_type == crate::indexer::ElementType::File {
                if let Some(module_path) =
                    crate::utils::file_path_to_module_path(&elem.file_path, repo_root)
                {
                    self.file_map
                        .insert(elem.file_path.clone(), module_path.clone());
                    self.module_map.insert(module_path, elem.file_path.clone());
                }
            } else {
                // Register as exported symbol
                self.export_map
                    .entry(elem.name.clone())
                    .or_default()
                    .push((elem.file_path.clone(), elem.id.clone()));
            }
        }
    }

    /// Look up which file a module path resolves to.
    pub fn resolve_module(&self, module_path: &str) -> Option<String> {
        self.module_map.get(module_path).map(|v| v.clone())
    }

    /// Look up which elements export a given symbol name.
    pub fn resolve_symbol(&self, symbol: &str) -> Vec<(String, String)> {
        self.export_map
            .get(symbol)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Get the module path for a file.
    pub fn file_to_module(&self, file_path: &str) -> Option<String> {
        self.file_map.get(file_path).map(|v| v.clone())
    }

    /// Remove all entries associated with a file path.
    pub fn remove_file(&self, file_path: &str) {
        // Remove file_map → module_map entry
        if let Some((_, module_path)) = self.file_map.remove(file_path) {
            self.module_map.remove(&module_path);
        }

        // Remove all export_map entries from this file
        let mut empty_keys = Vec::new();
        for mut entry in self.export_map.iter_mut() {
            entry.value_mut().retain(|(fp, _)| fp != file_path);
            if entry.value().is_empty() {
                empty_keys.push(entry.key().clone());
            }
        }
        for key in empty_keys {
            self.export_map.remove(&key);
        }
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        (
            self.file_map.len(),
            self.module_map.len(),
            self.export_map.len(),
        )
    }
}

impl Default for GlobalIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::element::*;
    use std::collections::HashMap;

    #[test]
    fn test_build_index() {
        let elements = vec![
            CodeElement {
                id: "file_auth".into(),
                element_type: ElementType::File,
                name: "auth.py".into(),
                file_path: "/repo/src/auth.py".into(),
                relative_path: "src/auth.py".into(),
                language: "python".into(),
                start_line: 1,
                end_line: 50,
                code: String::new(),
                signature: None,
                docstring: None,
                summary: None,
                metadata: HashMap::new(),
            },
            CodeElement {
                id: "func_login".into(),
                element_type: ElementType::Function,
                name: "login".into(),
                file_path: "/repo/src/auth.py".into(),
                relative_path: "src/auth.py".into(),
                language: "python".into(),
                start_line: 5,
                end_line: 20,
                code: "def login(): pass".into(),
                signature: Some("def login():".into()),
                docstring: None,
                summary: None,
                metadata: HashMap::new(),
            },
        ];

        let index = GlobalIndex::new();
        index.build(&elements, "/repo");

        assert_eq!(
            index.resolve_module("src.auth"),
            Some("/repo/src/auth.py".into())
        );
        assert_eq!(index.resolve_symbol("login").len(), 1);
        assert_eq!(
            index.file_to_module("/repo/src/auth.py"),
            Some("src.auth".into())
        );
    }

    #[test]
    fn test_remove_file() {
        let elements = vec![
            CodeElement {
                id: "file_auth".into(),
                element_type: ElementType::File,
                name: "auth.py".into(),
                file_path: "/repo/src/auth.py".into(),
                relative_path: "src/auth.py".into(),
                language: "python".into(),
                start_line: 1,
                end_line: 50,
                code: String::new(),
                signature: None,
                docstring: None,
                summary: None,
                metadata: HashMap::new(),
            },
            CodeElement {
                id: "func_login".into(),
                element_type: ElementType::Function,
                name: "login".into(),
                file_path: "/repo/src/auth.py".into(),
                relative_path: "src/auth.py".into(),
                language: "python".into(),
                start_line: 5,
                end_line: 20,
                code: "def login(): pass".into(),
                signature: Some("def login():".into()),
                docstring: None,
                summary: None,
                metadata: HashMap::new(),
            },
        ];

        let index = GlobalIndex::new();
        index.build(&elements, "/repo");
        assert_eq!(index.resolve_symbol("login").len(), 1);

        // Remove the file
        index.remove_file("/repo/src/auth.py");

        // Module mapping should be gone
        assert_eq!(index.resolve_module("src.auth"), None);
        assert_eq!(index.file_to_module("/repo/src/auth.py"), None);

        // Export map should be cleaned
        assert_eq!(index.resolve_symbol("login").len(), 0);
    }
}
