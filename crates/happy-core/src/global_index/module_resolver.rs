use super::GlobalIndex;
use crate::parser::imports::ImportInfo;

/// Resolve import statements to file paths using the global index.
pub struct ModuleResolver<'a> {
    index: &'a GlobalIndex,
}

impl<'a> ModuleResolver<'a> {
    pub fn new(index: &'a GlobalIndex) -> Self {
        Self { index }
    }

    /// Resolve an import to a file path.
    pub fn resolve_import(&self, import: &ImportInfo, current_file: &str) -> Option<String> {
        if import.level > 0 {
            self.resolve_relative_import(import, current_file)
        } else {
            self.resolve_absolute_import(import)
        }
    }

    fn resolve_absolute_import(&self, import: &ImportInfo) -> Option<String> {
        // Try direct module path match
        if let Some(file) = self.index.resolve_module(&import.module) {
            return Some(file);
        }

        // Try as a package (module.__init__)
        let package_path = format!("{}.__init__", import.module);
        if let Some(file) = self.index.resolve_module(&package_path) {
            return Some(file);
        }

        // Try parent module + name
        if let Some(dot_pos) = import.module.rfind('.') {
            let parent = &import.module[..dot_pos];
            if let Some(file) = self.index.resolve_module(parent) {
                return Some(file);
            }
        }

        None
    }

    fn resolve_relative_import(&self, import: &ImportInfo, current_file: &str) -> Option<String> {
        let current_module = self.index.file_to_module(current_file)?;

        // Go up `level` packages
        let parts: Vec<&str> = current_module.split('.').collect();
        if import.level as usize > parts.len() {
            return None;
        }

        let base_parts = &parts[..parts.len() - import.level as usize];
        let base = base_parts.join(".");

        let target_module = if import.module.is_empty() {
            base
        } else {
            if base.is_empty() {
                import.module.clone()
            } else {
                format!("{}.{}", base, import.module)
            }
        };

        self.index.resolve_module(&target_module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_absolute() {
        let index = GlobalIndex::new();
        index
            .module_map
            .insert("os.path".into(), "/usr/lib/os/path.py".into());

        let resolver = ModuleResolver::new(&index);
        let import = ImportInfo {
            module: "os.path".into(),
            names: vec!["join".into()],
            level: 0,
            start_line: 1,
            end_line: 1,
        };

        assert_eq!(
            resolver.resolve_import(&import, "test.py"),
            Some("/usr/lib/os/path.py".into())
        );
    }

    #[test]
    fn test_resolve_relative() {
        let index = GlobalIndex::new();
        index.module_map.insert(
            "app.services.auth".into(),
            "/repo/app/services/auth.py".into(),
        );
        index.file_map.insert(
            "/repo/app/services/views.py".into(),
            "app.services.views".into(),
        );

        let resolver = ModuleResolver::new(&index);
        let import = ImportInfo {
            module: "auth".into(),
            names: vec!["login".into()],
            level: 1,
            start_line: 1,
            end_line: 1,
        };

        assert_eq!(
            resolver.resolve_import(&import, "/repo/app/services/views.py"),
            Some("/repo/app/services/auth.py".into())
        );
    }
}
