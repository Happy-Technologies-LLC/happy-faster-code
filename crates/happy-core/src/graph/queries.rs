use petgraph::Direction;
use petgraph::algo::astar;
use petgraph::stable_graph::NodeIndex;

use super::RepositoryGraph;
use super::types::{EdgeKind, GraphNode};

impl RepositoryGraph {
    /// Find all callers of a symbol (nodes with Calls edges pointing to it).
    pub fn find_callers(&self, symbol: &str) -> Vec<&GraphNode> {
        let indices = self.find_nodes_by_name(symbol);
        let mut callers = Vec::new();

        for idx in indices {
            for neighbor in self.graph.neighbors_directed(idx, Direction::Incoming) {
                let edge_idx = self.graph.find_edge(neighbor, idx);
                if let Some(eidx) = edge_idx {
                    if self.graph[eidx].kind == EdgeKind::Calls {
                        callers.push(&self.graph[neighbor]);
                    }
                }
            }
        }

        callers
    }

    /// Find all callees of a symbol (nodes it calls).
    pub fn find_callees(&self, symbol: &str) -> Vec<&GraphNode> {
        let indices = self.find_nodes_by_name(symbol);
        let mut callees = Vec::new();

        for idx in indices {
            for neighbor in self.graph.neighbors_directed(idx, Direction::Outgoing) {
                let edge_idx = self.graph.find_edge(idx, neighbor);
                if let Some(eidx) = edge_idx {
                    if self.graph[eidx].kind == EdgeKind::Calls {
                        callees.push(&self.graph[neighbor]);
                    }
                }
            }
        }

        callees
    }

    /// Get files that a given file depends on (via import edges).
    pub fn get_dependencies(&self, file_path: &str) -> Vec<&GraphNode> {
        let indices = self.find_nodes_by_file(file_path);
        let mut deps = Vec::new();

        for idx in indices {
            if self.graph[idx].kind != super::types::NodeKind::File {
                continue;
            }
            for neighbor in self.graph.neighbors_directed(idx, Direction::Outgoing) {
                let edge_idx = self.graph.find_edge(idx, neighbor);
                if let Some(eidx) = edge_idx {
                    if self.graph[eidx].kind == EdgeKind::Imports {
                        deps.push(&self.graph[neighbor]);
                    }
                }
            }
        }

        deps
    }

    /// Get files that depend on a given file (reverse imports).
    pub fn get_dependents(&self, file_path: &str) -> Vec<&GraphNode> {
        let indices = self.find_nodes_by_file(file_path);
        let mut dependents = Vec::new();

        for idx in indices {
            if self.graph[idx].kind != super::types::NodeKind::File {
                continue;
            }
            for neighbor in self.graph.neighbors_directed(idx, Direction::Incoming) {
                let edge_idx = self.graph.find_edge(neighbor, idx);
                if let Some(eidx) = edge_idx {
                    if self.graph[eidx].kind == EdgeKind::Imports {
                        dependents.push(&self.graph[neighbor]);
                    }
                }
            }
        }

        dependents
    }

    /// Get subclasses of a class.
    pub fn get_subclasses(&self, class_name: &str) -> Vec<&GraphNode> {
        let indices = self.find_nodes_by_name(class_name);
        let mut subclasses = Vec::new();

        for idx in indices {
            for neighbor in self.graph.neighbors_directed(idx, Direction::Incoming) {
                let edge_idx = self.graph.find_edge(neighbor, idx);
                if let Some(eidx) = edge_idx {
                    if self.graph[eidx].kind == EdgeKind::Inherits {
                        subclasses.push(&self.graph[neighbor]);
                    }
                }
            }
        }

        subclasses
    }

    /// Get superclasses of a class.
    pub fn get_superclasses(&self, class_name: &str) -> Vec<&GraphNode> {
        let indices = self.find_nodes_by_name(class_name);
        let mut superclasses = Vec::new();

        for idx in indices {
            for neighbor in self.graph.neighbors_directed(idx, Direction::Outgoing) {
                let edge_idx = self.graph.find_edge(idx, neighbor);
                if let Some(eidx) = edge_idx {
                    if self.graph[eidx].kind == EdgeKind::Inherits {
                        superclasses.push(&self.graph[neighbor]);
                    }
                }
            }
        }

        superclasses
    }

    /// Find shortest path between two elements, optionally filtering by edge type.
    pub fn find_path(
        &self,
        source: &str,
        target: &str,
        edge_filter: Option<EdgeKind>,
    ) -> Option<Vec<String>> {
        let source_indices = self.find_nodes_by_name(source);
        let target_indices = self.find_nodes_by_name(target);

        for &src in &source_indices {
            for &tgt in &target_indices {
                let result = astar(
                    &self.graph,
                    src,
                    |n| n == tgt,
                    |edge| {
                        if let Some(filter) = edge_filter {
                            if edge.weight().kind == filter {
                                1
                            } else {
                                usize::MAX / 2
                            }
                        } else {
                            1
                        }
                    },
                    |_| 0,
                );

                if let Some((_cost, path)) = result {
                    return Some(
                        path.into_iter()
                            .map(|idx| self.graph[idx].id.clone())
                            .collect(),
                    );
                }
            }
        }

        None
    }

    /// Get related elements within a given number of hops.
    pub fn get_related(&self, element_name: &str, max_hops: usize) -> Vec<&GraphNode> {
        let indices = self.find_nodes_by_name(element_name);
        let mut visited = std::collections::HashSet::new();
        let mut result = Vec::new();
        let mut frontier: Vec<NodeIndex> = indices.clone();

        for _ in 0..max_hops {
            let mut next_frontier = Vec::new();
            for idx in &frontier {
                if !visited.insert(*idx) {
                    continue;
                }
                // Both directions
                for neighbor in self.graph.neighbors_directed(*idx, Direction::Outgoing) {
                    if !visited.contains(&neighbor) {
                        next_frontier.push(neighbor);
                    }
                }
                for neighbor in self.graph.neighbors_directed(*idx, Direction::Incoming) {
                    if !visited.contains(&neighbor) {
                        next_frontier.push(neighbor);
                    }
                }
            }
            frontier = next_frontier;
        }

        // Add the final frontier
        for idx in &frontier {
            if visited.insert(*idx) {
                // only new ones
            }
        }

        // Collect all visited except the originals
        for idx in visited {
            if !indices.contains(&idx) {
                result.push(&self.graph[idx]);
            }
        }

        result
    }

    // --- internal helpers ---

    fn find_nodes_by_name(&self, name: &str) -> Vec<NodeIndex> {
        self.name_to_nodes
            .get(name)
            .map(|v| v.value().clone())
            .unwrap_or_default()
    }

    fn find_nodes_by_file(&self, file_path: &str) -> Vec<NodeIndex> {
        self.file_to_nodes
            .get(file_path)
            .map(|v| v.value().clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::*;
    use smol_str::SmolStr;

    fn build_test_graph() -> RepositoryGraph {
        let mut repo = RepositoryGraph::new();

        let a = repo.add_node(GraphNode {
            id: "func_a".into(),
            kind: NodeKind::Function,
            name: SmolStr::new("func_a"),
            file_path: "a.py".into(),
            start_line: 1,
            end_line: 5,
        });
        let b = repo.add_node(GraphNode {
            id: "func_b".into(),
            kind: NodeKind::Function,
            name: SmolStr::new("func_b"),
            file_path: "b.py".into(),
            start_line: 1,
            end_line: 5,
        });

        repo.add_edge(
            a,
            b,
            GraphEdge {
                kind: EdgeKind::Calls,
            },
        );
        repo
    }

    #[test]
    fn test_find_callers() {
        let repo = build_test_graph();
        let callers = repo.find_callers("func_b");
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].name.as_str(), "func_a");
    }

    #[test]
    fn test_find_callees() {
        let repo = build_test_graph();
        let callees = repo.find_callees("func_a");
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].name.as_str(), "func_b");
    }

    #[test]
    fn test_find_path() {
        let repo = build_test_graph();
        let path = repo.find_path("func_a", "func_b", None);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, vec!["func_a", "func_b"]);
    }

    #[test]
    fn test_get_related() {
        let repo = build_test_graph();
        let related = repo.get_related("func_a", 1);
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].name.as_str(), "func_b");
    }
}
