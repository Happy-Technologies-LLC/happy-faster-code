pub mod types;
pub mod queries;

use dashmap::DashMap;
use petgraph::stable_graph::NodeIndex;
use smol_str::SmolStr;

use types::{CodeGraph, GraphNode, GraphEdge, NodeKind, EdgeKind};
use crate::indexer::{CodeElement, ElementType};
use crate::parser::calls::extract_calls;
use crate::parser::imports::extract_imports;

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
}

impl RepositoryGraph {
    pub fn new() -> Self {
        Self {
            graph: CodeGraph::default(),
            id_to_node: DashMap::new(),
            name_to_nodes: DashMap::new(),
            file_to_nodes: DashMap::new(),
            element_arena: DashMap::new(),
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
    pub fn build_from_elements(&mut self, elements: &[CodeElement]) {
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

        // Phase 2: Build "defines" edges (file -> its children)
        for elem in elements {
            if elem.element_type == ElementType::File {
                continue;
            }
            // Find the file node for this element
            if let Some(file_nodes) = self.file_to_nodes.get(&elem.file_path) {
                let file_idx = file_nodes.iter().find(|&&idx| {
                    self.graph[idx].kind == NodeKind::File
                });
                if let (Some(&file_idx), Some(elem_idx)) = (file_idx, self.id_to_node.get(&elem.id)) {
                    self.graph.add_edge(file_idx, *elem_idx, GraphEdge {
                        kind: EdgeKind::Defines,
                    });
                }
            }
        }

        // Phase 3: Build call/import/inheritance edges from source code analysis
        self.build_call_edges(elements);
        self.build_import_edges(elements);
        self.build_inheritance_edges(elements);
    }

    fn build_call_edges(&mut self, elements: &[CodeElement]) {
        let mut parser = crate::parser::Parser::new();

        for elem in elements {
            if elem.element_type != ElementType::Function
                && elem.element_type != ElementType::Method
            {
                continue;
            }

            let lang = match crate::parser::languages::SupportedLanguage::from_extension(&elem.file_path) {
                Some(l) => l,
                None => continue,
            };

            let tree = match parser.parse(&elem.code, lang) {
                Some(t) => t,
                None => continue,
            };

            let calls = extract_calls(&tree, &elem.code);

            let caller_idx = match self.id_to_node.get(&elem.id) {
                Some(idx) => *idx,
                None => continue,
            };

            for call in &calls {
                let callee_name = &call.call_name;
                if let Some(callee_indices) = self.name_to_nodes.get(callee_name) {
                    // Scope-aware resolution: prefer callees in the same file
                    let best_idx = callee_indices
                        .iter()
                        .find(|&&idx| self.graph[idx].file_path == elem.file_path)
                        .or_else(|| callee_indices.first())
                        .copied();

                    if let Some(callee_idx) = best_idx {
                        // Don't add self-edges
                        if callee_idx != caller_idx {
                            self.graph.add_edge(
                                caller_idx,
                                callee_idx,
                                GraphEdge { kind: EdgeKind::Calls },
                            );
                        }
                    }
                }
            }
        }
    }

    fn build_import_edges(&mut self, elements: &[CodeElement]) {
        let mut parser = crate::parser::Parser::new();

        for elem in elements {
            if elem.element_type != ElementType::File {
                continue;
            }

            let lang = match crate::parser::languages::SupportedLanguage::from_extension(&elem.file_path) {
                Some(l) => l,
                None => continue,
            };

            let tree = match parser.parse(&elem.code, lang) {
                Some(t) => t,
                None => continue,
            };

            let imports = extract_imports(&tree, &elem.code);

            let file_idx = match self.id_to_node.get(&elem.id) {
                Some(idx) => *idx,
                None => continue,
            };

            for import in &imports {
                // Try to resolve the imported module to a file node
                if let Some(target_indices) = self.name_to_nodes.get(&import.module) {
                    if let Some(&target_idx) = target_indices.first() {
                        self.graph.add_edge(
                            file_idx,
                            target_idx,
                            GraphEdge { kind: EdgeKind::Imports },
                        );
                    }
                }
            }
        }
    }

    fn build_inheritance_edges(&mut self, elements: &[CodeElement]) {
        let mut parser = crate::parser::Parser::new();

        for elem in elements {
            if elem.element_type != ElementType::Class {
                continue;
            }

            let lang = match crate::parser::languages::SupportedLanguage::from_extension(&elem.file_path) {
                Some(l) => l,
                None => continue,
            };

            let tree = match parser.parse(&elem.code, lang) {
                Some(t) => t,
                None => continue,
            };

            // Walk the class node to find bases/superclasses
            let root = tree.root_node();
            let mut cursor = root.walk();
            for child in root.children(&mut cursor) {
                if child.kind() == "class_definition" || child.kind() == "class_declaration" {
                    if let Some(bases) = child.child_by_field_name("superclasses") {
                        let mut base_cursor = bases.walk();
                        for base in bases.children(&mut base_cursor) {
                            if base.kind() == "identifier" || base.kind() == "dotted_name" {
                                let base_name = base.utf8_text(elem.code.as_bytes())
                                    .unwrap_or_default()
                                    .to_string();

                                let class_idx = match self.id_to_node.get(&elem.id) {
                                    Some(idx) => *idx,
                                    None => continue,
                                };

                                if let Some(base_indices) = self.name_to_nodes.get(&base_name) {
                                    if let Some(&base_idx) = base_indices.first() {
                                        self.graph.add_edge(
                                            class_idx,
                                            base_idx,
                                            GraphEdge { kind: EdgeKind::Inherits },
                                        );
                                    }
                                }
                            }
                        }
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

    /// Remove all nodes and edges associated with a file.
    pub fn remove_file(&mut self, file_path: &str) {
        if let Some((_, indices)) = self.file_to_nodes.remove(file_path) {
            for idx in indices {
                // Remove from name index
                let name = self.graph[idx].name.to_string();
                if let Some(mut nodes) = self.name_to_nodes.get_mut(&name) {
                    nodes.retain(|&i| i != idx);
                }
                // Remove from id index
                let id = self.graph[idx].id.clone();
                self.id_to_node.remove(&id);
                self.element_arena.remove(&id);
                // Remove the node (and its edges) from the graph
                self.graph.remove_node(idx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::element::*;
    use std::collections::HashMap;

    fn make_element(id: &str, name: &str, etype: ElementType, file: &str, code: &str) -> CodeElement {
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
            make_element("file_a", "a.py", ElementType::File, "a.py", "def foo():\n    pass\n"),
            make_element("func_foo", "foo", ElementType::Function, "a.py", "def foo():\n    pass\n"),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements);

        assert_eq!(graph.stats().node_count, 2);
        assert!(graph.get_source("func_foo").is_some());
    }

    #[test]
    fn test_remove_file() {
        let elements = vec![
            make_element("file_a", "a.py", ElementType::File, "a.py", "x = 1"),
            make_element("func_foo", "foo", ElementType::Function, "a.py", "def foo(): pass"),
        ];

        let mut graph = RepositoryGraph::new();
        graph.build_from_elements(&elements);
        assert_eq!(graph.stats().node_count, 2);

        graph.remove_file("a.py");
        assert_eq!(graph.stats().node_count, 0);
    }
}
