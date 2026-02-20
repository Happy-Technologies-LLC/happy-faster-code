# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

HappyFasterCode is an extreme-performance AI software engineering agent. It uses a **Rust-native indexing core** with a **Python RLM (Recursive Language Model)** orchestration layer. The Rust core provides sub-millisecond structural lookups over multi-million line codebases, exposed to Python via PyO3 bindings.

## Architecture

Three main modules:

- **happy-core (Rust)** — The indexing engine. Uses Tree-sitter for incremental AST parsing and Petgraph (`petgraph::stable_graph`) for a multi-layer structural graph (nodes: File/Class/Function/Interface/Variable; edges: DEFINES/CALLS/IMPORTS/IMPLEMENTS/REFERENCES). Includes HNSW vector search via `hnswlib-rs`.
- **PyO3 Bridge** — Compiles Rust core into `happy_core.so` Python module exposing `HappyRepo` class with methods like `find_callers()`, `get_dependencies()`, etc. Uses `Arc<RwLock<RepositoryGraph>>` for thread-safe graph access.
- **RLM Orchestrator (Python)** — Async Python REPL where a frontier model (Gemini 2.0 Flash / GPT-4o) queries the repo object. Supports recursive fan-out to worker models (Qwen-2.5-Coder-7B / Gemini 1.5 Flash) for parallel sub-graph analysis.

## Tech Stack

| Component | Technology |
|---|---|
| Indexing Engine | Rust (Tree-sitter + Petgraph) |
| Interface Bridge | PyO3 |
| Parallelism | Rayon (Rust) & Asyncio (Python) |
| File Walking | `ignore` crate + Rayon |
| Vector Engine | hnswlib-rs |
| Sandbox | gVisor or E2B |

## Performance Targets

- Indexing: 50,000 LoC/sec, <1.5s for 100k LoC
- Query latency: <500μs (graph), <200ms (REPL turn)
- Memory: <200MB medium projects, 512MB ceiling for 1k files
- Graph capacity: 1,000,000+ nodes

## Target Platforms

x86_64 and arm64 (Darwin and Linux).

## Development Phases

1. **Oxidized Core** — Rust crate with pyo3 + tree-sitter, ParallelWalker (ignore + rayon), basic call graph for Python/TypeScript
2. **RLM REPL** — Python HappyRepo wrapper, async orchestration loop, MCP server for Cursor/Claude Code integration
3. **Full Agentic Loop** — Delta-Diff patch generation, test-driven refactoring via Rust core, CLI: `happy-faster-code --path ./my-project`
