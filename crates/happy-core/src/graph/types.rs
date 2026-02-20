use petgraph::stable_graph::StableDiGraph;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// The kind of a node in the code graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    File,
    Module,
    Class,
    Function,
    Method,
    Variable,
    Interface,
    Struct,
    Enum,
}

/// A node in the repository graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    pub name: SmolStr,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// The type of relationship between two nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// File A imports from file B
    Imports,
    /// Class A inherits from class B
    Inherits,
    /// Function A calls function B
    Calls,
    /// File/class defines function/method
    Defines,
    /// Element A references element B
    References,
    /// Class A implements interface B
    Implements,
}

/// An edge in the repository graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub kind: EdgeKind,
}

/// The core graph type used throughout the application.
pub type CodeGraph = StableDiGraph<GraphNode, GraphEdge>;

impl From<crate::indexer::ElementType> for NodeKind {
    fn from(et: crate::indexer::ElementType) -> Self {
        match et {
            crate::indexer::ElementType::File => NodeKind::File,
            crate::indexer::ElementType::Class => NodeKind::Class,
            crate::indexer::ElementType::Function => NodeKind::Function,
            crate::indexer::ElementType::Method => NodeKind::Method,
            crate::indexer::ElementType::Module => NodeKind::Module,
            crate::indexer::ElementType::Import => NodeKind::Module,
            crate::indexer::ElementType::Variable => NodeKind::Variable,
            crate::indexer::ElementType::Interface => NodeKind::Interface,
            crate::indexer::ElementType::Struct => NodeKind::Struct,
            crate::indexer::ElementType::Enum => NodeKind::Enum,
        }
    }
}
