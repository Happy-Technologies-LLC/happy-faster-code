pub mod calls;
pub mod imports;
pub mod languages;

use languages::SupportedLanguage;
use std::collections::HashMap;
use tree_sitter::{Parser as TsParser, Tree};

/// Thread-safe parser that caches language instances.
pub struct Parser {
    parsers: HashMap<SupportedLanguage, TsParser>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    /// Parse source code for a given language.
    pub fn parse(&mut self, code: &str, language: SupportedLanguage) -> Option<Tree> {
        let parser = self.parsers.entry(language).or_insert_with(|| {
            let mut p = TsParser::new();
            p.set_language(&language.grammar())
                .expect("Failed to set language");
            p
        });
        parser.parse(code.as_bytes(), None)
    }

    /// Parse a file, detecting language from extension.
    pub fn parse_file(&mut self, path: &str, code: &str) -> Option<(SupportedLanguage, Tree)> {
        let lang = SupportedLanguage::from_extension(path)?;
        let tree = self.parse(code, lang)?;
        Some((lang, tree))
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_python() {
        let mut parser = Parser::new();
        let code = "def hello():\n    print('world')\n";
        let tree = parser.parse(code, SupportedLanguage::Python);
        assert!(tree.is_some());
        let tree = tree.unwrap();
        assert_eq!(tree.root_node().kind(), "module");
    }

    #[test]
    fn test_parse_file_detection() {
        let mut parser = Parser::new();
        let result = parser.parse_file("test.py", "x = 1");
        assert!(result.is_some());
        let (lang, _) = result.unwrap();
        assert_eq!(lang, SupportedLanguage::Python);
    }

    #[test]
    fn test_parse_unknown_extension() {
        let mut parser = Parser::new();
        let result = parser.parse_file("test.xyz", "x = 1");
        assert!(result.is_none());
    }
}
