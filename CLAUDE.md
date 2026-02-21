# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

HappyFasterCode is a code-graph-aware AI coding agent built on [OpenAI's Codex CLI](https://github.com/openai/codex). It extends the Codex agent with a Rust-native structural code graph engine (`happy-core`) that indexes the entire codebase into a petgraph directed graph, then exposes 12 graph navigation tools to the LLM alongside Codex's standard read/write/execute tools.

## Architecture

Two main components:

- **happy-core** (`crates/happy-core/`) — Rust library crate. Tree-sitter parses source files into ASTs across 9 languages (Python, JS, TS, TSX, Rust, Go, Java, C, C++). The indexer extracts `CodeElement`s via parallel filesystem walking (`ignore` + `rayon`). The graph builder connects elements via edges (Defines, Calls, Imports, Inherits, References) in a `petgraph::StableDiGraph`. A `GlobalIndex` with `ModuleResolver` and `SymbolResolver` provides import-aware call resolution. BM25 provides keyword search. Optional PyO3 bindings expose a `HappyRepo` Python class.

- **Codex fork** (`core/`, `cli/`, `tui/`, `exec/`, etc.) — Forked from OpenAI's Codex CLI. The 12 code graph tools are registered as handlers in `core/src/tools/handlers/code_graph.rs` and wired into Codex's `ToolRouter`. A `SharedRepoHandle` (`Arc<RwLock<Option<RepoHandle>>>`) persists at the `SessionServices` level, populated by a background indexing task spawned at session start.

## Key Files

| File | Purpose |
|------|---------|
| `crates/happy-core/src/graph/mod.rs` | `RepositoryGraph`: graph building, import/call/inheritance edge construction, 4-tier call resolution |
| `crates/happy-core/src/parser/imports.rs` | Multi-language import extraction (9 languages) |
| `crates/happy-core/src/parser/calls.rs` | Call site extraction with scope tracking |
| `crates/happy-core/src/global_index/` | `GlobalIndex`, `ModuleResolver`, `SymbolResolver` for cross-file resolution |
| `crates/happy-core/src/indexer/walker.rs` | Parallel filesystem walker, element extraction per language |
| `core/src/tools/handlers/code_graph.rs` | 12 tool handlers + `start_code_graph_indexing()` + `SharedRepoHandle` |
| `core/src/tools/spec.rs` | Tool registration, `build_specs()` with `code_graph_repo` parameter |
| `core/src/state/service.rs` | `SessionServices` holding the `SharedRepoHandle` |

## Build Commands

```bash
cargo build --release          # Build everything
cargo test                     # Run all tests (53 happy-core + Codex tests)
cargo test -p happy-core       # Run happy-core tests only
cargo check --workspace        # Type-check without building
```

## Tech Stack

| Component | Technology |
|---|---|
| Indexing engine | Rust (tree-sitter + petgraph) |
| Parallel indexing | `rayon` + `ignore` crate |
| Concurrent maps | `dashmap` |
| Keyword search | BM25 (custom Rust impl) |
| Vector search | Brute-force cosine similarity (`vector/cosine.rs`) |
| Agent runtime | Codex CLI (tokio + ratatui + reqwest) |
| Sandboxing | Codex's exec system (macOS Seatbelt / Linux landlock) |
| Python bindings | PyO3 (optional, behind `python` feature flag) |
| Serialization | bincode (graph cache), serde_json (API payloads) |

## Target Platforms

x86_64 and arm64 (Darwin and Linux).
