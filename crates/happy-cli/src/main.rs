mod agent;
mod config;
mod provider;
mod repo;
mod tools;
mod tui;

use clap::{Parser, Subcommand};
use std::path::Path;
use std::time::Instant;

use happy_core::graph::RepositoryGraph;
use happy_core::indexer;
use happy_core::store;
use happy_core::vector::BM25Index;

const HAPPY_DIR: &str = ".happy";

#[derive(Parser)]
#[command(name = "happycode", about = "HappyFasterCode")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the repository (used when no subcommand is given)
    #[arg(default_value = ".")]
    path: Option<String>,
    /// LLM provider (anthropic or openai)
    #[arg(long)]
    provider: Option<String>,
    /// Model name
    #[arg(long)]
    model: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive AI chat about a codebase
    Chat {
        /// Path to the repository
        #[arg(default_value = ".")]
        path: String,
        /// LLM provider (anthropic or openai)
        #[arg(long)]
        provider: Option<String>,
        /// Model name
        #[arg(long)]
        model: Option<String>,
    },
    /// Index a repository and save to .happy/ directory
    Index {
        /// Path to the repository
        path: String,
    },
    /// Query the code graph
    Query {
        /// Path to the repository
        path: String,
        /// Symbol to query
        symbol: String,
        /// Query type: callers, callees, deps, dependents, subclasses, superclasses
        #[arg(short = 't', long, default_value = "callers")]
        query_type: String,
    },
    /// Show repository statistics
    Stats {
        /// Path to the repository
        path: String,
    },
    /// Search for code
    Search {
        /// Path to the repository
        path: String,
        /// Search query
        query: String,
        /// Number of results
        #[arg(short, long, default_value = "10")]
        k: usize,
    },
    /// Watch for file changes and re-index
    Watch {
        /// Path to the repository
        path: String,
    },
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        // No subcommand → default to chat
        None => {
            let path = cli.path.unwrap_or_else(|| ".".into());
            if let Err(e) = cmd_chat(&path, cli.provider, cli.model).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Chat {
            path,
            provider,
            model,
        }) => {
            if let Err(e) = cmd_chat(&path, provider, model).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Index { path }) => cmd_index(&path),
        Some(Commands::Query {
            path,
            symbol,
            query_type,
        }) => cmd_query(&path, &symbol, &query_type),
        Some(Commands::Stats { path }) => cmd_stats(&path),
        Some(Commands::Search { path, query, k }) => cmd_search(&path, &query, k),
        Some(Commands::Watch { path }) => cmd_watch(&path),
    }
}

async fn cmd_chat(
    path: &str,
    provider_override: Option<String>,
    model_override: Option<String>,
) -> anyhow::Result<()> {
    use crate::config::{AgentConfig, ProviderKind};
    use crate::provider::anthropic::AnthropicProvider;
    use crate::provider::openai::OpenAIProvider;

    // Show color logo on startup (true-color ANSI art)
    const LOGO: &str = include_str!("../../../logo_color.txt");
    eprintln!("{}", LOGO);
    eprintln!();

    let mut config = AgentConfig::load(Some(path))?;

    // If no API key found from env/config, prompt interactively
    if config.api_key.is_empty() {
        config = AgentConfig::prompt_setup()?;
        config.save(path)?;
        eprintln!();
    }

    if let Some(p) = provider_override {
        config.provider = match p.to_lowercase().as_str() {
            "openai" => ProviderKind::OpenAI,
            _ => ProviderKind::Anthropic,
        };
    }
    if let Some(m) = model_override {
        config.model = m;
    }

    eprintln!(
        "Using {} ({})",
        config.model,
        match config.provider {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenAI => "openai",
        }
    );

    eprintln!("Indexing {}...", path);
    let repo = repo::RepoContext::load(path)?;
    repo.save_cache()?;

    let llm_provider: Box<dyn provider::LlmProvider> = match config.provider {
        ProviderKind::Anthropic => Box::new(AnthropicProvider::new(
            config.api_key.clone(),
            config.model.clone(),
            config.api_base.clone(),
        )),
        ProviderKind::OpenAI => Box::new(OpenAIProvider::new(
            config.api_key.clone(),
            config.model.clone(),
            config.api_base.clone(),
        )),
    };

    let agent = agent::Agent::new(llm_provider, config);
    tui::run(agent, repo).await
}

// ---------------------------------------------------------------------------
// Existing commands (unchanged)
// ---------------------------------------------------------------------------

/// Get the .happy/ directory for a repo, creating it if needed.
fn happy_dir(repo_path: &str) -> std::path::PathBuf {
    let dir = Path::new(repo_path).join(HAPPY_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok();
    }
    dir
}

/// Try to load cached elements from .happy/ directory.
fn try_load_cache(repo_path: &str) -> Option<Vec<happy_core::indexer::CodeElement>> {
    let dir = happy_dir(repo_path);
    let elements_path = dir.join("elements.bin");
    if elements_path.exists() {
        match store::load_elements(&elements_path) {
            Ok(elements) => {
                eprintln!("Loaded {} cached elements from .happy/", elements.len());
                Some(elements)
            }
            Err(e) => {
                eprintln!("Cache load failed ({}), re-indexing...", e);
                None
            }
        }
    } else {
        None
    }
}

/// Save elements and BM25 index to .happy/ directory.
fn save_cache(
    repo_path: &str,
    elements: &[happy_core::indexer::CodeElement],
    bm25: &BM25Index,
) {
    let dir = happy_dir(repo_path);
    if let Err(e) = store::save_elements(elements, &dir.join("elements.bin")) {
        eprintln!("Warning: failed to save elements cache: {}", e);
    }
    if let Err(e) = store::save_bm25(bm25, &dir.join("bm25.bin")) {
        eprintln!("Warning: failed to save BM25 cache: {}", e);
    }
    eprintln!("Saved index to {}/", dir.display());
}

fn build_repo(
    path: &str,
    use_cache: bool,
) -> (Vec<happy_core::indexer::CodeElement>, RepositoryGraph, BM25Index) {
    let elements = if use_cache {
        try_load_cache(path)
    } else {
        None
    };

    let elements = match elements {
        Some(e) => e,
        None => {
            let start = Instant::now();
            let elements = indexer::walk_and_index(path);
            eprintln!("Indexed {} elements in {:.2?}", elements.len(), start.elapsed());
            elements
        }
    };

    let start = Instant::now();
    let mut graph = RepositoryGraph::new();
    graph.build_from_elements(&elements);
    eprintln!("Built graph in {:.2?}", start.elapsed());

    let start = Instant::now();
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
    eprintln!("Built BM25 index in {:.2?}", start.elapsed());

    (elements, graph, bm25)
}

fn cmd_index(path: &str) {
    let (elements, graph, bm25) = build_repo(path, false);
    save_cache(path, &elements, &bm25);

    let stats = graph.stats();
    println!("Indexing complete:");
    println!("  Elements: {}", elements.len());
    println!("  Nodes: {}", stats.node_count);
    println!("  Edges: {}", stats.edge_count);
    println!("  Files: {}", stats.file_count);
}

fn cmd_query(path: &str, symbol: &str, query_type: &str) {
    let (_elements, graph, _bm25) = build_repo(path, true);

    let results: Vec<String> = match query_type {
        "callers" => graph
            .find_callers(symbol)
            .into_iter()
            .map(|n| format!("{} ({}:{})", n.name, n.file_path, n.start_line))
            .collect(),
        "callees" => graph
            .find_callees(symbol)
            .into_iter()
            .map(|n| format!("{} ({}:{})", n.name, n.file_path, n.start_line))
            .collect(),
        "deps" => graph
            .get_dependencies(symbol)
            .into_iter()
            .map(|n| format!("{} ({})", n.name, n.file_path))
            .collect(),
        "dependents" => graph
            .get_dependents(symbol)
            .into_iter()
            .map(|n| format!("{} ({})", n.name, n.file_path))
            .collect(),
        "subclasses" => graph
            .get_subclasses(symbol)
            .into_iter()
            .map(|n| format!("{} ({}:{})", n.name, n.file_path, n.start_line))
            .collect(),
        "superclasses" => graph
            .get_superclasses(symbol)
            .into_iter()
            .map(|n| format!("{} ({}:{})", n.name, n.file_path, n.start_line))
            .collect(),
        _ => {
            eprintln!(
                "Unknown query type: {}. Use: callers, callees, deps, dependents, subclasses, superclasses",
                query_type
            );
            return;
        }
    };

    if results.is_empty() {
        println!("No results for {} '{}'", query_type, symbol);
    } else {
        println!("{} for '{}':", query_type, symbol);
        for r in &results {
            println!("  {}", r);
        }
    }
}

fn cmd_stats(path: &str) {
    let (elements, graph, bm25) = build_repo(path, true);
    let stats = graph.stats();

    println!("Repository: {}", path);
    println!("  Elements: {}", elements.len());
    println!("  Graph nodes: {}", stats.node_count);
    println!("  Graph edges: {}", stats.edge_count);
    println!("  Files: {}", stats.file_count);
    println!("  BM25 documents: {}", bm25.len());

    // Count by type
    let mut type_counts = std::collections::HashMap::new();
    for elem in &elements {
        *type_counts
            .entry(elem.element_type.as_str())
            .or_insert(0usize) += 1;
    }
    println!("  By type:");
    let mut counts: Vec<_> = type_counts.into_iter().collect();
    counts.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (t, c) in counts {
        println!("    {}: {}", t, c);
    }
}

fn cmd_search(path: &str, query: &str, k: usize) {
    let (elements, _graph, bm25) = build_repo(path, true);
    let results = bm25.search(query, k);

    if results.is_empty() {
        println!("No results for '{}'", query);
    } else {
        println!("Search results for '{}':", query);
        for (id, score) in &results {
            let display = elements
                .iter()
                .find(|e| e.id == *id)
                .map(|e| format!("{} [{}] ({}:{})", e.name, e.element_type.as_str(), e.relative_path, e.start_line))
                .unwrap_or_else(|| id.clone());
            println!("  {:.4}  {}", score, display);
        }
    }
}

fn cmd_watch(path: &str) {
    use happy_core::watcher::FileWatcher;
    use std::time::Duration;

    println!("Initial indexing...");
    let (elements, _graph, bm25) = build_repo(path, false);
    save_cache(path, &elements, &bm25);
    println!("Watching for changes... (Ctrl+C to stop)");

    let watcher = match FileWatcher::new(path) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to start watcher: {}", e);
            return;
        }
    };

    let mut pending_reindex = false;
    let mut last_event = Instant::now();

    loop {
        if let Some(event) = watcher.recv_timeout(Duration::from_millis(500)) {
            let path_str = match &event {
                happy_core::watcher::WatchEvent::Modified(p)
                | happy_core::watcher::WatchEvent::Created(p)
                | happy_core::watcher::WatchEvent::Removed(p) => p.clone(),
            };

            if happy_core::parser::languages::SupportedLanguage::from_extension(&path_str).is_some() {
                eprintln!("  Changed: {}", path_str);
                pending_reindex = true;
                last_event = Instant::now();
            }
        }

        if pending_reindex && last_event.elapsed() > Duration::from_secs(1) {
            eprintln!("Re-indexing...");
            let (elements, _graph, bm25) = build_repo(path, false);
            save_cache(path, &elements, &bm25);
            pending_reindex = false;
        }
    }
}
