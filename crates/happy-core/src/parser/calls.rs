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
    "abs", "all", "any", "bin", "bool", "breakpoint", "bytearray", "bytes",
    "callable", "chr", "classmethod", "compile", "complex", "delattr", "dict",
    "dir", "divmod", "enumerate", "eval", "exec", "filter", "float", "format",
    "frozenset", "getattr", "globals", "hasattr", "hash", "help", "hex", "id",
    "input", "int", "isinstance", "issubclass", "iter", "len", "list", "locals",
    "map", "max", "memoryview", "min", "next", "object", "oct", "open", "ord",
    "pow", "print", "property", "range", "repr", "reversed", "round", "set",
    "setattr", "slice", "sorted", "staticmethod", "str", "sum", "super",
    "tuple", "type", "vars", "zip",
];

/// Extract function calls from a tree-sitter AST with scope tracking.
pub fn extract_calls(tree: &Tree, code: &str) -> Vec<CallInfo> {
    let root = tree.root_node();
    let code_bytes = code.as_bytes();
    let scopes = extract_scopes(&root, code_bytes);
    let mut calls = Vec::new();

    extract_calls_recursive(&root, code, &scopes, &mut calls);
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
        "function_definition" | "function_declaration" => {
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
        "class_definition" | "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
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
    scopes: &[ScopeInfo],
    calls: &mut Vec<CallInfo>,
) {
    if node.kind() == "call" {
        if let Some(call) = process_call_node(node, code, scopes) {
            if !should_filter_call(&call) {
                calls.push(call);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_calls_recursive(&child, code, scopes, calls);
    }
}

fn process_call_node(
    node: &Node,
    code: &str,
    scopes: &[ScopeInfo],
) -> Option<CallInfo> {
    let function_node = node.child_by_field_name("function")?;
    let code_bytes = code.as_bytes();

    let (call_name, base_object, call_type) = match function_node.kind() {
        "identifier" => {
            let name = function_node.utf8_text(code_bytes).ok()?.to_string();
            (name, None, CallType::Simple)
        }
        "attribute" => {
            let object_node = function_node.child_by_field_name("object")?;
            let attr_node = function_node.child_by_field_name("attribute")?;
            let base = object_node.utf8_text(code_bytes).ok()?.to_string();
            let name = attr_node.utf8_text(code_bytes).ok()?.to_string();
            (name, Some(base), CallType::Attribute)
        }
        _ => return None,
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

fn find_scope_for_position(byte_pos: usize, scopes: &[ScopeInfo]) -> Option<String> {
    // Find the innermost scope containing this position
    for scope in scopes.iter().rev() {
        if scope.start_byte <= byte_pos && byte_pos < scope.end_byte {
            return Some(format!("{}::{}", scope.scope_type, scope.name));
        }
    }
    None
}

fn should_filter_call(call: &CallInfo) -> bool {
    if call.call_type == CallType::Simple {
        PYTHON_BUILTINS.contains(&call.call_name.as_str())
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use crate::parser::languages::SupportedLanguage;

    #[test]
    fn test_extract_simple_calls() {
        let code = r#"
def greet(name):
    result = process(name)
    return format_output(result)
"#;
        let mut parser = Parser::new();
        let tree = parser.parse(code, SupportedLanguage::Python).unwrap();
        let calls = extract_calls(&tree, code);

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
        let calls = extract_calls(&tree, code);

        let attr_calls: Vec<&CallInfo> = calls.iter()
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
        let calls = extract_calls(&tree, code);

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
        let calls = extract_calls(&tree, code);

        let call_names: Vec<&str> = calls.iter().map(|c| c.call_name.as_str()).collect();
        assert!(!call_names.contains(&"len"));
        assert!(!call_names.contains(&"print"));
        assert!(call_names.contains(&"custom_func"));
    }
}
