use crate::parser::languages::SupportedLanguage;
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

/// Extract import statements from a parsed AST, dispatching by language.
pub fn extract_imports(tree: &Tree, code: &str, language: SupportedLanguage) -> Vec<ImportInfo> {
    let root = tree.root_node();
    let mut imports = Vec::new();
    match language {
        SupportedLanguage::Python => collect_python_imports(&root, code, &mut imports),
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            collect_js_ts_imports(&root, code, &mut imports)
        }
        SupportedLanguage::Rust => collect_rust_imports(&root, code, &mut imports),
        SupportedLanguage::Go => collect_go_imports(&root, code, &mut imports),
        SupportedLanguage::Java => collect_java_imports(&root, code, &mut imports),
        SupportedLanguage::Cpp | SupportedLanguage::C => {
            collect_c_cpp_imports(&root, code, &mut imports)
        }
    }
    imports
}

// ── Python ─────────────────────────────────────────────────────

fn collect_python_imports(node: &Node, code: &str, imports: &mut Vec<ImportInfo>) {
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
                        let module = name_node
                            .utf8_text(code_bytes)
                            .unwrap_or_default()
                            .to_string();
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
        "import_from_statement" => {
            let mut module = String::new();
            let mut names = Vec::new();
            let mut level = 0u32;

            if let Some(module_node) = node.child_by_field_name("module_name") {
                module = module_node
                    .utf8_text(code_bytes)
                    .unwrap_or_default()
                    .to_string();
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
                    match kind {
                        "relative_import" => {
                            let mut rc = child.walk();
                            for rchild in child.children(&mut rc) {
                                match rchild.kind() {
                                    "import_prefix" => {
                                        let prefix =
                                            rchild.utf8_text(code_bytes).unwrap_or_default();
                                        level = prefix.chars().filter(|&c| c == '.').count() as u32;
                                    }
                                    "dotted_name" => {
                                        module = rchild
                                            .utf8_text(code_bytes)
                                            .unwrap_or_default()
                                            .to_string();
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
                    match kind {
                        "dotted_name" | "identifier" => {
                            let name = text.to_string();
                            if !name.is_empty() {
                                names.push(name);
                            }
                        }
                        "aliased_import" => {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = name_node
                                    .utf8_text(code_bytes)
                                    .unwrap_or_default()
                                    .to_string();
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
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_python_imports(&child, code, imports);
            }
        }
    }
}

// ── JavaScript / TypeScript ────────────────────────────────────

fn collect_js_ts_imports(node: &Node, code: &str, imports: &mut Vec<ImportInfo>) {
    let code_bytes = code.as_bytes();

    match node.kind() {
        // import { foo, bar } from "module"
        // import foo from "module"
        // import * as foo from "module"
        // import "module"
        "import_statement" => {
            let module = node
                .child_by_field_name("source")
                .and_then(|s| s.utf8_text(code_bytes).ok())
                .map(|s| s.trim_matches(|c| c == '\'' || c == '"').to_string())
                .unwrap_or_default();

            let mut names = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "import_clause" => {
                        collect_js_import_names(&child, code_bytes, &mut names);
                    }
                    "identifier" => {
                        // default import
                        let name = child.utf8_text(code_bytes).unwrap_or_default().to_string();
                        if name != "import" && name != "from" {
                            names.push(name);
                        }
                    }
                    _ => {}
                }
            }

            imports.push(ImportInfo {
                module,
                names,
                level: 0,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            });
        }
        // require("module")
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                if func.utf8_text(code_bytes).unwrap_or_default() == "require" {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        for arg in args.children(&mut cursor) {
                            if arg.kind() == "string" {
                                let module = arg
                                    .utf8_text(code_bytes)
                                    .unwrap_or_default()
                                    .trim_matches(|c| c == '\'' || c == '"')
                                    .to_string();
                                imports.push(ImportInfo {
                                    module,
                                    names: vec![],
                                    level: 0,
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                });
                            }
                        }
                    }
                }
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_js_ts_imports(&child, code, imports);
            }
        }
    }
}

fn collect_js_import_names(node: &Node, code_bytes: &[u8], names: &mut Vec<String>) {
    match node.kind() {
        "identifier" => {
            let name = node.utf8_text(code_bytes).unwrap_or_default().to_string();
            if !name.is_empty() {
                names.push(name);
            }
        }
        "import_specifier" => {
            // import { foo as bar } — use the original name "foo"
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(code_bytes).ok())
                .unwrap_or_default()
                .to_string();
            if !name.is_empty() {
                names.push(name);
            }
        }
        "namespace_import" => {
            // import * as foo — record as "*"
            names.push("*".to_string());
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_js_import_names(&child, code_bytes, names);
            }
        }
    }
}

// ── Rust ───────────────────────────────────────────────────────

fn collect_rust_imports(node: &Node, code: &str, imports: &mut Vec<ImportInfo>) {
    let code_bytes = code.as_bytes();

    match node.kind() {
        // use std::collections::HashMap;
        // use crate::parser::{calls, imports};
        "use_declaration" => {
            let mut names = Vec::new();
            let path = extract_rust_use_path(node, code_bytes, &mut names);

            imports.push(ImportInfo {
                module: path,
                names,
                level: 0,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            });
        }
        // mod foo; (external module declaration)
        "mod_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node
                    .utf8_text(code_bytes)
                    .unwrap_or_default()
                    .to_string();
                // Only count `mod foo;` (no body), not `mod foo { ... }`
                let has_body = node.child_by_field_name("body").is_some();
                if !has_body {
                    imports.push(ImportInfo {
                        module: name.clone(),
                        names: vec![name],
                        level: 0,
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                    });
                }
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_rust_imports(&child, code, imports);
            }
        }
    }
}

fn extract_rust_use_path(node: &Node, code_bytes: &[u8], names: &mut Vec<String>) -> String {
    // Walk the use_declaration to build the full path
    let mut path_parts = Vec::new();

    fn walk_use(node: &Node, code_bytes: &[u8], parts: &mut Vec<String>, names: &mut Vec<String>) {
        match node.kind() {
            "scoped_identifier" | "scoped_use_list" => {
                if let Some(path) = node.child_by_field_name("path") {
                    walk_use(&path, code_bytes, parts, names);
                }
                if let Some(name) = node.child_by_field_name("name") {
                    let text = name.utf8_text(code_bytes).unwrap_or_default().to_string();
                    parts.push(text.clone());
                    names.push(text);
                }
                // Handle use_list: use foo::{bar, baz}
                if let Some(list) = node.child_by_field_name("list") {
                    let mut cursor = list.walk();
                    for child in list.children(&mut cursor) {
                        if child.kind() == "identifier" || child.kind() == "scoped_identifier" {
                            let text = child.utf8_text(code_bytes).unwrap_or_default().to_string();
                            names.push(text);
                        } else if child.kind() == "use_as_clause" {
                            if let Some(orig) = child.child(0) {
                                let text =
                                    orig.utf8_text(code_bytes).unwrap_or_default().to_string();
                                names.push(text);
                            }
                        }
                    }
                }
            }
            "identifier" | "crate" | "self" | "super" => {
                let text = node.utf8_text(code_bytes).unwrap_or_default().to_string();
                parts.push(text);
            }
            "use_declaration" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() != "use" && child.kind() != ";" && child.kind() != "pub" {
                        walk_use(&child, code_bytes, parts, names);
                    }
                }
            }
            "use_wildcard" => {
                if let Some(path) = node.child_by_field_name("path") {
                    walk_use(&path, code_bytes, parts, names);
                }
                names.push("*".to_string());
            }
            "use_as_clause" => {
                if let Some(path) = node.child(0) {
                    walk_use(&path, code_bytes, parts, names);
                }
            }
            _ => {}
        }
    }

    walk_use(node, code_bytes, &mut path_parts, names);
    path_parts.join("::")
}

// ── Go ─────────────────────────────────────────────────────────

fn collect_go_imports(node: &Node, code: &str, imports: &mut Vec<ImportInfo>) {
    let code_bytes = code.as_bytes();

    match node.kind() {
        // import "fmt"
        // import ( "fmt" ; "os" )
        "import_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "import_spec" => {
                        let module = extract_go_import_path(&child, code_bytes);
                        let last_segment = module.rsplit('/').next().unwrap_or(&module).to_string();
                        imports.push(ImportInfo {
                            module: module.clone(),
                            names: vec![last_segment],
                            level: 0,
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                        });
                    }
                    "import_spec_list" => {
                        let mut inner_cursor = child.walk();
                        for spec in child.children(&mut inner_cursor) {
                            if spec.kind() == "import_spec" {
                                let module = extract_go_import_path(&spec, code_bytes);
                                let last_segment =
                                    module.rsplit('/').next().unwrap_or(&module).to_string();
                                imports.push(ImportInfo {
                                    module: module.clone(),
                                    names: vec![last_segment],
                                    level: 0,
                                    start_line: spec.start_position().row + 1,
                                    end_line: spec.end_position().row + 1,
                                });
                            }
                        }
                    }
                    "interpreted_string_literal" => {
                        let module = child
                            .utf8_text(code_bytes)
                            .unwrap_or_default()
                            .trim_matches('"')
                            .to_string();
                        let last_segment = module.rsplit('/').next().unwrap_or(&module).to_string();
                        imports.push(ImportInfo {
                            module: module.clone(),
                            names: vec![last_segment],
                            level: 0,
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_go_imports(&child, code, imports);
            }
        }
    }
}

fn extract_go_import_path(node: &Node, code_bytes: &[u8]) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "interpreted_string_literal" {
            return child
                .utf8_text(code_bytes)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
        }
    }
    node.utf8_text(code_bytes)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

// ── Java ───────────────────────────────────────────────────────

fn collect_java_imports(node: &Node, code: &str, imports: &mut Vec<ImportInfo>) {
    let code_bytes = code.as_bytes();

    match node.kind() {
        // import java.util.HashMap;
        // import java.util.*;
        "import_declaration" => {
            let full_text = node.utf8_text(code_bytes).unwrap_or_default();
            let module = full_text
                .trim()
                .trim_start_matches("import ")
                .trim_start_matches("static ")
                .trim_end_matches(';')
                .trim()
                .to_string();

            let last_part = module.rsplit('.').next().unwrap_or(&module).to_string();
            let names = vec![last_part];

            imports.push(ImportInfo {
                module,
                names,
                level: 0,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            });
        }
        // package statement — record for module resolution
        "package_declaration" => {
            let full_text = node.utf8_text(code_bytes).unwrap_or_default();
            let module = full_text
                .trim()
                .trim_start_matches("package ")
                .trim_end_matches(';')
                .trim()
                .to_string();
            imports.push(ImportInfo {
                module,
                names: vec![],
                level: 0,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
            });
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_java_imports(&child, code, imports);
            }
        }
    }
}

// ── C / C++ ────────────────────────────────────────────────────

fn collect_c_cpp_imports(node: &Node, code: &str, imports: &mut Vec<ImportInfo>) {
    let code_bytes = code.as_bytes();

    match node.kind() {
        // #include <stdio.h>
        // #include "local.h"
        "preproc_include" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "system_lib_string" | "string_literal" => {
                        let raw = child.utf8_text(code_bytes).unwrap_or_default();
                        let module = raw
                            .trim_matches(|c| c == '<' || c == '>' || c == '"')
                            .to_string();
                        imports.push(ImportInfo {
                            module: module.clone(),
                            names: vec![module],
                            level: 0,
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_c_cpp_imports(&child, code, imports);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    // ── Python tests ───────────────────────────────────────────

    #[test]
    fn test_python_simple_import() {
        let code = "import os\nimport sys\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Python);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].module, "os");
    }

    #[test]
    fn test_python_from_import() {
        let code = "from os.path import join, exists\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Python);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "os.path");
    }

    #[test]
    fn test_python_relative_import() {
        let code = "from . import utils\nfrom ..models import User\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Python);
        assert!(imports.len() >= 1);
        assert!(imports.iter().any(|i| i.level > 0));
    }

    // ── JavaScript tests ───────────────────────────────────────

    #[test]
    fn test_js_named_import() {
        let code = r#"import { foo, bar } from "module-name";"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::JavaScript).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::JavaScript);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "module-name");
        assert!(imports[0].names.contains(&"foo".to_string()));
        assert!(imports[0].names.contains(&"bar".to_string()));
    }

    #[test]
    fn test_js_default_import() {
        let code = r#"import React from "react";"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::JavaScript).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::JavaScript);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "react");
    }

    #[test]
    fn test_ts_import() {
        let code = r#"import { Component } from "@angular/core";
import * as path from "path";
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::TypeScript).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::TypeScript);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].module, "@angular/core");
        assert_eq!(imports[1].module, "path");
    }

    // ── Rust tests ─────────────────────────────────────────────

    #[test]
    fn test_rust_use_simple() {
        let code = "use std::collections::HashMap;\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Rust).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Rust);
        assert_eq!(imports.len(), 1);
        assert!(imports[0].module.contains("std"));
    }

    #[test]
    fn test_rust_mod_decl() {
        let code = "mod parser;\nmod graph;\n";
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Rust).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Rust);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].module, "parser");
        assert_eq!(imports[1].module, "graph");
    }

    // ── Go tests ───────────────────────────────────────────────

    #[test]
    fn test_go_single_import() {
        let code = r#"package main

import "fmt"
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Go).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Go);
        assert!(
            imports.iter().any(|i| i.module == "fmt"),
            "imports: {:?}",
            imports
        );
    }

    #[test]
    fn test_go_grouped_import() {
        let code = r#"package main

import (
    "fmt"
    "os"
)
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Go).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Go);
        let modules: Vec<&str> = imports.iter().map(|i| i.module.as_str()).collect();
        assert!(modules.contains(&"fmt"), "modules: {:?}", modules);
        assert!(modules.contains(&"os"), "modules: {:?}", modules);
    }

    // ── Java tests ─────────────────────────────────────────────

    #[test]
    fn test_java_import() {
        let code = r#"
import java.util.HashMap;
import java.util.*;

public class Main {}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Java).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Java);
        let modules: Vec<&str> = imports.iter().map(|i| i.module.as_str()).collect();
        assert!(
            modules.contains(&"java.util.HashMap"),
            "modules: {:?}",
            modules
        );
        assert!(modules.contains(&"java.util.*"), "modules: {:?}", modules);
    }

    // ── C/C++ tests ────────────────────────────────────────────

    #[test]
    fn test_cpp_include() {
        let code = r#"
#include <vector>
#include "local.h"

int main() { return 0; }
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Cpp).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::Cpp);
        let modules: Vec<&str> = imports.iter().map(|i| i.module.as_str()).collect();
        assert!(modules.contains(&"vector"), "modules: {:?}", modules);
        assert!(modules.contains(&"local.h"), "modules: {:?}", modules);
    }

    #[test]
    fn test_c_include() {
        let code = r#"
#include <stdio.h>

int main() { return 0; }
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::C).unwrap();
        let imports = extract_imports(&tree, code, SupportedLanguage::C);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "stdio.h");
    }
}
