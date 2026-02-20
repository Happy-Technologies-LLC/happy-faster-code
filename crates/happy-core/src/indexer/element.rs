use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The type of a code element extracted from source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElementType {
    File,
    Class,
    Function,
    Method,
    Module,
    Import,
    Variable,
    Interface,
    Struct,
    Enum,
}

impl ElementType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Class => "class",
            Self::Function => "function",
            Self::Method => "method",
            Self::Module => "module",
            Self::Import => "import",
            Self::Variable => "variable",
            Self::Interface => "interface",
            Self::Struct => "struct",
            Self::Enum => "enum",
        }
    }
}

/// A code element extracted from source code.
/// Mirrors FastCode's CodeElement dataclass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeElement {
    pub id: String,
    pub element_type: ElementType,
    pub name: String,
    pub file_path: String,
    pub relative_path: String,
    pub language: String,
    pub start_line: usize,
    pub end_line: usize,
    pub code: String,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub summary: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl CodeElement {
    /// Generate a deterministic ID for this element.
    pub fn generate_id(type_: &str, parts: &[&str]) -> String {
        crate::utils::generate_element_id(type_, parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_type_str() {
        assert_eq!(ElementType::Function.as_str(), "function");
        assert_eq!(ElementType::Class.as_str(), "class");
    }

    #[test]
    fn test_generate_id() {
        let id = CodeElement::generate_id("function", &["src/main.py", "MyClass", "process"]);
        assert!(id.starts_with("function_"));
        // Same inputs produce same ID
        let id2 = CodeElement::generate_id("function", &["src/main.py", "MyClass", "process"]);
        assert_eq!(id, id2);
    }
}
