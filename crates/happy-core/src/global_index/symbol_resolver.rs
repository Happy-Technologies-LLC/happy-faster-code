use super::GlobalIndex;

/// Resolve symbol names to element IDs using the global index.
pub struct SymbolResolver<'a> {
    index: &'a GlobalIndex,
}

impl<'a> SymbolResolver<'a> {
    pub fn new(index: &'a GlobalIndex) -> Self {
        Self { index }
    }

    /// Resolve a symbol name to its element IDs.
    /// Returns (file_path, element_id) pairs.
    pub fn resolve(&self, symbol: &str) -> Vec<(String, String)> {
        self.index.resolve_symbol(symbol)
    }

    /// Resolve a symbol in the context of a specific file's imports.
    /// This narrows results to symbols that are actually imported by the file.
    pub fn resolve_in_context(
        &self,
        symbol: &str,
        _current_file: &str,
        imported_modules: &[String],
    ) -> Vec<(String, String)> {
        let all = self.index.resolve_symbol(symbol);

        if imported_modules.is_empty() {
            return all;
        }

        // Filter to only symbols from imported modules
        all.into_iter()
            .filter(|(file_path, _)| {
                if let Some(module) = self.index.file_to_module(file_path) {
                    imported_modules.iter().any(|m| module.starts_with(m))
                } else {
                    false
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_symbol() {
        let index = GlobalIndex::new();
        index.export_map.insert(
            "login".into(),
            vec![
                ("auth.py".into(), "func_login".into()),
                ("views.py".into(), "func_login_view".into()),
            ],
        );

        let resolver = SymbolResolver::new(&index);
        let results = resolver.resolve("login");
        assert_eq!(results.len(), 2);
    }
}
