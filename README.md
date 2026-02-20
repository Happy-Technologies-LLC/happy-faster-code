<p align="center">
  <img src="HappyFasterCode-text.png" alt="HappyFasterCode" width="600">
</p>

# HappyFasterCode

**The AI coding agent that actually understands your codebase.**

Every AI coding CLI today — Claude Code, Gemini CLI, Codex — works the same way: read files, grep for text, hope the LLM figures out the structure. They treat your codebase as a bag of text files.

HappyFasterCode is different. Before the LLM sees a single token, it builds a **full structural graph** of your entire codebase: every function call, every import chain, every class hierarchy, every dependency edge. Then it gives the LLM **18 tools** — 13 graph-aware navigation tools plus full read/write/execute capabilities — making it a complete coding agent that truly understands your code's structure.

The result: the LLM doesn't guess at relationships. It **knows** them. And it can act on that knowledge — editing files, running builds, executing tests, and committing changes.

## Why This Exists

AI coding tools are bottlenecked by context, not intelligence. When you ask "what calls this function?", existing tools grep the codebase, stuff matches into the context window, and hope for the best. This fails on large codebases, indirect calls, cross-file relationships, and anything that requires understanding structure rather than matching text.

HappyFasterCode solves this with a Rust-native indexing engine that builds a directed graph of your code in seconds, then exposes that graph to the LLM as first-class tools. The LLM can traverse call chains, walk dependency trees, and follow inheritance hierarchies — all with sub-millisecond query latency.

## What Makes It Different

| | HappyFasterCode | Typical AI CLI |
|---|---|---|
| **Code understanding** | Structural graph (AST-parsed, edge-connected) | Text search (grep/ripgrep) |
| **"Who calls X?"** | Exact answer via graph traversal | Best-effort regex match |
| **Cross-file dependencies** | Full import/dependency graph | File-by-file reading |
| **Class hierarchies** | Inheritance edges, subclass/superclass queries | String matching on `extends`/`implements` |
| **Path between symbols** | Shortest path through call/import/inheritance graph | Not possible |
| **Indexing speed** | Parallel Rust (tree-sitter + rayon), seconds for 100k LoC | N/A or Python-based |
| **Query latency** | Sub-millisecond graph lookups | Re-reads files each time |
| **LLM provider** | Anthropic, OpenAI, or any OpenAI-compatible endpoint | Usually locked to one |

## The 18 Agent Tools

Every other AI CLI gives the LLM `read_file` and `grep`. HappyFasterCode gives it a full code graph **plus** write and execute capabilities:

### Code Graph Navigation (unique to HappyFasterCode)

| Tool | What it does |
|------|-------------|
| `search_code` | BM25 keyword search across all indexed code elements |
| `get_source` | Retrieve source code of any function, class, or module by ID |
| `find_callers` | Who calls this function? (graph traversal, not grep) |
| `find_callees` | What does this function call? |
| `get_dependencies` | What files does this file import? |
| `get_dependents` | What files import this file? |
| `get_subclasses` | What classes extend this class? |
| `get_superclasses` | What does this class inherit from? |
| `find_path` | Shortest path between any two symbols through the code graph |
| `get_related` | All symbols within N hops in the graph (multi-edge traversal) |
| `repo_stats` | Node, edge, and file counts for the indexed graph |

### Read & Search

| Tool | What it does |
|------|-------------|
| `read_file` | Read file contents with line numbers, offset, and limit |
| `list_files` | All indexed source files in the repository |
| `list_directory` | Browse directory contents |
| `grep_files` | Regex search across file contents (uses ripgrep when available) |

### Write & Execute

| Tool | What it does |
|------|-------------|
| `write_file` | Create or overwrite files (auto-creates parent directories) |
| `edit_file` | Precise string replacement — surgical edits without rewriting entire files |
| `bash` | Execute any shell command: builds, tests, git, linters, package managers |

## Quick Start

```bash
# Build
cargo build --release

# Run (defaults to interactive chat mode)
happycode /path/to/your/repo

# First run will prompt for provider and API key
```

### Environment Variables

```bash
# Anthropic (default)
export ANTHROPIC_API_KEY=sk-ant-...

# OpenAI
export OPENAI_API_KEY=sk-...

# Override provider/model
export HAPPY_PROVIDER=openai    # or "anthropic"
export HAPPY_MODEL=gpt-4o       # any model name
export HAPPY_MAX_TOKENS=4096
export HAPPY_TEMPERATURE=0.0
```

### CLI Commands

```bash
happycode                              # Interactive chat (default)
happycode /path/to/repo                # Chat with specific repo
happycode index /path/to/repo          # Index only (no chat)
happycode query /path/to/repo symbol   # Query code graph
happycode search /path/to/repo "term"  # BM25 keyword search
happycode stats /path/to/repo          # Show graph statistics
happycode watch /path/to/repo          # Watch and re-index on changes
```

### TUI Key Bindings

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus between panels |
| `Enter` | Submit input |
| `Up/Down` | Scroll chat or navigate file tree |
| `Ctrl-C` | Cancel current agent turn |
| `Ctrl-D` | Quit |

### Chat Commands

| Command | Action |
|---------|--------|
| `/help` | Show all available commands |
| `/clear` | Clear conversation and reset context |
| `/compact` | Summarize conversation to save context window |
| `/model <name>` | Switch to a different model |
| `/stats` | Show repo and session statistics |
| `/files` | List all indexed files |
| `/quit` | Exit HappyFasterCode |

### @ File References

Include file contents directly in your messages:

```
@src/main.rs what does this file do?
explain the relationship between @src/parser/mod.rs and @src/indexer/walker.rs
```

Files are resolved relative to the repo root, truncated at 8KB to protect context.

### Custom Commands

Create reusable prompts as `.happy/commands/<name>.md` files:

```bash
# .happy/commands/review.md
Review the following code for bugs, security issues, and performance problems.
Focus on: $ARGUMENTS
```

Then use it: `/review error handling in the parser module`

## Supported Languages

Python, TypeScript, JavaScript, Rust, Go, Java, C, C++, C#  — with tree-sitter grammars for accurate AST parsing (not regex-based).

## Architecture

```
crates/
├── happy-core/     # Library: parser, indexer, graph, search, PyO3 bindings
└── happy-cli/      # Binary: TUI, agent loop, LLM providers, tools
```

**happy-core** — The indexing engine. Tree-sitter parses source files into AST nodes, the indexer extracts `CodeElement`s (functions, classes, modules), and the graph builder connects them via import/call/inheritance edges in a petgraph `StableDiGraph`. BM25 provides keyword search. All data structures are concurrent (`DashMap`, `rayon`).

**happy-cli** — The interactive agent. A channel-based architecture connects the ratatui TUI to a background agent loop. The agent streams LLM responses via reqwest SSE, executes tool calls against happy-core's graph/search APIs, and feeds results back to the LLM in an agentic loop (up to 20 iterations per query).

## Building from Source

```bash
# Prerequisites: Rust 1.75+
cargo build --release

# Run tests
cargo test

# With Python bindings (requires Python 3.9+)
pip install maturin
maturin develop --features python
```

## Configuration

On first run without an API key, HappyFasterCode will interactively prompt for:
1. Provider (Anthropic, OpenAI, or OpenAI-compatible)
2. API base URL (for self-hosted/proxy endpoints like LiteLLM, Ollama, vLLM)
3. API key

Configuration is saved to `.happy/agent.toml` in your repository. Add `.happy/` to your `.gitignore`.

## Acknowledgments

HappyFasterCode draws inspiration from several excellent open-source projects:

- **[FastCode](https://github.com/Zeeshan-Hamid/FastCode)** by Zeeshan Hamid — The original Python code indexing and graph approach that inspired happy-core's architecture
- **[Codex](https://github.com/openai/codex)** by OpenAI — Production Rust terminal agent patterns, event-driven architecture, and ratatui TUI design
- **[opencode](https://github.com/nicholasgriffintn/opencode)** — Rust LLM client patterns, provider trait abstraction, and streaming architecture
- **[RLM](https://github.com/ruv/rlm)** by ruv — Recursive Language Model framework for agentic reasoning patterns

## License

MIT — see [LICENSE](LICENSE).

Copyright (c) 2026 Happy Technologies LLC
