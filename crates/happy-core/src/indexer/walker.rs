use std::path::{Path, PathBuf};
use ignore::WalkBuilder;
use rayon::prelude::*;

use crate::parser::Parser;
use crate::parser::languages::SupportedLanguage;
use super::element::{CodeElement, ElementType};
use std::collections::HashMap;
use std::sync::Mutex;

/// Walk a repository and extract code elements from all supported files.
pub fn walk_and_index(repo_path: &str) -> Vec<CodeElement> {
    let repo_root = Path::new(repo_path).canonicalize().unwrap_or_else(|_| PathBuf::from(repo_path));
    let repo_root_str = repo_root.to_string_lossy().to_string();

    // Collect all file paths first
    let files: Vec<PathBuf> = WalkBuilder::new(&repo_root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
        .filter(|entry| SupportedLanguage::from_extension(&entry.path().to_string_lossy()).is_some())
        .map(|entry| entry.into_path())
        .collect();

    // Process files in parallel with rayon
    let elements: Mutex<Vec<CodeElement>> = Mutex::new(Vec::new());

    files.par_iter().for_each(|path| {
        let path_str = path.to_string_lossy().to_string();
        if let Ok(code) = std::fs::read_to_string(path) {
            let mut parser = Parser::new();
            if let Some((lang, tree)) = parser.parse_file(&path_str, &code) {
                let relative = path.strip_prefix(&repo_root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path_str.clone());

                let file_elements = extract_elements_from_tree(
                    &tree,
                    &code,
                    &path_str,
                    &relative,
                    lang,
                    &repo_root_str,
                );

                if let Ok(mut elems) = elements.lock() {
                    elems.extend(file_elements);
                }
            }
        }
    });

    elements.into_inner().unwrap_or_default()
}

/// Index a single file and return its code elements.
/// Used for incremental re-indexing when a file changes during a session.
pub fn index_single_file(file_path: &str, repo_root: &str) -> Option<Vec<CodeElement>> {
    let path = Path::new(file_path);
    let root = Path::new(repo_root);

    let lang = SupportedLanguage::from_extension(&path.to_string_lossy())?;
    let code = std::fs::read_to_string(path).ok()?;

    let mut parser = Parser::new();
    let tree = parser.parse(&code, lang)?;

    let relative = path.strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());

    Some(extract_elements_from_tree(
        &tree,
        &code,
        file_path,
        &relative,
        lang,
        repo_root,
    ))
}

/// Extract code elements from a parsed tree-sitter AST.
pub fn extract_elements_from_tree(
    tree: &tree_sitter::Tree,
    code: &str,
    file_path: &str,
    relative_path: &str,
    language: SupportedLanguage,
    _repo_root: &str,
) -> Vec<CodeElement> {
    let mut elements = Vec::new();
    let code_bytes = code.as_bytes();
    let lang_str = language.name().to_string();

    // File-level element
    let file_id = CodeElement::generate_id("file", &[relative_path]);
    elements.push(CodeElement {
        id: file_id,
        element_type: ElementType::File,
        name: Path::new(file_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default(),
        file_path: file_path.to_string(),
        relative_path: relative_path.to_string(),
        language: lang_str.clone(),
        start_line: 1,
        end_line: code.lines().count(),
        code: code.to_string(),
        signature: None,
        docstring: None,
        summary: None,
        metadata: HashMap::new(),
    });

    // Walk the AST for class/function definitions
    let root = tree.root_node();
    collect_definitions(
        &root,
        code_bytes,
        code,
        file_path,
        relative_path,
        &lang_str,
        language,
        &mut elements,
        None,
    );

    elements
}

/// Check if a tree-sitter node kind represents a function/method definition.
fn is_function_kind(kind: &str) -> bool {
    matches!(kind,
        // Python
        "function_definition" |
        // JS/TS
        "function_declaration" |
        // JS/TS class methods
        "method_definition" |
        // Rust
        "function_item" |
        // Go/Java methods
        "method_declaration" |
        // Java constructors
        "constructor_declaration"
    )
}

/// Check if a tree-sitter node kind represents a class/struct/enum/interface definition.
fn is_class_like_kind(kind: &str) -> bool {
    matches!(kind,
        // Python/JS/TS/Java
        "class_definition" | "class_declaration" |
        // Rust
        "struct_item" | "enum_item" | "impl_item" |
        // C/C++
        "struct_specifier" | "enum_specifier" |
        // TS/Java
        "interface_declaration" | "enum_declaration"
    )
}

/// Determine the ElementType for a class-like node kind.
fn classify_class_kind(kind: &str) -> ElementType {
    match kind {
        "struct_item" | "struct_specifier" => ElementType::Struct,
        "enum_item" | "enum_specifier" | "enum_declaration" => ElementType::Enum,
        "interface_declaration" => ElementType::Interface,
        "impl_item" => ElementType::Class,
        _ => ElementType::Class,
    }
}

/// Try to extract the name from a definition node, handling language-specific patterns.
fn extract_name(node: &tree_sitter::Node, code_bytes: &[u8]) -> Option<String> {
    // Most languages use a "name" field
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(name_node.utf8_text(code_bytes).unwrap_or_default().to_string());
    }

    // Rust impl_item uses "type" field for the implemented type
    if node.kind() == "impl_item" {
        if let Some(type_node) = node.child_by_field_name("type") {
            return Some(type_node.utf8_text(code_bytes).unwrap_or_default().to_string());
        }
    }

    // C/C++ function_definition uses "declarator" field
    if node.kind() == "function_definition" {
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return extract_declarator_name(&declarator, code_bytes);
        }
    }

    None
}

/// Extract function name from a C/C++ declarator node (which may be nested).
fn extract_declarator_name(node: &tree_sitter::Node, code_bytes: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "type_identifier" => {
            Some(node.utf8_text(code_bytes).unwrap_or_default().to_string())
        }
        "function_declarator" | "pointer_declarator" | "parenthesized_declarator" => {
            if let Some(inner) = node.child_by_field_name("declarator") {
                extract_declarator_name(&inner, code_bytes)
            } else {
                node.child(0).and_then(|c| extract_declarator_name(&c, code_bytes))
            }
        }
        "qualified_identifier" | "scoped_identifier" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                Some(name_node.utf8_text(code_bytes).unwrap_or_default().to_string())
            } else {
                Some(node.utf8_text(code_bytes).unwrap_or_default().to_string())
            }
        }
        _ => {
            node.child_by_field_name("declarator")
                .and_then(|c| extract_declarator_name(&c, code_bytes))
        }
    }
}

fn collect_definitions(
    node: &tree_sitter::Node,
    code_bytes: &[u8],
    code: &str,
    file_path: &str,
    relative_path: &str,
    language: &str,
    lang_enum: SupportedLanguage,
    elements: &mut Vec<CodeElement>,
    parent_class: Option<&str>,
) {
    let kind = node.kind();

    if is_function_kind(kind) {
        if let Some(name) = extract_name(node, code_bytes) {
            let element_type = if parent_class.is_some() {
                ElementType::Method
            } else {
                ElementType::Function
            };

            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;
            let node_code = node.utf8_text(code_bytes).unwrap_or_default().to_string();
            let signature = node_code.lines().next().map(|s| s.to_string());
            let docstring = extract_docstring(node, code_bytes, lang_enum);

            let id_parts: Vec<&str> = if let Some(cls) = parent_class {
                vec![relative_path, cls, &name]
            } else {
                vec![relative_path, &name]
            };
            let id = CodeElement::generate_id(element_type.as_str(), &id_parts);

            elements.push(CodeElement {
                id,
                element_type,
                name,
                file_path: file_path.to_string(),
                relative_path: relative_path.to_string(),
                language: language.to_string(),
                start_line,
                end_line,
                code: node_code,
                signature,
                docstring,
                summary: None,
                metadata: HashMap::new(),
            });
        }
    } else if is_class_like_kind(kind) {
        if let Some(name) = extract_name(node, code_bytes) {
            let element_type = classify_class_kind(kind);

            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;
            let node_code = node.utf8_text(code_bytes).unwrap_or_default().to_string();
            let signature = node_code.lines().next().map(|s| s.to_string());
            let docstring = extract_docstring(node, code_bytes, lang_enum);

            let id = CodeElement::generate_id(element_type.as_str(), &[relative_path, &name]);

            elements.push(CodeElement {
                id,
                element_type,
                name: name.clone(),
                file_path: file_path.to_string(),
                relative_path: relative_path.to_string(),
                language: language.to_string(),
                start_line,
                end_line,
                code: node_code,
                signature,
                docstring,
                summary: None,
                metadata: HashMap::new(),
            });

            // Recurse into class/struct/impl body to find methods
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_definitions(
                    &child,
                    code_bytes,
                    code,
                    file_path,
                    relative_path,
                    language,
                    lang_enum,
                    elements,
                    Some(&name),
                );
            }
            return;
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_definitions(
            &child,
            code_bytes,
            code,
            file_path,
            relative_path,
            language,
            lang_enum,
            elements,
            parent_class,
        );
    }
}

/// Try to extract a docstring from a function/class body.
fn extract_docstring(
    node: &tree_sitter::Node,
    code_bytes: &[u8],
    language: SupportedLanguage,
) -> Option<String> {
    match language {
        SupportedLanguage::Python => extract_python_docstring(node, code_bytes),
        SupportedLanguage::Rust => extract_rust_doc_comment(node, code_bytes),
        _ => extract_comment_doc(node, code_bytes),
    }
}

/// Python docstring: triple-quoted string as first expression in body.
fn extract_python_docstring(node: &tree_sitter::Node, code_bytes: &[u8]) -> Option<String> {
    let body = node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "expression_statement" {
            if let Some(string_node) = child.child(0) {
                if string_node.kind() == "string" || string_node.kind() == "concatenated_string" {
                    let text = string_node.utf8_text(code_bytes).unwrap_or_default();
                    let trimmed = text
                        .trim_start_matches("\"\"\"")
                        .trim_start_matches("'''")
                        .trim_end_matches("\"\"\"")
                        .trim_end_matches("'''")
                        .trim();
                    return Some(trimmed.to_string());
                }
            }
        }
        break;
    }
    None
}

/// Rust doc comments: /// or //! preceding the item.
fn extract_rust_doc_comment(node: &tree_sitter::Node, code_bytes: &[u8]) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(sibling) = prev {
        if sibling.kind() == "line_comment" {
            let text = sibling.utf8_text(code_bytes).unwrap_or_default();
            if text.starts_with("///") || text.starts_with("//!") {
                let content = text
                    .trim_start_matches("///")
                    .trim_start_matches("//!")
                    .trim_start();
                doc_lines.push(content.to_string());
            } else {
                break;
            }
        } else {
            break;
        }
        prev = sibling.prev_sibling();
    }

    if doc_lines.is_empty() {
        return None;
    }

    doc_lines.reverse();
    Some(doc_lines.join("\n"))
}

/// Generic comment-based doc extraction for JS/TS/Java/Go/C++.
fn extract_comment_doc(node: &tree_sitter::Node, code_bytes: &[u8]) -> Option<String> {
    let mut prev = node.prev_sibling();
    while let Some(sibling) = prev {
        match sibling.kind() {
            "comment" | "block_comment" => {
                let text = sibling.utf8_text(code_bytes).unwrap_or_default();
                if text.starts_with("/**") || text.starts_with("/*") {
                    let cleaned = text
                        .trim_start_matches("/**")
                        .trim_start_matches("/*")
                        .trim_end_matches("*/")
                        .lines()
                        .map(|l| l.trim().trim_start_matches('*').trim())
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    return Some(cleaned);
                } else if text.starts_with("//") {
                    let content = text.trim_start_matches("//").trim();
                    return Some(content.to_string());
                }
            }
            _ => break,
        }
        prev = sibling.prev_sibling();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[test]
    fn test_extract_elements_python() {
        let code = r#"
class MyClass:
    """A test class."""
    def method(self, x):
        """Do something."""
        return x

def standalone():
    pass
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let elements = extract_elements_from_tree(
            &tree, code, "test.py", "test.py", SupportedLanguage::Python, "/repo",
        );

        let types: Vec<ElementType> = elements.iter().map(|e| e.element_type).collect();
        assert!(types.contains(&ElementType::File));
        assert!(types.contains(&ElementType::Class));
        assert!(types.contains(&ElementType::Method));
        assert!(types.contains(&ElementType::Function));
    }

    #[test]
    fn test_extract_docstring() {
        let code = r#"
def foo():
    """This is a docstring."""
    pass
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let elements = extract_elements_from_tree(
            &tree, code, "test.py", "test.py", SupportedLanguage::Python, "/repo",
        );

        let func = elements.iter().find(|e| e.name == "foo").unwrap();
        assert_eq!(func.docstring.as_deref(), Some("This is a docstring."));
    }

    #[test]
    fn test_extract_rust_functions() {
        let code = r#"
/// Documentation for greet.
fn greet(name: &str) -> String {
    format!("Hello, {}", name)
}

struct Config {
    verbose: bool,
}

impl Config {
    fn new() -> Self {
        Self { verbose: false }
    }
}

enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Rust).unwrap();
        let elements = extract_elements_from_tree(
            &tree, code, "test.rs", "test.rs", SupportedLanguage::Rust, "/repo",
        );

        let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"greet"), "Should find fn greet: {:?}", names);
        assert!(names.contains(&"Config"), "Should find struct Config: {:?}", names);
        assert!(names.contains(&"Color"), "Should find enum Color: {:?}", names);
        assert!(names.contains(&"new"), "Should find method new: {:?}", names);

        let greet = elements.iter().find(|e| e.name == "greet").unwrap();
        assert_eq!(greet.element_type, ElementType::Function);

        let config = elements.iter().find(|e| e.name == "Config").unwrap();
        assert_eq!(config.element_type, ElementType::Struct);

        let color = elements.iter().find(|e| e.name == "Color").unwrap();
        assert_eq!(color.element_type, ElementType::Enum);

        let new_method = elements.iter().find(|e| e.name == "new").unwrap();
        assert_eq!(new_method.element_type, ElementType::Method);

        // Check doc extraction
        assert!(greet.docstring.is_some(), "greet should have a doc comment");
        assert!(greet.docstring.as_deref().unwrap().contains("Documentation for greet"));
    }

    #[test]
    fn test_extract_javascript_elements() {
        let code = r#"
function processData(data) {
    return data.map(x => x * 2);
}

class DataService {
    constructor(url) {
        this.url = url;
    }

    fetch() {
        return this.url;
    }
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::JavaScript).unwrap();
        let elements = extract_elements_from_tree(
            &tree, code, "test.js", "test.js", SupportedLanguage::JavaScript, "/repo",
        );

        let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"processData"), "Should find processData: {:?}", names);
        assert!(names.contains(&"DataService"), "Should find DataService: {:?}", names);
    }

    #[test]
    fn test_extract_typescript_elements() {
        let code = r#"
interface UserService {
    getUser(id: string): User;
}

enum Status {
    Active,
    Inactive,
}

function createUser(name: string): User {
    return { name };
}

class UserManager {
    private users: User[] = [];

    add(user: User): void {
        this.users.push(user);
    }
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::TypeScript).unwrap();
        let elements = extract_elements_from_tree(
            &tree, code, "test.ts", "test.ts", SupportedLanguage::TypeScript, "/repo",
        );

        let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"UserService"), "Should find interface: {:?}", names);
        assert!(names.contains(&"Status"), "Should find enum: {:?}", names);
        assert!(names.contains(&"createUser"), "Should find function: {:?}", names);
        assert!(names.contains(&"UserManager"), "Should find class: {:?}", names);

        let iface = elements.iter().find(|e| e.name == "UserService").unwrap();
        assert_eq!(iface.element_type, ElementType::Interface);

        let status = elements.iter().find(|e| e.name == "Status").unwrap();
        assert_eq!(status.element_type, ElementType::Enum);
    }

    #[test]
    fn test_extract_go_elements() {
        let code = r#"
package main

func Hello(name string) string {
    return "Hello, " + name
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Go).unwrap();
        let elements = extract_elements_from_tree(
            &tree, code, "test.go", "test.go", SupportedLanguage::Go, "/repo",
        );

        let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Hello"), "Should find func Hello: {:?}", names);
    }

    #[test]
    fn test_extract_java_elements() {
        let code = r#"
public class UserService {
    public void processUser(String name) {
        System.out.println(name);
    }

    private int calculate(int x) {
        return x * 2;
    }
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Java).unwrap();
        let elements = extract_elements_from_tree(
            &tree, code, "Test.java", "Test.java", SupportedLanguage::Java, "/repo",
        );

        let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"UserService"), "Should find class: {:?}", names);
        assert!(names.contains(&"processUser"), "Should find method: {:?}", names);
        assert!(names.contains(&"calculate"), "Should find method: {:?}", names);
    }

    #[test]
    fn test_index_single_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            writeln!(f, "def hello():").unwrap();
            writeln!(f, "    return 'world'").unwrap();
        }

        let file_str = file_path.to_string_lossy().to_string();
        let repo_root = dir.path().to_string_lossy().to_string();
        let elements = index_single_file(&file_str, &repo_root).unwrap();

        // Should have a File element + a Function element
        assert!(elements.len() >= 2, "Expected at least 2 elements, got {}", elements.len());

        let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"hello"), "Should find function hello: {:?}", names);

        let types: Vec<ElementType> = elements.iter().map(|e| e.element_type).collect();
        assert!(types.contains(&ElementType::File));
        assert!(types.contains(&ElementType::Function));
    }
}
