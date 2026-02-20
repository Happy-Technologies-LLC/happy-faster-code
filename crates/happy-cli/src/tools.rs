use crate::provider::ToolDefinition;
use crate::repo::RepoContext;
use serde_json::{json, Value};
use std::io::Read;

/// Resolve a path relative to the repo root.
fn resolve_path(repo_path: &str, path: &str) -> String {
    if std::path::Path::new(path).is_absolute() {
        path.to_string()
    } else {
        format!("{}/{}", repo_path, path)
    }
}

/// All tool definitions for the LLM.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ---------------------------------------------------------------
        // Code graph tools (13 original tools)
        // ---------------------------------------------------------------
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
            description: "Read the contents of a file. Returns numbered lines (L1:, L2:, etc.). Use offset and limit for large files.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path (absolute or relative to repo root)" },
                    "offset": { "type": "integer", "description": "Start reading from this line number (1-based, default 1)" },
                    "limit": { "type": "integer", "description": "Maximum number of lines to return (default: entire file)" }
                },
                "required": ["path"]
            }),
        },
        // ---------------------------------------------------------------
        // Write / edit tools
        // ---------------------------------------------------------------
        ToolDefinition {
            name: "write_file".into(),
            description: "Create a new file or overwrite an existing file with the given content. Parent directories are created automatically.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path (relative to repo root or absolute)" },
                    "content": { "type": "string", "description": "The full file content to write" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "edit_file".into(),
            description: "Edit a file by replacing an exact string match with new content. The old_string must appear exactly once in the file for safety. Read the file first to see its current content.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path (relative to repo root or absolute)" },
                    "old_string": { "type": "string", "description": "The exact string to find (must be unique in the file)" },
                    "new_string": { "type": "string", "description": "The replacement string" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        // ---------------------------------------------------------------
        // Shell execution
        // ---------------------------------------------------------------
        ToolDefinition {
            name: "bash".into(),
            description: "Execute a shell command in the repo directory. Returns stdout and stderr. Use for running builds, tests, git, linters, package managers, and any other CLI tools. Working directory is the repository root.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The shell command to execute" },
                    "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 120, max 600)" }
                },
                "required": ["command"]
            }),
        },
        // ---------------------------------------------------------------
        // File system navigation
        // ---------------------------------------------------------------
        ToolDefinition {
            name: "list_directory".into(),
            description: "List the contents of a directory. Shows files and subdirectories with type indicators.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path (relative to repo root or absolute, default: repo root)" }
                }
            }),
        },
        ToolDefinition {
            name: "grep_files".into(),
            description: "Search for a pattern across files in the repository. Returns matching lines with file paths and line numbers. Uses ripgrep if available, falls back to grep.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Search pattern (regex supported)" },
                    "path": { "type": "string", "description": "Directory or file to search in (default: repo root)" },
                    "include": { "type": "string", "description": "File glob pattern to include (e.g. '*.rs', '*.py')" },
                    "max_results": { "type": "integer", "description": "Maximum number of matching lines to return (default 50)" }
                },
                "required": ["pattern"]
            }),
        },
    ]
}

/// Execute a tool call and return the result as a string.
pub fn execute_tool(repo: &RepoContext, tool_name: &str, input: &Value) -> Result<String, String> {
    match tool_name {
        // -----------------------------------------------------------
        // Code graph tools
        // -----------------------------------------------------------
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
            Ok(format_graph_nodes(
                &repo.graph.get_related(element, max_hops),
            ))
        }
        "repo_stats" => {
            let stats = repo.graph.stats();
            let mut type_counts = std::collections::HashMap::new();
            for elem in &repo.elements {
                *type_counts
                    .entry(elem.element_type.as_str())
                    .or_insert(0usize) += 1;
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

        // -----------------------------------------------------------
        // read_file — with line-number output, offset, and limit
        // -----------------------------------------------------------
        "read_file" => {
            let path = input["path"].as_str().ok_or("missing 'path'")?;
            let full_path = resolve_path(&repo.repo_path, path);
            let content = std::fs::read_to_string(&full_path)
                .map_err(|e| format!("Failed to read '{}': {}", path, e))?;

            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let offset = input["offset"]
                .as_u64()
                .map(|o| (o as usize).saturating_sub(1)) // 1-based → 0-based
                .unwrap_or(0);
            let limit = input["limit"].as_u64().map(|l| l as usize).unwrap_or(total);

            let mut out = String::new();
            for (i, line) in lines.iter().enumerate().skip(offset).take(limit) {
                out.push_str(&format!("L{}:{}\n", i + 1, line));
            }
            if offset + limit < total {
                out.push_str(&format!(
                    "\n[showing lines {}-{} of {}]",
                    offset + 1,
                    (offset + limit).min(total),
                    total
                ));
            }
            Ok(out)
        }

        // -----------------------------------------------------------
        // write_file — create or overwrite a file
        // -----------------------------------------------------------
        "write_file" => {
            let path = input["path"].as_str().ok_or("missing 'path'")?;
            let content = input["content"].as_str().ok_or("missing 'content'")?;
            let full_path = resolve_path(&repo.repo_path, path);

            // Create parent directories if needed
            if let Some(parent) = std::path::Path::new(&full_path).parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create directories: {}", e))?;
                }
            }

            std::fs::write(&full_path, content)
                .map_err(|e| format!("Failed to write '{}': {}", path, e))?;

            Ok(format!("Wrote {} bytes to {}", content.len(), path))
        }

        // -----------------------------------------------------------
        // edit_file — exact string replacement (must be unique)
        // -----------------------------------------------------------
        "edit_file" => {
            let path = input["path"].as_str().ok_or("missing 'path'")?;
            let old_string = input["old_string"].as_str().ok_or("missing 'old_string'")?;
            let new_string = input["new_string"].as_str().ok_or("missing 'new_string'")?;
            let full_path = resolve_path(&repo.repo_path, path);

            let content = std::fs::read_to_string(&full_path)
                .map_err(|e| format!("Failed to read '{}': {}", path, e))?;

            let count = content.matches(old_string).count();
            if count == 0 {
                return Err(format!(
                    "old_string not found in {}. Read the file first to see exact content.",
                    path
                ));
            }
            if count > 1 {
                return Err(format!(
                    "old_string found {} times in {} — must be unique. Include more surrounding context to make it unique.",
                    count, path
                ));
            }

            let new_content = content.replacen(old_string, new_string, 1);
            std::fs::write(&full_path, &new_content)
                .map_err(|e| format!("Failed to write '{}': {}", path, e))?;

            Ok(format!("Edited {} — replaced 1 occurrence", path))
        }

        // -----------------------------------------------------------
        // bash — execute shell commands
        // -----------------------------------------------------------
        "bash" => {
            let command = input["command"].as_str().ok_or("missing 'command'")?;
            let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(120).min(600);
            let timeout = std::time::Duration::from_secs(timeout_secs);

            let mut child = std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&repo.repo_path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to execute command: {}", e))?;

            // Wait with timeout
            let start = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let mut stdout = String::new();
                        if let Some(mut out) = child.stdout.take() {
                            out.read_to_string(&mut stdout).ok();
                        }
                        let mut stderr = String::new();
                        if let Some(mut err) = child.stderr.take() {
                            err.read_to_string(&mut stderr).ok();
                        }

                        let mut result = String::new();
                        if !stdout.is_empty() {
                            result.push_str(&stdout);
                        }
                        if !stderr.is_empty() {
                            if !result.is_empty() {
                                result.push('\n');
                            }
                            result.push_str(&stderr);
                        }
                        if !status.success() {
                            if !result.is_empty() {
                                result.push('\n');
                            }
                            result.push_str(&format!(
                                "[exit code: {}]",
                                status.code().unwrap_or(-1)
                            ));
                        }
                        if result.is_empty() {
                            result = "(no output)".to_string();
                        }
                        return Ok(result);
                    }
                    Ok(None) => {
                        if start.elapsed() > timeout {
                            let _ = child.kill();
                            return Err(format!(
                                "Command timed out after {}s: {}",
                                timeout_secs, command
                            ));
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(e) => {
                        return Err(format!("Failed to wait for process: {}", e));
                    }
                }
            }
        }

        // -----------------------------------------------------------
        // list_directory — list directory contents
        // -----------------------------------------------------------
        "list_directory" => {
            let path = input["path"]
                .as_str()
                .unwrap_or(".");
            let full_path = resolve_path(&repo.repo_path, path);

            let entries = std::fs::read_dir(&full_path)
                .map_err(|e| format!("Failed to read directory '{}': {}", path, e))?;

            let mut dirs: Vec<String> = Vec::new();
            let mut files: Vec<String> = Vec::new();

            for entry in entries {
                let entry = entry.map_err(|e| format!("Error reading entry: {}", e))?;
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip hidden files unless specifically requested
                if name.starts_with('.') {
                    continue;
                }
                if entry.path().is_dir() {
                    dirs.push(format!("{}/", name));
                } else {
                    files.push(name);
                }
            }

            dirs.sort();
            files.sort();

            let mut out = String::new();
            for d in &dirs {
                out.push_str(d);
                out.push('\n');
            }
            for f in &files {
                out.push_str(f);
                out.push('\n');
            }
            if out.is_empty() {
                out = "(empty directory)".to_string();
            }
            Ok(out)
        }

        // -----------------------------------------------------------
        // grep_files — search file contents with regex
        // -----------------------------------------------------------
        "grep_files" => {
            let pattern = input["pattern"].as_str().ok_or("missing 'pattern'")?;
            let search_path = input["path"].as_str().unwrap_or(".");
            let full_path = resolve_path(&repo.repo_path, search_path);
            let max_results = input["max_results"].as_u64().unwrap_or(50) as usize;
            let include = input["include"].as_str();

            // Try ripgrep first (faster), fall back to grep
            let mut cmd = if which_exists("rg") {
                let mut c = std::process::Command::new("rg");
                c.arg("--no-heading")
                    .arg("--line-number")
                    .arg("--max-count")
                    .arg(max_results.to_string());
                if let Some(glob) = include {
                    c.arg("--glob").arg(glob);
                }
                c.arg(pattern).arg(&full_path);
                c
            } else {
                let mut c = std::process::Command::new("grep");
                c.arg("-rn")
                    .arg("--max-count")
                    .arg(max_results.to_string());
                if let Some(glob) = include {
                    c.arg("--include").arg(glob);
                }
                c.arg(pattern).arg(&full_path);
                c
            };

            let output = cmd
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .map_err(|e| format!("Failed to run search: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.is_empty() {
                return Ok(format!("No matches found for '{}'", pattern));
            }

            // Strip repo path prefix from results for readability
            let result: String = stdout
                .lines()
                .take(max_results)
                .map(|line| {
                    line.strip_prefix(&repo.repo_path)
                        .and_then(|l| l.strip_prefix('/'))
                        .unwrap_or(line)
                })
                .collect::<Vec<_>>()
                .join("\n");

            Ok(result)
        }

        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

/// Check if a command exists on PATH.
fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
