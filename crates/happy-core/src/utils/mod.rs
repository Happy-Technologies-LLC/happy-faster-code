use std::path::Path;

/// Normalize a file path for consistent lookups.
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

/// Convert a file path to a dotted module path (Python-style).
/// e.g., "src/app/services/auth.py" -> "src.app.services.auth"
pub fn file_path_to_module_path(file_path: &str, repo_root: &str) -> Option<String> {
    let path = Path::new(file_path);
    let root = Path::new(repo_root);

    let relative = path.strip_prefix(root).ok()?;
    let relative_str = relative.to_str()?;

    // Remove file extension
    let without_ext = relative_str
        .strip_suffix(".py")
        .or_else(|| relative_str.strip_suffix(".ts"))
        .or_else(|| relative_str.strip_suffix(".js"))
        .or_else(|| relative_str.strip_suffix(".rs"))
        .or_else(|| relative_str.strip_suffix(".go"))
        .or_else(|| relative_str.strip_suffix(".java"))
        .unwrap_or(relative_str);

    // Convert path separators to dots
    let module_path = without_ext.replace(['/', '\\'], ".");

    // Handle __init__.py -> strip trailing .__init__
    let module_path = module_path
        .strip_suffix(".__init__")
        .unwrap_or(&module_path)
        .to_string();

    if module_path.is_empty() {
        None
    } else {
        Some(module_path)
    }
}

/// Generate a deterministic element ID using blake3 hash.
pub fn generate_element_id(type_: &str, parts: &[&str]) -> String {
    let unique_string = format!("{}/{}", type_, parts.join("/"));
    let hash = blake3::hash(unique_string.as_bytes());
    let hex = hash.to_hex();
    format!("{}_{}", type_, &hex[..16])
}

/// Simple whitespace tokenizer for BM25.
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_path_to_module_path() {
        assert_eq!(
            file_path_to_module_path("/repo/src/app/auth.py", "/repo"),
            Some("src.app.auth".to_string())
        );
        assert_eq!(
            file_path_to_module_path("/repo/src/app/__init__.py", "/repo"),
            Some("src.app".to_string())
        );
    }

    #[test]
    fn test_generate_element_id() {
        let id = generate_element_id("function", &["src/main.py", "MyClass", "process"]);
        assert!(id.starts_with("function_"));
        assert_eq!(id.len(), "function_".len() + 16);
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello World FOO");
        assert_eq!(tokens, vec!["hello", "world", "foo"]);
    }
}
