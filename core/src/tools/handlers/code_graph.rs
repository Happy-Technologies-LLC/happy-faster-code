use async_trait::async_trait;
use codex_protocol::models::FunctionCallOutputBody;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::spec::JsonSchema;

use happy_core::graph::RepositoryGraph;
use happy_core::vector::bm25::BM25Index;

// ── Tool spec metadata ─────────────────────────────────────────

type ParamsFn = fn() -> JsonSchema;

/// Tool names, descriptions, and parameter schema factories for registration in spec.rs.
pub static CODE_GRAPH_TOOL_NAMES: &[(&str, &str, ParamsFn)] = &[
    (
        "find_callers",
        "Find all functions/methods that call a given symbol in the indexed codebase.",
        params_symbol as ParamsFn,
    ),
    (
        "find_callees",
        "Find all functions/methods called by a given symbol in the indexed codebase.",
        params_symbol,
    ),
    (
        "get_dependencies",
        "Get all files imported by a given file path in the indexed codebase.",
        params_file_path,
    ),
    (
        "get_dependents",
        "Get all files that import a given file path in the indexed codebase.",
        params_file_path,
    ),
    (
        "get_subclasses",
        "Find all classes that inherit from a given class in the indexed codebase.",
        params_symbol,
    ),
    (
        "get_superclasses",
        "Find all parent classes of a given class in the indexed codebase.",
        params_symbol,
    ),
    (
        "find_code_path",
        "Find the shortest path between two symbols in the code graph.",
        params_source_target,
    ),
    (
        "get_related",
        "Find all elements within N hops of a symbol in the code graph.",
        params_related,
    ),
    (
        "search_code",
        "BM25 keyword search across all indexed code elements.",
        params_search,
    ),
    (
        "get_code_source",
        "Get the source code of a specific indexed element by its ID or name.",
        params_symbol,
    ),
    (
        "repo_stats",
        "Get statistics about the indexed codebase (node/edge/file counts).",
        params_empty,
    ),
    (
        "list_indexed_files",
        "List all files that have been indexed in the code graph.",
        params_empty,
    ),
    (
        "rlm_analyze",
        "Run a deep recursive analysis query against the indexed codebase using the RLM orchestrator. Use for complex multi-step questions requiring call chain tracing, dependency analysis, or architectural pattern understanding.",
        params_rlm_analyze,
    ),
];

fn params_symbol() -> JsonSchema {
    JsonSchema::Object {
        properties: BTreeMap::from([(
            "symbol".to_string(),
            JsonSchema::String {
                description: Some(
                    "The symbol name to query (function, class, method, etc).".to_string(),
                ),
            },
        )]),
        required: Some(vec!["symbol".to_string()]),
        additional_properties: Some(false.into()),
    }
}

fn params_file_path() -> JsonSchema {
    JsonSchema::Object {
        properties: BTreeMap::from([(
            "file_path".to_string(),
            JsonSchema::String {
                description: Some("The file path to query.".to_string()),
            },
        )]),
        required: Some(vec!["file_path".to_string()]),
        additional_properties: Some(false.into()),
    }
}

fn params_source_target() -> JsonSchema {
    JsonSchema::Object {
        properties: BTreeMap::from([
            (
                "source".to_string(),
                JsonSchema::String {
                    description: Some("The source symbol name.".to_string()),
                },
            ),
            (
                "target".to_string(),
                JsonSchema::String {
                    description: Some("The target symbol name.".to_string()),
                },
            ),
        ]),
        required: Some(vec!["source".to_string(), "target".to_string()]),
        additional_properties: Some(false.into()),
    }
}

fn params_related() -> JsonSchema {
    JsonSchema::Object {
        properties: BTreeMap::from([
            (
                "symbol".to_string(),
                JsonSchema::String {
                    description: Some("The symbol name to find related elements for.".to_string()),
                },
            ),
            (
                "max_hops".to_string(),
                JsonSchema::Number {
                    description: Some(
                        "Maximum number of hops in the graph (default: 2).".to_string(),
                    ),
                },
            ),
        ]),
        required: Some(vec!["symbol".to_string()]),
        additional_properties: Some(false.into()),
    }
}

fn params_search() -> JsonSchema {
    JsonSchema::Object {
        properties: BTreeMap::from([
            (
                "query".to_string(),
                JsonSchema::String {
                    description: Some("The search query string.".to_string()),
                },
            ),
            (
                "limit".to_string(),
                JsonSchema::Number {
                    description: Some(
                        "Maximum number of results to return (default: 10).".to_string(),
                    ),
                },
            ),
        ]),
        required: Some(vec!["query".to_string()]),
        additional_properties: Some(false.into()),
    }
}

fn params_empty() -> JsonSchema {
    JsonSchema::Object {
        properties: BTreeMap::new(),
        required: None,
        additional_properties: Some(false.into()),
    }
}

fn params_rlm_analyze() -> JsonSchema {
    JsonSchema::Object {
        properties: BTreeMap::from([
            (
                "query".to_string(),
                JsonSchema::String {
                    description: Some(
                        "The analysis query to run against the codebase.".to_string(),
                    ),
                },
            ),
            (
                "max_depth".to_string(),
                JsonSchema::Number {
                    description: Some(
                        "Maximum recursion depth for sub-queries (default: 3).".to_string(),
                    ),
                },
            ),
        ]),
        required: Some(vec!["query".to_string()]),
        additional_properties: Some(false.into()),
    }
}

/// Shared handle to the indexed repo state, initialized once at startup.
pub struct RepoHandle {
    pub graph: RepositoryGraph,
    pub bm25: BM25Index,
}

/// Lazy-init shared state: starts as None, populated after indexing.
pub type SharedRepoHandle = Arc<RwLock<Option<RepoHandle>>>;

// ── Argument structs ───────────────────────────────────────────

#[derive(Deserialize)]
struct SymbolArg {
    symbol: String,
}

#[derive(Deserialize)]
struct FileArg {
    file_path: String,
}

#[derive(Deserialize)]
struct FindPathArgs {
    source: String,
    target: String,
}

#[derive(Deserialize)]
struct GetRelatedArgs {
    symbol: String,
    #[serde(default = "default_max_hops")]
    max_hops: usize,
}

fn default_max_hops() -> usize {
    2
}

#[derive(Deserialize)]
struct SearchCodeArgs {
    query: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize {
    10
}

#[derive(Deserialize)]
struct RlmAnalyzeArgs {
    query: String,
    #[serde(default = "default_rlm_max_depth")]
    max_depth: usize,
}

fn default_rlm_max_depth() -> usize {
    3
}

#[derive(Deserialize)]
struct GraphRpcRequest {
    token: String,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct GraphRpcResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl GraphRpcResponse {
    fn ok(result: Value) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(message.into()),
        }
    }
}

// ── Handler dispatch ───────────────────────────────────────────

/// Dispatcher that routes tool_name to the right happy-core query.
/// Holds an `Arc<RwLock<Option<RepoHandle>>>` so it can be registered
/// at startup and populated later when the repo is indexed.
pub struct CodeGraphDispatcher {
    pub repo: SharedRepoHandle,
}

impl CodeGraphDispatcher {
    pub fn new(repo: SharedRepoHandle) -> Self {
        Self { repo }
    }

    pub async fn dispatch(
        &self,
        tool_name: &str,
        arguments: &str,
        conversation_id: Option<&str>,
    ) -> Result<String, FunctionCallError> {
        // rlm_analyze is handled separately because it invokes the Python
        // orchestrator and passes a serialized snapshot of the current graph.
        if tool_name == "rlm_analyze" {
            return self.dispatch_rlm_analyze(arguments, conversation_id).await;
        }

        let guard = self.repo.read().await;
        let repo = guard.as_ref().ok_or_else(|| {
            FunctionCallError::RespondToModel(
                "No repository has been indexed yet. The code graph tools require an indexed \
                 codebase. Wait for auto-indexing to complete and retry."
                    .to_string(),
            )
        })?;
        match tool_name {
            "find_callers" => {
                let args: SymbolArg = parse_arguments(arguments)?;
                let results = repo.graph.find_callers(&args.symbol);
                Ok(format_nodes(&results))
            }
            "find_callees" => {
                let args: SymbolArg = parse_arguments(arguments)?;
                let results = repo.graph.find_callees(&args.symbol);
                Ok(format_nodes(&results))
            }
            "get_dependencies" => {
                let args: FileArg = parse_arguments(arguments)?;
                let results = repo.graph.get_dependencies(&args.file_path);
                Ok(format_nodes(&results))
            }
            "get_dependents" => {
                let args: FileArg = parse_arguments(arguments)?;
                let results = repo.graph.get_dependents(&args.file_path);
                Ok(format_nodes(&results))
            }
            "get_subclasses" => {
                let args: SymbolArg = parse_arguments(arguments)?;
                let results = repo.graph.get_subclasses(&args.symbol);
                Ok(format_nodes(&results))
            }
            "get_superclasses" => {
                let args: SymbolArg = parse_arguments(arguments)?;
                let results = repo.graph.get_superclasses(&args.symbol);
                Ok(format_nodes(&results))
            }
            "find_code_path" => {
                let args: FindPathArgs = parse_arguments(arguments)?;
                match repo.graph.find_path(&args.source, &args.target, None) {
                    Some(path) => Ok(json!({
                        "source": args.source,
                        "target": args.target,
                        "path": path,
                        "hops": path.len().saturating_sub(1),
                    })
                    .to_string()),
                    None => Ok(json!({
                        "source": args.source,
                        "target": args.target,
                        "path": null,
                        "message": "no path found between source and target",
                    })
                    .to_string()),
                }
            }
            "get_related" => {
                let args: GetRelatedArgs = parse_arguments(arguments)?;
                let results = repo.graph.get_related(&args.symbol, args.max_hops);
                Ok(format_nodes(&results))
            }
            "search_code" => {
                let args: SearchCodeArgs = parse_arguments(arguments)?;
                let results = repo.bm25.search(&args.query, args.limit);
                let output: Vec<serde_json::Value> = results
                    .iter()
                    .map(|(id, score)| {
                        json!({
                            "element_id": id,
                            "score": score,
                        })
                    })
                    .collect();
                Ok(json!({
                    "query": args.query,
                    "results": output,
                    "total": results.len(),
                })
                .to_string())
            }
            "get_code_source" => {
                let args: SymbolArg = parse_arguments(arguments)?;
                match repo.graph.get_source(&args.symbol) {
                    Some(source) => Ok(source),
                    None => Err(FunctionCallError::RespondToModel(format!(
                        "element '{}' not found in index",
                        args.symbol
                    ))),
                }
            }
            "repo_stats" => {
                let stats = repo.graph.stats();
                Ok(json!({
                    "total_nodes": stats.node_count,
                    "total_edges": stats.edge_count,
                    "files": stats.file_count,
                    "elements": stats.element_count,
                })
                .to_string())
            }
            "list_indexed_files" => {
                let files = repo.graph.file_paths();
                Ok(json!({
                    "total": files.len(),
                    "files": files,
                })
                .to_string())
            }
            _ => Err(FunctionCallError::Fatal(format!(
                "unknown code graph tool: {tool_name}"
            ))),
        }
    }

    async fn dispatch_rlm_analyze(
        &self,
        arguments: &str,
        conversation_id: Option<&str>,
    ) -> Result<String, FunctionCallError> {
        let args: RlmAnalyzeArgs = parse_arguments(arguments)?;
        let cwd = std::env::current_dir().map_err(|err| {
            FunctionCallError::Fatal(format!("failed to determine working directory: {err}"))
        })?;
        let cwd_str = cwd.to_string_lossy().to_string();
        let snapshot_dir = tempfile::Builder::new()
            .prefix("happycode-rlm-")
            .tempdir()
            .map_err(|err| {
                FunctionCallError::Fatal(format!(
                    "failed to create temporary snapshot directory: {err}"
                ))
            })?;
        let elements_path = snapshot_dir.path().join("elements.bin");
        {
            let guard = self.repo.read().await;
            let repo = guard.as_ref().ok_or_else(|| {
                FunctionCallError::RespondToModel(
                    "No repository has been indexed yet. The code graph tools require an indexed \
                     codebase. Wait for auto-indexing to complete and retry."
                        .to_string(),
                )
            })?;
            let elements = repo.graph.all_elements();
            happy_core::store::save_elements(&elements, &elements_path).map_err(|err| {
                FunctionCallError::Fatal(format!(
                    "failed to snapshot indexed elements for rlm_analyze: {err}"
                ))
            })?;
        }
        let elements_path_str = elements_path.to_string_lossy().to_string();

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|err| {
            FunctionCallError::Fatal(format!(
                "failed to start graph RPC listener for rlm_analyze: {err}"
            ))
        })?;
        let endpoint = listener.local_addr().map_err(|err| {
            FunctionCallError::Fatal(format!("failed to read graph RPC listener address: {err}"))
        })?;
        let endpoint_str = endpoint.to_string();
        let rpc_token = Uuid::new_v4().to_string();
        let server_repo = self.repo.clone();
        let server_token = rpc_token.clone();
        let server_task = tokio::spawn(async move {
            match listener.accept().await {
                Ok((socket, _addr)) => {
                    if let Err(err) =
                        handle_graph_rpc_client(socket, server_repo, server_token).await
                    {
                        tracing::warn!(error = %err, "rlm_analyze graph RPC session failed");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "rlm_analyze graph RPC accept failed");
                }
            }
        });

        let output_result = tokio::process::Command::new("python3")
            .args([
                "-m",
                "happy_faster_code.orchestrator",
                "--path",
                &cwd_str,
                "--query",
                &args.query,
                "--max-depth",
                &args.max_depth.to_string(),
                "--graph-rpc-endpoint",
                &endpoint_str,
                "--graph-rpc-token",
                &rpc_token,
                "--elements-file",
                &elements_path_str,
                "--json",
                "--quiet",
            ])
            .current_dir(&cwd)
            .args({
                let mut extra_args = Vec::new();
                if let Some(cid) = conversation_id {
                    extra_args.push("--volt-conversation-id".to_string());
                    extra_args.push(cid.to_string());
                }
                extra_args
            })
            .output()
            .await;

        server_task.abort();
        let _ = server_task.await;

        let output = output_result.map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "Failed to invoke RLM orchestrator: {err}. \
                 Ensure happycode Python package is installed: \
                 pip install -e . (from the repo root)"
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FunctionCallError::RespondToModel(format!(
                "RLM orchestrator exited with status {}: {}",
                output.status, stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn required_string(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("missing or invalid '{key}' parameter"))
}

fn optional_usize(params: &Value, key: &str, default: usize) -> usize {
    params
        .get(key)
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(default)
}

fn dispatch_graph_rpc_method(
    repo: &RepoHandle,
    method: &str,
    params: &Value,
) -> Result<Value, String> {
    match method {
        "find_callers" => {
            let symbol = required_string(params, "symbol")?;
            Ok(json!(
                repo.graph
                    .find_callers(&symbol)
                    .into_iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            ))
        }
        "find_callees" => {
            let symbol = required_string(params, "symbol")?;
            Ok(json!(
                repo.graph
                    .find_callees(&symbol)
                    .into_iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            ))
        }
        "get_dependencies" => {
            let file_path = required_string(params, "file_path")?;
            Ok(json!(
                repo.graph
                    .get_dependencies(&file_path)
                    .into_iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            ))
        }
        "get_dependents" => {
            let file_path = required_string(params, "file_path")?;
            Ok(json!(
                repo.graph
                    .get_dependents(&file_path)
                    .into_iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            ))
        }
        "get_subclasses" => {
            let class_name = required_string(params, "class_name")?;
            Ok(json!(
                repo.graph
                    .get_subclasses(&class_name)
                    .into_iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            ))
        }
        "get_superclasses" => {
            let class_name = required_string(params, "class_name")?;
            Ok(json!(
                repo.graph
                    .get_superclasses(&class_name)
                    .into_iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            ))
        }
        "find_path" => {
            let source = required_string(params, "source")?;
            let target = required_string(params, "target")?;
            Ok(json!(repo.graph.find_path(&source, &target, None)))
        }
        "get_related" => {
            let element = required_string(params, "element")?;
            let max_hops = optional_usize(params, "max_hops", 2);
            Ok(json!(
                repo.graph
                    .get_related(&element, max_hops)
                    .into_iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            ))
        }
        "search" => {
            let query = required_string(params, "query")?;
            let k = optional_usize(params, "k", 10);
            Ok(json!(repo.bm25.search(&query, k)))
        }
        "get_source" => {
            let element_id = required_string(params, "element_id")?;
            Ok(json!(repo.graph.get_source(&element_id)))
        }
        "file_tree" => {
            let mut files = repo.graph.file_paths();
            files.sort();
            Ok(json!(files))
        }
        "stats" => {
            let stats = repo.graph.stats();
            Ok(json!({
                "nodes": stats.node_count,
                "edges": stats.edge_count,
                "files": stats.file_count,
                "elements": stats.element_count,
                "bm25_docs": repo.bm25.len(),
                "has_vectors": false,
            }))
        }
        "resolve_symbol" => {
            let symbol = required_string(params, "symbol")?;
            Ok(json!(repo.graph.resolve_symbol(&symbol)))
        }
        "resolve_module" => {
            let module_name = required_string(params, "module_name")?;
            Ok(json!(repo.graph.resolve_module(&module_name)))
        }
        _ => Err(format!("unknown graph RPC method: {method}")),
    }
}

async fn handle_graph_rpc_client(
    socket: TcpStream,
    repo_handle: SharedRepoHandle,
    expected_token: String,
) -> Result<(), String> {
    let (reader_half, mut writer_half) = socket.into_split();
    let mut reader = BufReader::new(reader_half);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .await
            .map_err(|err| format!("failed to read graph RPC request: {err}"))?;
        if bytes == 0 {
            break;
        }

        let response = match serde_json::from_str::<GraphRpcRequest>(line.trim_end()) {
            Ok(request) => {
                if request.token != expected_token {
                    GraphRpcResponse::error("unauthorized graph RPC token")
                } else {
                    let guard = repo_handle.read().await;
                    if let Some(repo) = guard.as_ref() {
                        match dispatch_graph_rpc_method(repo, &request.method, &request.params) {
                            Ok(result) => GraphRpcResponse::ok(result),
                            Err(err) => GraphRpcResponse::error(err),
                        }
                    } else {
                        GraphRpcResponse::error(
                            "No repository has been indexed yet. Wait for auto-indexing to complete and retry.",
                        )
                    }
                }
            }
            Err(err) => GraphRpcResponse::error(format!("invalid graph RPC request: {err}")),
        };

        let encoded = serde_json::to_string(&response)
            .map_err(|err| format!("failed to encode graph RPC response: {err}"))?;
        writer_half
            .write_all(encoded.as_bytes())
            .await
            .map_err(|err| format!("failed to write graph RPC response: {err}"))?;
        writer_half
            .write_all(b"\n")
            .await
            .map_err(|err| format!("failed to write graph RPC newline: {err}"))?;
    }

    Ok(())
}

fn format_nodes(nodes: &[&happy_core::graph::types::GraphNode]) -> String {
    if nodes.is_empty() {
        return json!({ "results": [], "total": 0 }).to_string();
    }
    let items: Vec<serde_json::Value> = nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "kind": format!("{:?}", n.kind),
                "name": n.name.as_str(),
                "file_path": n.file_path,
                "start_line": n.start_line,
                "end_line": n.end_line,
            })
        })
        .collect();
    json!({
        "results": items,
        "total": items.len(),
    })
    .to_string()
}

// ── ToolHandler implementation ─────────────────────────────────

/// Single handler struct that delegates to CodeGraphDispatcher based on tool_name.
pub struct CodeGraphToolHandler {
    pub dispatcher: Arc<CodeGraphDispatcher>,
}

#[async_trait]
impl ToolHandler for CodeGraphToolHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<ToolOutput, FunctionCallError> {
        let arguments = match &invocation.payload {
            ToolPayload::Function { arguments } => arguments.clone(),
            _ => {
                return Err(FunctionCallError::Fatal(
                    "code graph handler received unsupported payload".to_string(),
                ));
            }
        };

        let conversation_id = invocation.session.conversation_id.to_string();
        let result = self
            .dispatcher
            .dispatch(
                &invocation.tool_name,
                &arguments,
                Some(conversation_id.as_str()),
            )
            .await?;

        Ok(ToolOutput::Function {
            body: FunctionCallOutputBody::Text(result),
            success: Some(true),
        })
    }
}

// ── Background repo indexing ───────────────────────────────────

/// Spawn a background task that indexes the repository at `cwd` using happy-core
/// and populates the shared `RepoHandle`. This runs entirely off the critical path
/// so the session is interactive immediately while indexing proceeds.
pub fn start_code_graph_indexing(repo_handle: SharedRepoHandle, cwd: std::path::PathBuf) {
    let watcher_handle = repo_handle.clone();
    let watcher_cwd = cwd.clone();

    tokio::spawn(async move {
        let path_str = cwd.to_string_lossy().to_string();
        tracing::info!(path = %path_str, "starting background code graph indexing");

        // Run the CPU-intensive indexing on a blocking thread to avoid starving
        // the async runtime.
        let result = tokio::task::spawn_blocking(move || {
            let elements = happy_core::indexer::walk_and_index(&path_str);
            if elements.is_empty() {
                tracing::warn!("code graph indexing found no elements");
                return None;
            }
            tracing::info!(
                count = elements.len(),
                "indexed code elements, building graph"
            );

            let mut graph = RepositoryGraph::new();
            graph.build_from_elements(&elements, &path_str);

            let mut bm25 = BM25Index::new();
            for elem in &elements {
                let text = format!(
                    "{} {} {}",
                    elem.name,
                    elem.code,
                    elem.docstring.as_deref().unwrap_or("")
                );
                bm25.add_document(&elem.id, &text);
            }

            let stats = graph.stats();
            tracing::info!(
                nodes = stats.node_count,
                edges = stats.edge_count,
                files = stats.file_count,
                "code graph built successfully"
            );
            Some(RepoHandle { graph, bm25 })
        })
        .await;

        match result {
            Ok(Some(handle)) => {
                let mut guard = repo_handle.write().await;
                *guard = Some(handle);
                tracing::info!("code graph repo handle populated");
            }
            Ok(None) => {
                tracing::info!("code graph indexing produced no results (empty repo?)");
            }
            Err(err) => {
                tracing::error!(error = %err, "code graph indexing task panicked");
            }
        }
    });

    // Spawn file watcher for incremental re-indexing
    start_file_watcher(watcher_handle, watcher_cwd);
}

/// Spawn a background task that watches for file changes and incrementally
/// updates the code graph and BM25 index.
fn start_file_watcher(repo_handle: SharedRepoHandle, cwd: std::path::PathBuf) {
    tokio::spawn(async move {
        // Wait for initial indexing to complete
        loop {
            if repo_handle.read().await.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let path_str = cwd.to_string_lossy().to_string();
        let watcher = match happy_core::watcher::FileWatcher::new(&path_str) {
            Ok(w) => w,
            Err(err) => {
                tracing::warn!(error = %err, "failed to start file watcher for incremental re-indexing");
                return;
            }
        };
        tracing::info!("file watcher started for incremental re-indexing");

        loop {
            // Sleep for debouncing — batch rapid changes
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            let mut changed_files = std::collections::HashSet::new();
            let mut removed_files = Vec::new();

            // Drain all pending events
            while let Some(event) = watcher.try_recv() {
                match event {
                    happy_core::watcher::WatchEvent::Modified(path)
                    | happy_core::watcher::WatchEvent::Created(path) => {
                        if happy_core::parser::languages::SupportedLanguage::from_extension(&path)
                            .is_some()
                        {
                            changed_files.insert(path);
                        }
                    }
                    happy_core::watcher::WatchEvent::Removed(path) => {
                        removed_files.push(path);
                    }
                }
            }

            if changed_files.is_empty() && removed_files.is_empty() {
                continue;
            }

            // Acquire write lock and apply updates
            let mut guard = repo_handle.write().await;
            if let Some(handle) = guard.as_mut() {
                for path in &removed_files {
                    // Remove BM25 entries before removing from graph
                    for id in handle.graph.element_ids_for_file(path) {
                        handle.bm25.remove_document(&id);
                    }
                    handle.graph.remove_file(path);
                    tracing::debug!(path = %path, "removed file from code graph");
                }

                for path in &changed_files {
                    let repo_root = path_str.clone();
                    if let Some(new_elements) =
                        happy_core::indexer::index_single_file(path, &repo_root)
                    {
                        // Remove old BM25 entries
                        for id in handle.graph.element_ids_for_file(path) {
                            handle.bm25.remove_document(&id);
                        }

                        // Update graph (removes old, adds new)
                        handle.graph.update_file(path, &new_elements, &repo_root);

                        // Add new BM25 entries
                        for elem in &new_elements {
                            let text = format!(
                                "{} {} {}",
                                elem.name,
                                elem.code,
                                elem.docstring.as_deref().unwrap_or("")
                            );
                            handle.bm25.add_document(&elem.id, &text);
                        }

                        tracing::debug!(
                            path = %path,
                            elements = new_elements.len(),
                            "updated file in code graph"
                        );
                    }
                }
            }
        }
    });
}
