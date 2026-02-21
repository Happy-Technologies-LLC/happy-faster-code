use std::path::Path;
use tree_sitter::Language;

/// Supported programming languages for parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Rust,
    Go,
    Java,
    Cpp,
    C,
}

impl SupportedLanguage {
    /// Detect language from file extension.
    pub fn from_extension(path: &str) -> Option<Self> {
        let ext = Path::new(path).extension()?.to_str()?;
        match ext {
            "py" | "pyi" => Some(Self::Python),
            "js" | "mjs" | "cjs" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" | "jsx" => Some(Self::Tsx),
            "rs" => Some(Self::Rust),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "h" => Some(Self::Cpp),
            "c" => Some(Self::C),
            _ => None,
        }
    }

    /// Get the tree-sitter Language grammar for this language.
    pub fn grammar(&self) -> Language {
        match self {
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
        }
    }

    /// Get language name as string.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Java => "java",
            Self::Cpp => "cpp",
            Self::C => "c",
        }
    }
}

impl std::fmt::Display for SupportedLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension() {
        assert_eq!(
            SupportedLanguage::from_extension("foo.py"),
            Some(SupportedLanguage::Python)
        );
        assert_eq!(
            SupportedLanguage::from_extension("bar.ts"),
            Some(SupportedLanguage::TypeScript)
        );
        assert_eq!(
            SupportedLanguage::from_extension("baz.rs"),
            Some(SupportedLanguage::Rust)
        );
        assert_eq!(SupportedLanguage::from_extension("qux.txt"), None);
    }

    #[test]
    fn test_grammar_loads() {
        // Verify all grammars can be loaded
        for lang in [
            SupportedLanguage::Python,
            SupportedLanguage::JavaScript,
            SupportedLanguage::TypeScript,
            SupportedLanguage::Rust,
            SupportedLanguage::Go,
            SupportedLanguage::Java,
            SupportedLanguage::Cpp,
            SupportedLanguage::C,
        ] {
            let _grammar = lang.grammar();
        }
    }
}
