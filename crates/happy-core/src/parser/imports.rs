use tree_sitter::{Node, Tree};

/// Information about an import statement.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub module: String,
    pub names: Vec<String>,
    pub level: u32,
    pub start_line: usize,
    pub end_line: usize,
}

/// Extract import statements from a Python AST.
pub fn extract_imports(tree: &Tree, code: &str) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    collect_imports(&root, code, &mut imports);
    imports
}

fn collect_imports(node: &Node, code: &str, imports: &mut Vec<ImportInfo>) {
    let code_bytes = code.as_bytes();

    match node.kind() {
        // import foo, import foo.bar
        "import_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "dotted_name" {
                    let module = child.utf8_text(code_bytes).unwrap_or_default().to_string();
                    imports.push(ImportInfo {
                        module: module.clone(),
                        names: vec![module],
                        level: 0,
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                    });
                } else if child.kind() == "aliased_import" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let module = name_node.utf8_text(code_bytes).unwrap_or_default().to_string();
                        imports.push(ImportInfo {
                            module: module.clone(),
                            names: vec![module],
                            level: 0,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                        });
                    }
                }
            }
        }
        // from foo import bar, baz
        // from . import bar
        // from ..foo import bar
        "import_from_statement" => {
            let mut module = String::new();
            let mut names = Vec::new();
            let mut level = 0u32;

            // Use field names to distinguish module from imported names
            if let Some(module_node) = node.child_by_field_name("module_name") {
                module = module_node.utf8_text(code_bytes).unwrap_or_default().to_string();
            }

            let mut cursor = node.walk();
            let mut seen_import_keyword = false;
            for child in node.children(&mut cursor) {
                let kind = child.kind();
                let text = child.utf8_text(code_bytes).unwrap_or_default();

                if text == "import" && kind == "import" {
                    seen_import_keyword = true;
                    continue;
                }

                if !seen_import_keyword {
                    // Before "import" keyword: module/prefix info
                    match kind {
                        "relative_import" => {
                            let mut rc = child.walk();
                            for rchild in child.children(&mut rc) {
                                match rchild.kind() {
                                    "import_prefix" => {
                                        let prefix = rchild.utf8_text(code_bytes).unwrap_or_default();
                                        level = prefix.chars().filter(|&c| c == '.').count() as u32;
                                    }
                                    "dotted_name" => {
                                        module = rchild.utf8_text(code_bytes).unwrap_or_default().to_string();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "import_prefix" => {
                            level = text.chars().filter(|&c| c == '.').count() as u32;
                        }
                        _ => {}
                    }
                } else {
                    // After "import" keyword: imported names
                    match kind {
                        "dotted_name" | "identifier" => {
                            let name = text.to_string();
                            if !name.is_empty() {
                                names.push(name);
                            }
                        }
                        "aliased_import" => {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = name_node.utf8_text(code_bytes).unwrap_or_default().to_string();
                                names.push(name);
                            }
                        }
                        "wildcard_import" => {
                            names.push("*".to_string());
                        }
                        _ => {}
                    }
                }
            }

            imports.push(ImportInfo {
                module,
                names,
                level,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            });
        }
        _ => {
            // Only recurse into top-level children (imports are usually at module level)
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_imports(&child, code, imports);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use crate::parser::languages::SupportedLanguage;

    #[test]
    fn test_simple_import() {
        let code = "import os\nimport sys\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let imports = extract_imports(&tree, code);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].module, "os");
        assert_eq!(imports[0].level, 0);
    }

    #[test]
    fn test_from_import() {
        let code = "from os.path import join, exists\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let imports = extract_imports(&tree, code);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "os.path");
    }

    #[test]
    fn test_relative_import() {
        let code = "from . import utils\nfrom ..models import User\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let imports = extract_imports(&tree, code);
        assert!(imports.len() >= 1);
        // Relative imports have level > 0
        assert!(imports.iter().any(|i| i.level > 0));
    }
}
