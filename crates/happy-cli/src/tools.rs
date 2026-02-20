use crate::provider::ToolDefinition;
use crate::repo::RepoContext;
use serde_json::{json, Value};

/// All tool definitions for the LLM.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "search_code".into(),
            description: "Search the codebase using BM25 keyword search. Returns matching code elements ranked by relevance.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query (keywords)" },
                    "limit": { "type": "integer", "description": "Max results (default 10)" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "get_source".into(),
            description: "Get the full source code of a code element by its ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "element_id": { "type": "string", "description": "The element ID" }
                },
                "required": ["element_id"]
            }),
        },
        ToolDefinition {
            name: "find_callers".into(),
            description: "Find all functions/methods that call a given symbol.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "string", "description": "The symbol name" }
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "find_callees".into(),
            description: "Find all functions/methods that a given symbol calls.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "string", "description": "The symbol name" }
                },
                "required": ["symbol"]
            }),
        },
        ToolDefinition {
            name: "get_dependencies".into(),
            description: "Get files that a given file depends on (imports from).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The file path" }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "get_dependents".into(),
            description: "Get files that depend on (import from) a given file.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The file path" }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "get_subclasses".into(),
            description: "Find all classes that inherit from a given class.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_name": { "type": "string", "description": "The class name" }
                },
                "required": ["class_name"]
            }),
        },
        ToolDefinition {
            name: "get_superclasses".into(),
            description: "Find parent classes of a given class.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_name": { "type": "string", "description": "The class name" }
                },
                "required": ["class_name"]
            }),
        },
        ToolDefinition {
            name: "find_path".into(),
            description: "Find the shortest dependency/call path between two code elements.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Source symbol name" },
                    "target": { "type": "string", "description": "Target symbol name" }
                },
                "required": ["source", "target"]
            }),
        },
        ToolDefinition {
            name: "get_related".into(),
            description: "Find code elements related to a given element within N hops in the code graph.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "element": { "type": "string", "description": "The element name" },
                    "max_hops": { "type": "integer", "description": "Maximum graph hops (default 2)" }
                },
                "required": ["element"]
            }),
        },
        ToolDefinition {
            name: "repo_stats".into(),
            description: "Get repository statistics: node count, edge count, file count, and breakdown by element type.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "list_files".into(),
            description: "List all indexed source files in the repository.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "read_file".into(),
            description: "Read the raw source code of a file by path.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path (absolute or relative to repo root)" }
                },
                "required": ["path"]
            }),
        },
    ]
}

/// Execute a tool call and return the result as a string.
pub fn execute_tool(repo: &RepoContext, tool_name: &str, input: &Value) -> Result<String, String> {
    match tool_name {
        "search_code" => {
            let query = input["query"].as_str().ok_or("missing 'query'")?;
            let limit = input["limit"].as_u64().unwrap_or(10) as usize;
            let results = repo.bm25.search(query, limit);
            if results.is_empty() {
                return Ok("No results found.".into());
            }
            let mut out = String::new();
            for (id, score) in &results {
                let display = repo
                    .elements
                    .iter()
                    .find(|e| e.id == *id)
                    .map(|e| {
                        format!(
                            "{} [{}] ({}:{})",
                            e.name,
                            e.element_type.as_str(),
                            e.relative_path,
                            e.start_line
                        )
                    })
                    .unwrap_or_else(|| id.clone());
                out.push_str(&format!("  {:.4}  {} (id: {})\n", score, display, id));
            }
            Ok(out)
        }
        "get_source" => {
            let id = input["element_id"].as_str().ok_or("missing 'element_id'")?;
            repo.graph
                .get_source(id)
                .ok_or_else(|| format!("Element '{}' not found", id))
        }
        "find_callers" => {
            let symbol = input["symbol"].as_str().ok_or("missing 'symbol'")?;
            Ok(format_graph_nodes(&repo.graph.find_callers(symbol)))
        }
        "find_callees" => {
            let symbol = input["symbol"].as_str().ok_or("missing 'symbol'")?;
            Ok(format_graph_nodes(&repo.graph.find_callees(symbol)))
        }
        "get_dependencies" => {
            let file = input["file_path"].as_str().ok_or("missing 'file_path'")?;
            Ok(format_graph_nodes(&repo.graph.get_dependencies(file)))
        }
        "get_dependents" => {
            let file = input["file_path"].as_str().ok_or("missing 'file_path'")?;
            Ok(format_graph_nodes(&repo.graph.get_dependents(file)))
        }
        "get_subclasses" => {
            let name = input["class_name"].as_str().ok_or("missing 'class_name'")?;
            Ok(format_graph_nodes(&repo.graph.get_subclasses(name)))
        }
        "get_superclasses" => {
            let name = input["class_name"].as_str().ok_or("missing 'class_name'")?;
            Ok(format_graph_nodes(&repo.graph.get_superclasses(name)))
        }
        "find_path" => {
            let source = input["source"].as_str().ok_or("missing 'source'")?;
            let target = input["target"].as_str().ok_or("missing 'target'")?;
            match repo.graph.find_path(source, target, None) {
                Some(path) => Ok(format!("Path: {}", path.join(" -> "))),
                None => Ok(format!("No path found between '{}' and '{}'", source, target)),
            }
        }
        "get_related" => {
            let element = input["element"].as_str().ok_or("missing 'element'")?;
            let max_hops = input["max_hops"].as_u64().unwrap_or(2) as usize;
            Ok(format_graph_nodes(&repo.graph.get_related(element, max_hops)))
        }
        "repo_stats" => {
            let stats = repo.graph.stats();
            let mut type_counts = std::collections::HashMap::new();
            for elem in &repo.elements {
                *type_counts.entry(elem.element_type.as_str()).or_insert(0usize) += 1;
            }
            let mut out = format!(
                "Nodes: {}, Edges: {}, Files: {}, Elements: {}\nBy type:\n",
                stats.node_count, stats.edge_count, stats.file_count, stats.element_count
            );
            let mut counts: Vec<_> = type_counts.into_iter().collect();
            counts.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
            for (t, c) in counts {
                out.push_str(&format!("  {}: {}\n", t, c));
            }
            Ok(out)
        }
        "list_files" => Ok(repo.list_files().join("\n")),
        "read_file" => {
            let path = input["path"].as_str().ok_or("missing 'path'")?;
            let full_path = if std::path::Path::new(path).is_absolute() {
                path.to_string()
            } else {
                format!("{}/{}", repo.repo_path, path)
            };
            std::fs::read_to_string(&full_path)
                .map_err(|e| format!("Failed to read '{}': {}", full_path, e))
        }
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

fn format_graph_nodes(nodes: &[&happy_core::graph::types::GraphNode]) -> String {
    if nodes.is_empty() {
        return "No results.".into();
    }
    nodes
        .iter()
        .map(|n| {
            format!(
                "{} [{:?}] ({}:{}) id: {}",
                n.name, n.kind, n.file_path, n.start_line, n.id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
