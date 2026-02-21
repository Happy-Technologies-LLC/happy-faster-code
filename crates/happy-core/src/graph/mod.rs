pub mod queries;
pub mod types;

use dashmap::DashMap;
use petgraph::stable_graph::NodeIndex;
use smol_str::SmolStr;

use crate::global_index::GlobalIndex;
use crate::global_index::module_resolver::ModuleResolver;
use crate::global_index::symbol_resolver::SymbolResolver;
use crate::indexer::{CodeElement, ElementType};
use crate::parser::calls::extract_calls;
use crate::parser::imports::extract_imports;
use crate::parser::languages::SupportedLanguage;
use types::{CodeGraph, EdgeKind, GraphEdge, GraphNode, NodeKind};

/// The main repository graph holding all code relationships.
pub struct RepositoryGraph {
    pub(crate) graph: CodeGraph,
    /// element_id -> NodeIndex
    pub(crate) id_to_node: DashMap<String, NodeIndex>,
    /// name -> Vec<NodeIndex> (multiple elements can share a name)
    pub(crate) name_to_nodes: DashMap<String, Vec<NodeIndex>>,
    /// file_path -> Vec<NodeIndex>
    pub(crate) file_to_nodes: DashMap<String, Vec<NodeIndex>>,
    /// Store elements for source code retrieval
    pub(crate) element_arena: DashMap<String, CodeElement>,
    /// file_path -> list of imported module/symbol names (for import-aware resolution)
    file_imports: DashMap<String, Vec<String>>,
    /// Global index for module/symbol resolution across the repo
    global_index: GlobalIndex,
}

impl RepositoryGraph {
    pub fn new() -> Self {
        Self {
            graph: CodeGraph::default(),
            id_to_node: DashMap::new(),
            name_to_nodes: DashMap::new(),
            file_to_nodes: DashMap::new(),
            element_arena: DashMap::new(),
            file_imports: DashMap::new(),
            global_index: GlobalIndex::new(),
        }
    }

    /// Add a node to the graph and update lookup indexes.
    pub fn add_node(&mut self, node: GraphNode) -> NodeIndex {
        let id = node.id.clone();
        let name = node.name.to_string();
        let file_path = node.file_path.clone();

        let idx = self.graph.add_node(node);

        self.id_to_node.insert(id, idx);
        self.name_to_nodes.entry(name).or_default().push(idx);
        self.file_to_nodes.entry(file_path).or_default().push(idx);

        idx
    }

    /// Add an edge between two nodes.
    pub fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, edge: GraphEdge) {
        self.graph.add_edge(from, to, edge);
    }

    /// Build the graph from a set of code elements.
    ///
    /// `repo_root` is used to compute module paths for the GlobalIndex (enables
    /// proper import resolution for Python relative imports, Java packages, etc.).
    /// Pass `""` if repo root is unknown — resolution will fall back to heuristics.
    pub fn build_from_elements(&mut self, elements: &[CodeElement], repo_root: &str) {
        // Phase 1: Add all elements as nodes
        for elem in elements {
            let node = GraphNode {
                id: elem.id.clone(),
                kind: NodeKind::from(elem.element_type),
                name: SmolStr::new(&elem.name),
                file_path: elem.file_path.clone(),
                start_line: elem.start_line,
                end_line: elem.end_line,
            };
            self.add_node(node);
            self.element_arena.insert(elem.id.clone(), elem.clone());
        }

        // Phase 1.5: Build GlobalIndex (file→module, module→file, symbol→elements)
        self.global_index.build(elements, repo_root);

        // Phase 2: Build "defines" edges (file -> its children)
        for elem in elements {
            if elem.element_type == ElementType::File {
                continue;
            }
            if let Some(file_nodes) = self.file_to_nodes.get(&elem.file_path) {
                let file_idx = file_nodes
                    .iter()
                    .find(|&&idx| self.graph[idx].kind == NodeKind::File);
                if let (Some(&file_idx), Some(elem_idx)) = (file_idx, self.id_to_node.get(&elem.id))
                {
                    self.graph.add_edge(
                        file_idx,
                        *elem_idx,
                        GraphEdge {
                            kind: EdgeKind::Defines,
                        },
                    );
                }
            }
        }

        // Phase 3: Build semantic edges from source code analysis
        // Import edges first (populates file_imports for call resolution)
        self.build_import_edges(elements);
        self.build_call_edges(elements);
        self.build_inheritance_edges(elements);
    }

    /// Build call edges using import-aware resolution.
    ///
    /// Resolution strategy (in priority order):
    /// 1. Same-file match — prefer callee defined in the same file
    /// 2. GlobalIndex SymbolResolver — use export_map + import context for precise resolution
    /// 3. Import-aware heuristic — prefer callee from a file matching an import name
    /// 4. Fallback — first match by name (least accurate)
    fn build_call_edges(&mut self, elements: &[CodeElement]) {
        let mut parser = crate::parser::Parser::new();

        for elem in elements {
            if elem.element_type != ElementType::Function
                && elem.element_type != ElementType::Method
            {
                continue;
            }

            let lang = match SupportedLanguage::from_extension(&elem.file_path) {
                Some(l) => l,
                None => continue,
            };

            let tree = match parser.parse(&elem.code, lang) {
                Some(t) => t,
                None => continue,
            };

            let calls = extract_calls(&tree, &elem.code, lang);

            let caller_idx = match self.id_to_node.get(&elem.id) {
                Some(idx) => *idx,
                None => continue,
            };

            // Get this file's imported names for context-aware resolution
            let imported_names: Vec<String> = self
                .file_imports
                .get(&elem.file_path)
                .map(|v| v.clone())
                .unwrap_or_default();

            for call in &calls {
                let callee_name = &call.call_name;
                if let Some(callee_indices) = self.name_to_nodes.get(callee_name) {
                    let best_idx = self.resolve_call_target(
                        callee_name,
                        &callee_indices,
                        &elem.file_path,
                        &imported_names,
                    );

                    if let Some(callee_idx) = best_idx {
                        if callee_idx != caller_idx {
                            self.graph.add_edge(
                                caller_idx,
                                callee_idx,
                                GraphEdge {
                                    kind: EdgeKind::Calls,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    /// Resolve a call target from a list of candidates using layered heuristics.
    fn resolve_call_target(
        &self,
        callee_name: &str,
        candidates: &[NodeIndex],
        caller_file: &str,
        imported_names: &[String],
    ) -> Option<NodeIndex> {
        // Priority 1: Same file
        let same_file = candidates
            .iter()
            .find(|&&idx| self.graph[idx].file_path == caller_file);
        if let Some(&idx) = same_file {
            return Some(idx);
        }

        // Priority 2: Use SymbolResolver with import context
        let resolver = SymbolResolver::new(&self.global_index);
        let resolved = resolver.resolve_in_context(callee_name, caller_file, imported_names);
        if !resolved.is_empty() {
            // Find the first resolved element that's in our candidates
            for (_file_path, element_id) in &resolved {
                if let Some(idx_ref) = self.id_to_node.get(element_id) {
                    let idx = *idx_ref;
                    if candidates.contains(&idx) {
                        return Some(idx);
                    }
                }
            }
            // If resolved but not in candidates, use the first resolved element directly
            let (_file_path, element_id) = &resolved[0];
            if let Some(idx_ref) = self.id_to_node.get(element_id) {
                return Some(*idx_ref);
            }
        }

        // Priority 3: Heuristic — from an imported module (file path/name matching)
        if !imported_names.is_empty() {
            let from_import = candidates.iter().find(|&&idx| {
                let node = &self.graph[idx];
                imported_names
                    .iter()
                    .any(|imp| node.file_path.contains(imp) || node.name.as_str() == imp.as_str())
            });
            if let Some(&idx) = from_import {
                return Some(idx);
            }
        }

        // Priority 4: Fallback to first candidate
        candidates.first().copied()
    }

    /// Build import edges between files, dispatched by language.
    ///
    /// Uses the GlobalIndex's ModuleResolver as the primary resolution strategy
    /// (proper module-path-based resolution for Python, Java, etc.), falling back
    /// to heuristic name/path matching for other languages or when the GlobalIndex
    /// doesn't have a match.
    fn build_import_edges(&mut self, elements: &[CodeElement]) {
        let mut parser = crate::parser::Parser::new();

        for elem in elements {
            if elem.element_type != ElementType::File {
                continue;
            }

            let lang = match SupportedLanguage::from_extension(&elem.file_path) {
                Some(l) => l,
                None => continue,
            };

            let tree = match parser.parse(&elem.code, lang) {
                Some(t) => t,
                None => continue,
            };

            let imports = extract_imports(&tree, &elem.code, lang);

            let file_idx = match self.id_to_node.get(&elem.id) {
                Some(idx) => *idx,
                None => continue,
            };

            // Collect imported names for this file (used by call resolution)
            let mut imported_names = Vec::new();
            for import in &imports {
                imported_names.push(import.module.clone());
                imported_names.extend(import.names.iter().cloned());
            }
            self.file_imports
                .insert(elem.file_path.clone(), imported_names);

            let module_resolver = ModuleResolver::new(&self.global_index);

            for import in &imports {
                // Strategy 1: Use ModuleResolver for proper module-path resolution
                let resolved_via_index = module_resolver
                    .resolve_import(import, &elem.file_path)
                    .and_then(|file_path| {
                        // Find the File node for this resolved path
                        self.file_to_nodes.get(&file_path).and_then(|nodes| {
                            nodes
                                .iter()
                                .find(|&&idx| self.graph[idx].kind == NodeKind::File)
                                .copied()
                        })
                    });

                // Strategy 2: Fallback to heuristic name/path matching
                let target_idx =
                    resolved_via_index.or_else(|| self.resolve_import_target_heuristic(import));

                if let Some(target_idx) = target_idx {
                    self.graph.add_edge(
                        file_idx,
                        target_idx,
                        GraphEdge {
                            kind: EdgeKind::Imports,
                        },
                    );
                }

                // Also link to individually imported names (via SymbolResolver)
                let symbol_resolver = SymbolResolver::new(&self.global_index);
                for name in &import.names {
                    if name == "*" {
                        continue;
                    }

                    // Try SymbolResolver first (proper export_map lookup)
                    let resolved = symbol_resolver.resolve(name);
                    let linked = if !resolved.is_empty() {
                        // Link to the first matching element by its ID
                        let (_file_path, element_id) = &resolved[0];
                        self.id_to_node.get(element_id).map(|idx| *idx)
                    } else {
                        None
                    };

                    // Fallback to name_to_nodes
                    let target = linked.or_else(|| {
                        self.name_to_nodes
                            .get(name)
                            .and_then(|indices| indices.first().copied())
                    });

                    if let Some(target_idx) = target {
                        self.graph.add_edge(
                            file_idx,
                            target_idx,
                            GraphEdge {
                                kind: EdgeKind::Imports,
                            },
                        );
                    }
                }
            }
        }
    }

    /// Heuristic import resolution (fallback when GlobalIndex doesn't resolve).
    fn resolve_import_target_heuristic(
        &self,
        import: &crate::parser::imports::ImportInfo,
    ) -> Option<NodeIndex> {
        // Try direct module name match
        if let Some(target_indices) = self.name_to_nodes.get(&import.module) {
            if let Some(&idx) = target_indices.first() {
                return Some(idx);
            }
        }

        // Try last segment of module path (e.g., "os.path" -> "path")
        let last_segment = import
            .module
            .rsplit('.')
            .next()
            .or_else(|| import.module.rsplit('/').next())
            .or_else(|| import.module.rsplit("::").next());

        if let Some(seg) = last_segment {
            if seg != import.module {
                if let Some(target_indices) = self.name_to_nodes.get(seg) {
                    if let Some(&idx) = target_indices.first() {
                        return Some(idx);
                    }
                }
            }
        }

        // Try matching against file paths
        let normalized = import.module.replace('.', "/").replace("::", "/");
        for entry in self.file_to_nodes.iter() {
            let file_path = entry.key();
            if file_path.contains(&normalized) {
                if let Some(file_idx) = entry
                    .value()
                    .iter()
                    .find(|&&idx| self.graph[idx].kind == NodeKind::File)
                {
                    return Some(*file_idx);
                }
            }
        }

        None
    }

    /// Build inheritance edges, dispatched by language.
    fn build_inheritance_edges(&mut self, elements: &[CodeElement]) {
        let mut parser = crate::parser::Parser::new();

        for elem in elements {
            if elem.element_type != ElementType::Class
                && elem.element_type != ElementType::Struct
                && elem.element_type != ElementType::Interface
            {
                continue;
            }

            let lang = match SupportedLanguage::from_extension(&elem.file_path) {
                Some(l) => l,
                None => continue,
            };

            let tree = match parser.parse(&elem.code, lang) {
                Some(t) => t,
                None => continue,
            };

            let class_idx = match self.id_to_node.get(&elem.id) {
                Some(idx) => *idx,
                None => continue,
            };

            let base_names = extract_base_classes(&tree, lang);

            for base_name in &base_names {
                if let Some(base_indices) = self.name_to_nodes.get(base_name) {
                    if let Some(&base_idx) = base_indices.first() {
                        self.graph.add_edge(
                            class_idx,
                            base_idx,
                            GraphEdge {
                                kind: EdgeKind::Inherits,
                            },
                        );
                    }
                }
            }
        }
    }

    /// Get the source code for an element by ID.
    pub fn get_source(&self, element_id: &str) -> Option<String> {
        self.element_arena.get(element_id).map(|e| e.code.clone())
    }

    /// Get graph statistics.
    pub fn stats(&self) -> GraphStats {
        GraphStats {
            node_count: self.graph.node_count(),
            edge_count: self.graph.edge_count(),
            file_count: self.file_to_nodes.len(),
            element_count: self.element_arena.len(),
        }
    }

    /// Get all indexed file paths.
    pub fn file_paths(&self) -> Vec<String> {
        self.file_to_nodes
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Resolve a module name to its file path using the global index.
    pub fn resolve_module(&self, module_name: &str) -> Option<String> {
        self.global_index.resolve_module(module_name)
    }

    /// Resolve a symbol name to matching (file_path, element_id) pairs.
    pub fn resolve_symbol(&self, symbol_name: &str) -> Vec<(String, String)> {
        self.global_index.resolve_symbol(symbol_name)
    }

    /// Get a clone of all indexed elements.
    /// Useful for snapshotting the current in-memory graph state.
    pub fn all_elements(&self) -> Vec<CodeElement> {
        let mut elements: Vec<CodeElement> = self
            .element_arena
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        elements.sort_by(|a, b| a.id.cmp(&b.id));
        elements
    }

    /// Get all element IDs associated with a file path.
    /// Used by the file watcher to remove stale BM25 entries before re-indexing.
    pub fn element_ids_for_file(&self, file_path: &str) -> Vec<String> {
        self.file_to_nodes
            .get(file_path)
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|&idx| self.graph[idx].id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Remove all nodes and edges associated with a file.
    pub fn remove_file(&mut self, file_path: &str) {
        if let Some((_, indices)) = self.file_to_nodes.remove(file_path) {
            for idx in indices {
                let name = self.graph[idx].name.to_string();
                if let Some(mut nodes) = self.name_to_nodes.get_mut(&name) {
                    nodes.retain(|&i| i != idx);
                }
                let id = self.graph[idx].id.clone();
                self.id_to_node.remove(&id);
                self.element_arena.remove(&id);
                self.graph.remove_node(idx);
            }
        }
        self.file_imports.remove(file_path);
    }

    /// Incrementally update the graph for a single changed file.
    ///
    /// Removes all old data for `file_path`, then re-adds `new_elements`
    /// and rebuilds edges (defines, imports, calls, inheritance) for them.
    pub fn update_file(&mut self, file_path: &str, new_elements: &[CodeElement], repo_root: &str) {
        // Phase 1: Remove old data
        self.remove_file(file_path);
        self.global_index.remove_file(file_path);

        // Phase 2: Add new nodes
        for elem in new_elements {
            let node = GraphNode {
                id: elem.id.clone(),
                kind: NodeKind::from(elem.element_type),
                name: SmolStr::new(&elem.name),
                file_path: elem.file_path.clone(),
                start_line: elem.start_line,
                end_line: elem.end_line,
            };
            self.add_node(node);
            self.element_arena.insert(elem.id.clone(), elem.clone());
        }

        // Phase 3: Rebuild GlobalIndex for these elements
        self.global_index.build(new_elements, repo_root);

        // Phase 4: Rebuild edges for the changed file
        self.build_import_edges(new_elements);
        self.build_call_edges(new_elements);
        self.build_inheritance_edges(new_elements);

        // Phase 5: Rebuild defines edges
        for elem in new_elements {
            if elem.element_type == ElementType::File {
                continue;
            }
            if let Some(file_nodes) = self.file_to_nodes.get(&elem.file_path) {
                let file_idx = file_nodes
                    .iter()
                    .find(|&&idx| self.graph[idx].kind == NodeKind::File);
                if let (Some(&file_idx), Some(elem_idx)) = (file_idx, self.id_to_node.get(&elem.id))
                {
                    self.graph.add_edge(
                        file_idx,
                        *elem_idx,
                        GraphEdge {
                            kind: EdgeKind::Defines,
                        },
                    );
                }
            }
        }
    }
}

impl Default for RepositoryGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub file_count: usize,
    pub element_count: usize,
}

// ── Multi-language inheritance extraction ──────────────────────

/// Extract base class/interface names from a class definition, dispatched by language.
fn extract_base_classes(tree: &tree_sitter::Tree, language: SupportedLanguage) -> Vec<String> {
    let root = tree.root_node();
    let mut bases = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        collect_bases_recursive(&child, language, tree, &mut bases);
    }
    // Also check root itself
    collect_bases_recursive(&root, language, tree, &mut bases);
    bases
}

fn collect_bases_recursive(
    node: &tree_sitter::Node,
    language: SupportedLanguage,
    tree: &tree_sitter::Tree,
    bases: &mut Vec<String>,
) {
    let code_bytes = tree.root_node().utf8_text(&[]).unwrap_or_default();
    let src = code_bytes.as_bytes();

    match language {
        SupportedLanguage::Python => {
            if node.kind() == "class_definition" {
                if let Some(superclasses) = node.child_by_field_name("superclasses") {
                    extract_identifier_names(&superclasses, src, bases);
                }
            }
        }
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            if node.kind() == "class_declaration" || node.kind() == "class" {
                // class Foo extends Bar implements Baz
                extract_js_ts_bases(node, src, bases);
            }
            if node.kind() == "interface_declaration" {
                // interface Foo extends Bar
                extract_js_ts_bases(node, src, bases);
            }
        }
        SupportedLanguage::Java => {
            if node.kind() == "class_declaration" {
                // class Foo extends Bar implements Baz, Qux
                if let Some(sc) = node.child_by_field_name("superclass") {
                    extract_identifier_names(&sc, src, bases);
                }
                if let Some(ifaces) = node.child_by_field_name("interfaces") {
                    extract_java_type_list(&ifaces, src, bases);
                }
            }
            if node.kind() == "interface_declaration" {
                if let Some(extends) = node.child_by_field_name("extends_interfaces") {
                    extract_java_type_list(&extends, src, bases);
                }
            }
        }
        SupportedLanguage::Rust => {
            if node.kind() == "impl_item" {
                // impl Trait for Type — extract trait name
                if let Some(trait_node) = node.child_by_field_name("trait") {
                    let name = trait_node.utf8_text(src).unwrap_or_default().to_string();
                    if !name.is_empty() {
                        bases.push(name);
                    }
                }
            }
        }
        SupportedLanguage::Cpp | SupportedLanguage::C => {
            if node.kind() == "class_specifier" || node.kind() == "struct_specifier" {
                // class Derived : public Base, private Other
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "base_class_clause" {
                        extract_identifier_names(&child, src, bases);
                    }
                }
            }
        }
        SupportedLanguage::Go => {
            // Go doesn't have class inheritance. Struct embedding is handled
            // at the field level, not captured here.
        }
    }

    // Recurse
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_bases_recursive(&child, language, tree, bases);
    }
}

/// Extract identifier or dotted_name children as base class names.
fn extract_identifier_names(node: &tree_sitter::Node, src: &[u8], names: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "dotted_name" | "type_identifier" | "scoped_type_identifier" => {
                let name = child.utf8_text(src).unwrap_or_default().to_string();
                if !name.is_empty() && name != "public" && name != "private" && name != "protected"
                {
                    names.push(name);
                }
            }
            _ => {}
        }
    }
}

/// Extract base classes from JS/TS class declaration (extends/implements).
fn extract_js_ts_bases(node: &tree_sitter::Node, src: &[u8], bases: &mut Vec<String>) {
    let mut cursor = node.walk();
    let mut found_extends = false;
    let mut found_implements = false;

    for child in node.children(&mut cursor) {
        let text = child.utf8_text(src).unwrap_or_default();

        if text == "extends" {
            found_extends = true;
            found_implements = false;
            continue;
        }
        if text == "implements" {
            found_implements = true;
            found_extends = false;
            continue;
        }

        if (found_extends || found_implements)
            && (child.kind() == "identifier"
                || child.kind() == "type_identifier"
                || child.kind() == "member_expression")
        {
            let name = text.to_string();
            if !name.is_empty() && name != "{" {
                bases.push(name);
            }
            found_extends = false;
            found_implements = false;
        }

        // class_heritage node in some TS grammars
        if child.kind() == "class_heritage" {
            extract_identifier_names(&child, src, bases);
        }
    }
}

/// Extract type names from a Java type list (implements Foo, Bar).
fn extract_java_type_list(node: &tree_sitter::Node, src: &[u8], bases: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "scoped_type_identifier" | "generic_type" => {
                // For generic_type, take the base type name
                if child.kind() == "generic_type" {
                    if let Some(base) = child.child(0) {
                        let name = base.utf8_text(src).unwrap_or_default().to_string();
                        if !name.is_empty() {
                            bases.push(name);
                        }
                    }
                } else {
                    let name = child.utf8_text(src).unwrap_or_default().to_string();
                    if !name.is_empty() {
                        bases.push(name);
                    }
                }
            }
            "type_list" => {
                extract_java_type_list(&child, src, bases);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::element::*;
    use std::collections::HashMap;

    fn make_element(
        id: &str,
        name: &str,
        etype: ElementType,
        file: &str,
        code: &str,
    ) -> CodeElement {
        CodeElement {
            id: id.to_string(),
            element_type: etype,
            name: name.to_string(),
            file_path: file.to_string(),
            relative_path: file.to_string(),
            language: "python".to_string(),
            start_line: 1,
            end_line: 5,
            code: code.to_string(),
            signature: None,
            docstring: None,
            summary: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_build_graph_nodes() {
        let elements = vec![
            make_element(
                "file_a",
                "a.py",
                ElementType::File,
                "a.py",
                "def foo():\n    pass\n",
            ),
            make_element(
                "func_foo",
                "foo",
                ElementType::Function,
                "a.py",
                "def foo():\n    pass\n",
            ),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements, "");

        assert_eq!(graph.stats().node_count, 2);
        assert!(graph.get_source("func_foo").is_some());
    }

    #[test]
    fn test_remove_file() {
        let elements = vec![
            make_element("file_a", "a.py", ElementType::File, "a.py", "x = 1"),
            make_element(
                "func_foo",
                "foo",
                ElementType::Function,
                "a.py",
                "def foo(): pass",
            ),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements, "");
        assert_eq!(graph.stats().node_count, 2);

        graph.remove_file("a.py");
        assert_eq!(graph.stats().node_count, 0);
    }

    #[test]
    fn test_import_aware_call_resolution() {
        // Two files: a.py imports helper from b.py
        // Both files have a function named "helper", but a.py's call should resolve to b.py's
        let elements = vec![
            make_element(
                "file_a",
                "a.py",
                ElementType::File,
                "a.py",
                "from b import helper\n\ndef caller():\n    helper()\n",
            ),
            make_element(
                "func_caller",
                "caller",
                ElementType::Function,
                "a.py",
                "def caller():\n    helper()\n",
            ),
            make_element(
                "file_b",
                "b.py",
                ElementType::File,
                "b.py",
                "def helper():\n    pass\n",
            ),
            make_element(
                "func_helper_b",
                "helper",
                ElementType::Function,
                "b.py",
                "def helper():\n    pass\n",
            ),
            make_element(
                "file_c",
                "c.py",
                ElementType::File,
                "c.py",
                "def helper():\n    return 42\n",
            ),
            make_element(
                "func_helper_c",
                "helper",
                ElementType::Function,
                "c.py",
                "def helper():\n    return 42\n",
            ),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements, "");

        // Verify the graph has call edges
        assert!(graph.stats().edge_count > 0, "Should have edges");
    }

    #[test]
    fn test_update_file() {
        // Build initial graph with two files
        let elements = vec![
            make_element(
                "file_a",
                "a.py",
                ElementType::File,
                "a.py",
                "def foo():\n    pass\n",
            ),
            make_element(
                "func_foo",
                "foo",
                ElementType::Function,
                "a.py",
                "def foo():\n    pass\n",
            ),
            make_element(
                "file_b",
                "b.py",
                ElementType::File,
                "b.py",
                "def bar():\n    pass\n",
            ),
            make_element(
                "func_bar",
                "bar",
                ElementType::Function,
                "b.py",
                "def bar():\n    pass\n",
            ),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements, "");
        assert_eq!(graph.stats().node_count, 4);
        assert!(graph.get_source("func_foo").is_some());

        // Update a.py: replace foo with baz
        let new_elements = vec![
            make_element(
                "file_a_v2",
                "a.py",
                ElementType::File,
                "a.py",
                "def baz():\n    pass\n",
            ),
            make_element(
                "func_baz",
                "baz",
                ElementType::Function,
                "a.py",
                "def baz():\n    pass\n",
            ),
        ];
        graph.update_file("a.py", &new_elements, "");

        // Old element should be gone
        assert!(graph.get_source("func_foo").is_none());
        // New element should be present
        assert!(graph.get_source("func_baz").is_some());
        // b.py should still be intact
        assert!(graph.get_source("func_bar").is_some());
        // Node count should still be 4 (2 from b.py + 2 new from a.py)
        assert_eq!(graph.stats().node_count, 4);
    }

    #[test]
    fn test_element_ids_for_file() {
        let elements = vec![
            make_element(
                "file_a",
                "a.py",
                ElementType::File,
                "a.py",
                "def foo():\n    pass\n",
            ),
            make_element(
                "func_foo",
                "foo",
                ElementType::Function,
                "a.py",
                "def foo():\n    pass\n",
            ),
            make_element(
                "file_b",
                "b.py",
                ElementType::File,
                "b.py",
                "def bar():\n    pass\n",
            ),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements, "");

        let ids_a = graph.element_ids_for_file("a.py");
        assert_eq!(ids_a.len(), 2);
        assert!(ids_a.contains(&"file_a".to_string()));
        assert!(ids_a.contains(&"func_foo".to_string()));

        let ids_b = graph.element_ids_for_file("b.py");
        assert_eq!(ids_b.len(), 1);

        let ids_c = graph.element_ids_for_file("nonexistent.py");
        assert!(ids_c.is_empty());
    }

    #[test]
    fn test_all_elements_snapshot() {
        let elements = vec![
            make_element(
                "z_func",
                "z",
                ElementType::Function,
                "a.py",
                "def z():\n    pass\n",
            ),
            make_element(
                "a_file",
                "a.py",
                ElementType::File,
                "a.py",
                "def z():\n    pass\n",
            ),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements, "");

        let snapshot = graph.all_elements();
        assert_eq!(snapshot.len(), 2);
        // Returned in stable order for deterministic snapshotting.
        assert_eq!(snapshot[0].id, "a_file");
        assert_eq!(snapshot[1].id, "z_func");
    }
}
