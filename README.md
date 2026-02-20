# HappyFasterCode

A high-performance, Rust-native AI coding agent with an interactive terminal UI. Index any codebase, build a navigable code graph, and chat with an LLM that has deep structural understanding of your code — all from your terminal.

## Features

- **Rust-native code indexing** — Tree-sitter parsing for Python, TypeScript, JavaScript, Rust, Go, Java, C/C++, C#
- **Code graph** — Petgraph-backed directed graph with callers, callees, dependencies, inheritance, and cross-file relationships
- **BM25 keyword search** — Fast full-text search across all indexed code elements
- **Interactive TUI** — Multi-panel ratatui terminal interface with file tree, chat, and code preview
- **Streaming LLM integration** — Anthropic (Claude) and OpenAI (GPT) with streaming responses and tool use
- **OpenAI-compatible endpoints** — Works with LiteLLM, Ollama, vLLM, and other OpenAI-compatible APIs
- **Parallel indexing** — Rayon-powered parallel file walking and parsing
- **Incremental updates** — File watcher for live re-indexing on changes
- **PyO3 bridge** — Optional Python bindings for integration with Python toolchains

## Quick Start

```bash
# Build
cargo build --release

# Run (defaults to interactive chat mode)
happycode /path/to/your/repo

# Or use the short name
happy chat /path/to/your/repo

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
happycode                          # Interactive chat (default)
happycode /path/to/repo            # Chat with specific repo
happy index /path/to/repo          # Index only (no chat)
happy query /path/to/repo symbol   # Query code graph
happy search /path/to/repo "term"  # BM25 keyword search
happy stats /path/to/repo          # Show graph statistics
happy watch /path/to/repo          # Watch and re-index on changes
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
| `/clear` | Clear conversation history |
| `/model <name>` | Switch model |

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
2. API base URL (for self-hosted/proxy endpoints)
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

Copyright (c) 2025 Happy Technologies LLC
