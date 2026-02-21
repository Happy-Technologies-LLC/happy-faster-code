use super::languages::SupportedLanguage;
use tree_sitter::{Node, Tree};

/// Information about a function call extracted from source code.
#[derive(Debug, Clone)]
pub struct CallInfo {
    pub call_name: String,
    pub base_object: Option<String>,
    pub call_type: CallType,
    pub scope_id: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub node_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallType {
    Simple,
    Attribute,
}

/// Information about a scope (function or class definition).
#[derive(Debug, Clone)]
struct ScopeInfo {
    scope_type: &'static str,
    name: String,
    start_byte: usize,
    end_byte: usize,
}

/// Python built-in function names to filter out.
const PYTHON_BUILTINS: &[&str] = &[
    "abs",
    "all",
    "any",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
];

/// Extract function calls from a tree-sitter AST with scope tracking.
/// Dispatches to language-specific extraction for each supported language.
pub fn extract_calls(tree: &Tree, code: &str, language: SupportedLanguage) -> Vec<CallInfo> {
    let root = tree.root_node();
    let code_bytes = code.as_bytes();
    let scopes = extract_scopes(&root, code_bytes);
    let mut calls = Vec::new();

    extract_calls_recursive(&root, code, language, &scopes, &mut calls);
    calls
}

fn extract_scopes(node: &Node, code_bytes: &[u8]) -> Vec<ScopeInfo> {
    let mut scopes = Vec::new();
    collect_scopes(node, code_bytes, &mut scopes);
    scopes.sort_by_key(|s| s.start_byte);
    scopes
}

fn collect_scopes(node: &Node, code_bytes: &[u8], scopes: &mut Vec<ScopeInfo>) {
    match node.kind() {
        // Function scopes (all languages)
        "function_definition"       // Python
        | "function_declaration"    // JS/TS, Go
        | "function_item"           // Rust
        | "method_definition"       // JS/TS class methods
        | "method_declaration"      // Java, Go
        | "constructor_declaration" // Java
        => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(code_bytes).unwrap_or_default().to_string();
                scopes.push(ScopeInfo {
                    scope_type: "function",
                    name,
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                });
            }
        }
        // Class/type scopes (all languages)
        "class_definition"        // Python
        | "class_declaration"     // JS/TS, Java
        | "struct_item"           // Rust
        | "impl_item"             // Rust
        | "enum_item"             // Rust
        | "interface_declaration" // Java, TS
        | "enum_declaration"      // Java
        => {
            // Try "name" field first, then "type" (Rust impl_item uses "type")
            let name_node = node.child_by_field_name("name")
                .or_else(|| node.child_by_field_name("type"));
            if let Some(name_node) = name_node {
                let name = name_node.utf8_text(code_bytes).unwrap_or_default().to_string();
                scopes.push(ScopeInfo {
                    scope_type: "class",
                    name,
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                });
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_scopes(&child, code_bytes, scopes);
    }
}

fn extract_calls_recursive(
    node: &Node,
    code: &str,
    language: SupportedLanguage,
    scopes: &[ScopeInfo],
    calls: &mut Vec<CallInfo>,
) {
    let is_call = match language {
        SupportedLanguage::Python => node.kind() == "call",
        SupportedLanguage::Java => node.kind() == "method_invocation",
        _ => node.kind() == "call_expression", // JS/TS/Rust/Go/C/C++
    };

    if is_call {
        if let Some(call) = process_call_node(node, code, language, scopes) {
            if !should_filter_call(&call, language) {
                calls.push(call);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_calls_recursive(&child, code, language, scopes, calls);
    }
}

fn process_call_node(
    node: &Node,
    code: &str,
    language: SupportedLanguage,
    scopes: &[ScopeInfo],
) -> Option<CallInfo> {
    let code_bytes = code.as_bytes();

    // Java's method_invocation has no `function` field — extract directly from node
    let (call_name, base_object, call_type) = if language == SupportedLanguage::Java {
        extract_java_call(node, code_bytes)?
    } else {
        let function_node = node.child_by_field_name("function")?;
        match language {
            SupportedLanguage::Python => extract_python_call(&function_node, code_bytes)?,
            SupportedLanguage::JavaScript
            | SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx => extract_js_ts_call(&function_node, code_bytes)?,
            SupportedLanguage::Rust => extract_rust_call(&function_node, code_bytes)?,
            SupportedLanguage::Go => extract_go_call(&function_node, code_bytes)?,
            SupportedLanguage::C | SupportedLanguage::Cpp => {
                extract_c_cpp_call(&function_node, code_bytes)?
            }
            SupportedLanguage::Java => unreachable!(),
        }
    };

    let scope_id = find_scope_for_position(node.start_byte(), scopes);
    let node_text = node.utf8_text(code_bytes).unwrap_or_default().to_string();

    Some(CallInfo {
        call_name,
        base_object,
        call_type,
        scope_id,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        node_text,
    })
}

// ── Language-specific call extraction ──────────────────────────

/// Python: `call` → `function` field → `identifier` | `attribute`
fn extract_python_call(
    function_node: &Node,
    code_bytes: &[u8],
) -> Option<(String, Option<String>, CallType)> {
    match function_node.kind() {
        "identifier" => {
            let name = function_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, None, CallType::Simple))
        }
        "attribute" => {
            let object_node = function_node.child_by_field_name("object")?;
            let attr_node = function_node.child_by_field_name("attribute")?;
            let base = object_node.utf8_text(code_bytes).ok()?.to_string();
            let name = attr_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, Some(base), CallType::Attribute))
        }
        _ => None,
    }
}

/// JS/TS/TSX: `call_expression` → `function` field → `identifier` | `member_expression`
fn extract_js_ts_call(
    function_node: &Node,
    code_bytes: &[u8],
) -> Option<(String, Option<String>, CallType)> {
    match function_node.kind() {
        "identifier" => {
            let name = function_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, None, CallType::Simple))
        }
        "member_expression" => {
            let object_node = function_node.child_by_field_name("object")?;
            let property_node = function_node.child_by_field_name("property")?;
            let base = object_node.utf8_text(code_bytes).ok()?.to_string();
            let name = property_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, Some(base), CallType::Attribute))
        }
        _ => None,
    }
}

/// Rust: `call_expression` → `function` field → `identifier` | `field_expression` | `scoped_identifier`
fn extract_rust_call(
    function_node: &Node,
    code_bytes: &[u8],
) -> Option<(String, Option<String>, CallType)> {
    match function_node.kind() {
        "identifier" => {
            let name = function_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, None, CallType::Simple))
        }
        "field_expression" => {
            let value_node = function_node.child_by_field_name("value")?;
            let field_node = function_node.child_by_field_name("field")?;
            let base = value_node.utf8_text(code_bytes).ok()?.to_string();
            let name = field_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, Some(base), CallType::Attribute))
        }
        "scoped_identifier" => {
            // e.g., Vec::new() → name="new", base="Vec"
            let text = function_node.utf8_text(code_bytes).ok()?.to_string();
            if let Some(pos) = text.rfind("::") {
                let base = text[..pos].to_string();
                let name = text[pos + 2..].to_string();
                Some((name, Some(base), CallType::Attribute))
            } else {
                Some((text, None, CallType::Simple))
            }
        }
        _ => None,
    }
}

/// Go: `call_expression` → `function` field → `identifier` | `selector_expression`
fn extract_go_call(
    function_node: &Node,
    code_bytes: &[u8],
) -> Option<(String, Option<String>, CallType)> {
    match function_node.kind() {
        "identifier" => {
            let name = function_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, None, CallType::Simple))
        }
        "selector_expression" => {
            let operand_node = function_node.child_by_field_name("operand")?;
            let field_node = function_node.child_by_field_name("field")?;
            let base = operand_node.utf8_text(code_bytes).ok()?.to_string();
            let name = field_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, Some(base), CallType::Attribute))
        }
        _ => None,
    }
}

/// Java: `method_invocation` → direct `object` + `name` fields (no `function` wrapper)
fn extract_java_call(node: &Node, code_bytes: &[u8]) -> Option<(String, Option<String>, CallType)> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(code_bytes).ok()?.to_string();

    let base_object = node
        .child_by_field_name("object")
        .and_then(|obj| obj.utf8_text(code_bytes).ok())
        .map(|s| s.to_string());

    let call_type = if base_object.is_some() {
        CallType::Attribute
    } else {
        CallType::Simple
    };

    Some((name, base_object, call_type))
}

/// C/C++: `call_expression` → `function` field → `identifier` | `field_expression` | `qualified_identifier`
fn extract_c_cpp_call(
    function_node: &Node,
    code_bytes: &[u8],
) -> Option<(String, Option<String>, CallType)> {
    match function_node.kind() {
        "identifier" => {
            let name = function_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, None, CallType::Simple))
        }
        "field_expression" => {
            let argument_node = function_node.child_by_field_name("argument")?;
            let field_node = function_node.child_by_field_name("field")?;
            let base = argument_node.utf8_text(code_bytes).ok()?.to_string();
            let name = field_node.utf8_text(code_bytes).ok()?.to_string();
            Some((name, Some(base), CallType::Attribute))
        }
        "qualified_identifier" => {
            // C++: std::sort() → name="sort", base="std"
            let text = function_node.utf8_text(code_bytes).ok()?.to_string();
            if let Some(pos) = text.rfind("::") {
                let base = text[..pos].to_string();
                let name = text[pos + 2..].to_string();
                Some((name, Some(base), CallType::Attribute))
            } else {
                Some((text, None, CallType::Simple))
            }
        }
        _ => None,
    }
}

// ── Scope and filter helpers ──────────────────────────────────

fn find_scope_for_position(byte_pos: usize, scopes: &[ScopeInfo]) -> Option<String> {
    // Find the innermost scope containing this position
    for scope in scopes.iter().rev() {
        if scope.start_byte <= byte_pos && byte_pos < scope.end_byte {
            return Some(format!("{}::{}", scope.scope_type, scope.name));
        }
    }
    None
}

fn should_filter_call(call: &CallInfo, language: SupportedLanguage) -> bool {
    match language {
        SupportedLanguage::Python => {
            call.call_type == CallType::Simple && PYTHON_BUILTINS.contains(&call.call_name.as_str())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    // ── Python tests ──────────────────────────────────────────

    #[test]
    fn test_extract_simple_calls() {
        let code = r#"
def greet(name):
    result = process(name)
    return format_output(result)
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Python);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(call_names.contains(&"process"));
        assert!(call_names.contains(&"format_output"));
    }

    #[test]
    fn test_extract_attribute_calls() {
        let code = r#"
def run():
    self.loader.load()
    db.query("SELECT *")
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Python);

        let attr_calls: Vec<&CallInfo> = calls
            .iter()
            .filter(|c| c.call_type == CallType::Attribute)
            .collect();
        assert!(!attr_calls.is_empty());
    }

    #[test]
    fn test_scope_tracking() {
        let code = r#"
def outer():
    inner_call()

def another():
    other_call()
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Python);

        let inner = calls.iter().find(|c| c.call_name == "inner_call").unwrap();
        assert_eq!(inner.scope_id.as_deref(), Some("function::outer"));

        let other = calls.iter().find(|c| c.call_name == "other_call").unwrap();
        assert_eq!(other.scope_id.as_deref(), Some("function::another"));
    }

    #[test]
    fn test_filters_builtins() {
        let code = r#"
x = len([1, 2, 3])
y = print("hello")
z = custom_func()
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Python);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(!call_names.contains(&"len"));
        assert!(!call_names.contains(&"print"));
        assert!(call_names.contains(&"custom_func"));
    }

    // ── JavaScript tests ──────────────────────────────────────

    #[test]
    fn test_extract_js_calls() {
        let code = r#"
function main() {
    process();
    console.log("hello");
    arr.map(x => x + 1);
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::JavaScript).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::JavaScript);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(
            call_names.contains(&"process"),
            "Missing 'process' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"log"),
            "Missing 'log' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"map"),
            "Missing 'map' in {:?}",
            call_names
        );

        // Verify attribute calls have correct base
        let log_call = calls.iter().find(|c| c.call_name == "log").unwrap();
        assert_eq!(log_call.base_object.as_deref(), Some("console"));
        assert_eq!(log_call.call_type, CallType::Attribute);
    }

    // ── TypeScript tests ──────────────────────────────────────

    #[test]
    fn test_extract_ts_calls() {
        let code = r#"
async function fetchData() {
    const response = await fetch("/api/data");
    const data = await response.json();
    return data;
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::TypeScript).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::TypeScript);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(
            call_names.contains(&"fetch"),
            "Missing 'fetch' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"json"),
            "Missing 'json' in {:?}",
            call_names
        );

        let json_call = calls.iter().find(|c| c.call_name == "json").unwrap();
        assert_eq!(json_call.base_object.as_deref(), Some("response"));
    }

    // ── Rust tests ────────────────────────────────────────────

    #[test]
    fn test_extract_rust_calls() {
        let code = r#"
fn main() {
    process();
    let mut v = Vec::new();
    v.push(42);
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Rust).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Rust);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(
            call_names.contains(&"process"),
            "Missing 'process' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"new"),
            "Missing 'new' (Vec::new) in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"push"),
            "Missing 'push' in {:?}",
            call_names
        );

        // Verify Vec::new() is extracted as attribute call
        let new_call = calls.iter().find(|c| c.call_name == "new").unwrap();
        assert_eq!(new_call.base_object.as_deref(), Some("Vec"));
    }

    // ── Go tests ──────────────────────────────────────────────

    #[test]
    fn test_extract_go_calls() {
        let code = r#"
package main

import "fmt"

func main() {
    process()
    fmt.Println("hello")
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Go).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Go);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(
            call_names.contains(&"process"),
            "Missing 'process' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"Println"),
            "Missing 'Println' in {:?}",
            call_names
        );

        let println_call = calls.iter().find(|c| c.call_name == "Println").unwrap();
        assert_eq!(println_call.base_object.as_deref(), Some("fmt"));
    }

    // ── Java tests ────────────────────────────────────────────

    #[test]
    fn test_extract_java_calls() {
        let code = r#"
public class Main {
    public void run() {
        process();
        System.out.println("hello");
        list.add("item");
    }
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Java).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Java);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(
            call_names.contains(&"process"),
            "Missing 'process' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"println"),
            "Missing 'println' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"add"),
            "Missing 'add' in {:?}",
            call_names
        );

        let add_call = calls.iter().find(|c| c.call_name == "add").unwrap();
        assert_eq!(add_call.base_object.as_deref(), Some("list"));
    }

    // ── C++ tests ─────────────────────────────────────────────

    #[test]
    fn test_extract_cpp_calls() {
        let code = r#"
#include <vector>
#include <iostream>

void run() {
    process();
    std::vector<int> vec;
    vec.push_back(42);
}
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Cpp).unwrap();
        let calls = extract_calls(&tree, code, SupportedLanguage::Cpp);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(
            call_names.contains(&"process"),
            "Missing 'process' in {:?}",
            call_names
        );
        assert!(
            call_names.contains(&"push_back"),
            "Missing 'push_back' in {:?}",
            call_names
        );

        let push_call = calls.iter().find(|c| c.call_name == "push_back").unwrap();
        assert_eq!(push_call.base_object.as_deref(), Some("vec"));
    }
}
