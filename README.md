<p align="center">
  <img src="logo.svg" alt="happycode" width="600">
</p>

# happycode

**A code-graph-aware AI coding agent built on OpenAI Codex CLI.**

happycode extends [OpenAI's Codex CLI](https://github.com/openai/codex) with a Rust-native structural code graph engine. Before the LLM sees a single token, it builds a **full structural graph** of your entire codebase — every function call, every import chain, every class hierarchy, every dependency edge — then exposes that graph to the LLM as **13 additional tools** on top of Codex's existing read/write/execute capabilities.

The result: the LLM doesn't guess at relationships. It **knows** them.

## Why This Exists

AI coding tools are bottlenecked by context, not intelligence. When you ask "what calls this function?", existing tools grep the codebase, stuff matches into the context window, and hope for the best. This fails on large codebases, indirect calls, cross-file relationships, and anything that requires understanding structure rather than matching text.

happycode adds a structural code graph layer to the Codex agent, giving the LLM precise answers to structural queries with sub-millisecond latency — no grepping, no guessing.

## What Makes It Different

| | happycode | Typical AI CLI |
|---|---|---|
| **Code understanding** | Structural graph (AST-parsed, edge-connected) | Text search (grep/ripgrep) |
| **"Who calls X?"** | Exact answer via graph traversal | Best-effort regex match |
| **Cross-file dependencies** | Full import/dependency graph | File-by-file reading |
| **Class hierarchies** | Inheritance edges, subclass/superclass queries | String matching on `extends`/`implements` |
| **Path between symbols** | Shortest path through call/import/inheritance graph | Not possible |
| **Import resolution** | Module-path-aware, multi-language | N/A |
| **Indexing speed** | Parallel Rust (tree-sitter + rayon), seconds for 100k LoC | N/A |
| **Query latency** | Sub-millisecond graph lookups | Re-reads files each time |

## Agent Tools

happycode gives the LLM all of Codex's built-in tools **plus** 13 code graph navigation tools:

### Code Graph Tools (unique to happycode)

| Tool | What it does |
|------|-------------|
| `search_code` | BM25 keyword search across all indexed code elements |
| `get_code_source` | Retrieve source code of any function, class, or module by ID |
| `find_callers` | Who calls this function? (graph traversal, not grep) |
| `find_callees` | What does this function call? |
| `get_dependencies` | What files does this file import? |
| `get_dependents` | What files import this file? |
| `get_subclasses` | What classes extend this class? |
| `get_superclasses` | What does this class inherit from? |
| `find_code_path` | Shortest path between any two symbols through the code graph |
| `get_related` | All symbols within N hops in the graph (multi-edge traversal) |
| `repo_stats` | Node, edge, and file counts for the indexed graph |
| `list_indexed_files` | All files indexed in the code graph |
| `rlm_analyze` | Deep recursive analysis via the public Python RLM orchestrator |

### Codex Built-in Tools

All standard Codex tools are available: `shell`, `apply_patch`, `read_file`, `list_dir`, `grep_files`, `view_image`, and MCP server support.

## Quick Start

```bash
# Install Python package (includes RLM dependencies automatically)
pip install .

# Build
cargo build --release

# Run (Codex CLI with code graph)
happycode

# Set your API key
export OPENAI_API_KEY=sk-...
# Or for Anthropic
export ANTHROPIC_API_KEY=sk-ant-...
```

The code graph indexes your working directory automatically in the background on session start. No separate indexing step required.
`rlms` and `litellm` are installed as package dependencies, so no separate manual install is required for `rlm_analyze`.

## Supported Languages

Python, TypeScript, JavaScript, TSX/JSX, Rust, Go, Java, C, C++ — with tree-sitter grammars for accurate AST parsing (not regex-based).

### Language-Specific Features

| Feature | Python | JS/TS | Rust | Go | Java | C/C++ |
|---------|--------|-------|------|----|------|-------|
| **Import extraction** | `import`, `from...import`, relative | `import`, `require()`, namespace | `use`, `mod` | `import` (single + grouped) | `import`, `package` | `#include` |
| **Call resolution** | Import-aware + GlobalIndex | Import-aware + GlobalIndex | Import-aware | Import-aware | Import-aware | Import-aware |
| **Inheritance** | `class Foo(Bar)` | `extends`, `implements` | `impl Trait for Type` | N/A (no inheritance) | `extends`, `implements` | `: public Base` |

## Architecture

```
├── crates/
│   └── happy-core/          # Rust library: parser, indexer, graph, search
│       ├── parser/           # Tree-sitter AST parsing, multi-language dispatch
│       │   ├── languages.rs  # 9 languages, extension mapping, grammar loading
│       │   ├── calls.rs      # Call site extraction with scope tracking
│       │   └── imports.rs    # Multi-language import extraction
│       ├── indexer/          # Parallel filesystem walker (ignore + rayon)
│       ├── graph/            # petgraph StableDiGraph with 5 edge types
│       │   ├── mod.rs        # RepositoryGraph: build, resolve, query
│       │   ├── types.rs      # GraphNode, GraphEdge, NodeKind, EdgeKind
│       │   └── queries.rs    # Callers, callees, deps, subclasses, find_path
│       ├── global_index/     # Module/symbol resolution across repo
│       │   ├── module_resolver.rs   # Import → file path resolution
│       │   └── symbol_resolver.rs   # Symbol → element ID resolution
│       ├── vector/           # BM25 keyword search + brute-force cosine similarity
│       └── store/            # Bincode serialization for cached indexes
├── core/                     # Codex core (forked from openai/codex)
│   └── src/tools/handlers/
│       └── code_graph.rs     # 13 tool handlers wired to happy-core APIs
├── cli/                      # CLI binary (happycode)
├── tui/                      # Terminal UI (ratatui, from Codex)
└── exec/                     # Sandboxed execution (from Codex)
```

**happy-core** — The indexing engine. Tree-sitter parses source files into AST nodes, the indexer extracts `CodeElement`s (functions, classes, modules) via parallel filesystem walking. The graph builder connects them via import/call/inheritance edges in a petgraph `StableDiGraph`. A `GlobalIndex` with `ModuleResolver` and `SymbolResolver` provides import-aware call resolution across files. BM25 provides keyword search. All data structures are concurrent (`DashMap`, `rayon`).

**Codex integration** — The 13 code graph tools are registered as tool handlers in Codex's `ToolRouter`. A `SharedRepoHandle` (`Arc<RwLock<Option<RepoHandle>>>`) holds the graph state, populated by a background indexing task spawned at session start. Each tool call acquires a read lock and dispatches to the appropriate happy-core query.

## Integration Tiers

Use `happy-launch` (or `HAPPY_MODE`) to choose an integration mode:

```bash
# Full in-process fork integration (default)
happy-launch --mode all-in-one

# MCP-oriented launch path
happy-launch --mode mcp

# Skills-first lightweight path
happy-launch --mode skills
```

Adapter docs and shared contracts live under `adapters/`:

- `adapters/all_in_one/`
- `adapters/mcp/`
- `adapters/skills/`
- `adapters/tool_contracts/code_graph_tools.json`

You can validate contract sync against Rust registration with:

```bash
python3 scripts/verify_code_graph_contract.py
```

### Graph Edge Types

| Edge | Meaning | Built from |
|------|---------|-----------|
| `Defines` | File defines a function/class | Element containment |
| `Calls` | Function calls another function | AST call extraction + import-aware resolution |
| `Imports` | File imports a module/symbol | Multi-language import extraction + ModuleResolver |
| `Inherits` | Class extends/implements another | Multi-language inheritance extraction |
| `References` | General reference (reserved) | — |

### Call Resolution Pipeline

Call targets are resolved with a 4-tier priority system:

1. **Same-file match** — callee defined in the caller's file
2. **SymbolResolver** — uses GlobalIndex export_map + import context for precise scoped resolution
3. **Import heuristic** — callee from a file matching an imported module name
4. **Fallback** — first candidate by name (least accurate)

## Building from Source

```bash
# Prerequisites: Rust 1.93.0+ (matches `rust-toolchain.toml`)
cargo build --release

# Run tests
cargo test

# With Python bindings (optional, requires Python 3.9+)
pip install maturin
maturin develop -m crates/happy-core/Cargo.toml --features python
```

## Acknowledgments

happycode builds on several excellent open-source projects:

- **[Codex](https://github.com/openai/codex)** by OpenAI — The foundation: production Rust terminal agent, event-driven architecture, ratatui TUI, sandboxed execution
- **[FastCode](https://github.com/Zeeshan-Hamid/FastCode)** by Zeeshan Hamid — The original Python code indexing and graph approach that inspired happy-core's architecture
- **[RLMs](https://pypi.org/project/rlms/)** — Recursive orchestration runtime used by `rlm_analyze`
- **[LiteLLM](https://github.com/BerriAI/litellm)** — Provider abstraction layer used by the RLM orchestrator
- **[opencode](https://github.com/nicholasgriffintn/opencode)** — Rust LLM client patterns, provider trait abstraction

## License

Apache-2.0 — see [LICENSE](LICENSE).

Copyright (c) 2026 Happy Technologies LLC
