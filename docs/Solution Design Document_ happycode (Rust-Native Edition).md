# Solution Design Document: happycode (Rust-Native Edition)

## 1. Executive Summary

happycode is a fork of OpenAI Codex CLI that adds a Rust-native code graph engine (`happy-core`) and exposes graph-native tools directly to the model runtime.

As implemented today:

- The core graph/indexing pipeline is in Rust (`tree-sitter` + `petgraph` + `rayon` + `dashmap`).
- Codex sessions auto-start background indexing and incremental updates for the current working directory.
- 13 public code graph tools are registered, including `rlm_analyze`.
- Optional PyO3 bindings expose `HappyRepo` for Python orchestration.

This document describes the shipped architecture, not aspirational design.

## 2. Core Objectives

- Structural navigation over text-only search for code relationships.
- Fast startup and low query latency through in-process indexed state.
- Public recursive analysis path via Python RLM orchestration (`rlm_analyze`).
- Keep implementation grounded in Codex runtime primitives (tool router, session services, sandbox model).

## 3. Implemented Architecture

### 3.1 Rust Indexing and Graph Core (`happy-core`)

- Multi-language parsing: Python, JavaScript, TypeScript, TSX/JSX, Rust, Go, Java, C, C++.
- Index extraction via parallel filesystem walk (`ignore` + `rayon`).
- Graph storage via `petgraph::StableDiGraph` with node/edge lookup side indexes (`DashMap`).
- Edge construction includes:
  - `Defines`
  - `Calls`
  - `Imports`
  - `Inherits`
  - `References`
  - `Implements`
- Call resolution uses layered strategy:
  1. same-file match
  2. symbol resolver with import context
  3. import/path heuristic
  4. fallback by name

### 3.2 Codex Runtime Integration (Fork Layer)

- Session service owns `SharedRepoHandle = Arc<RwLock<Option<RepoHandle>>>`.
- Startup path spawns background indexing for session CWD.
- A file watcher incrementally updates graph + BM25 for changed files.
- Tool registration occurs through Codex `ToolRouter`/`build_specs`.
- Code graph tools are always registered; they return a friendly message until indexing is ready.

### 3.3 Public Tool Surface

The following 13 code graph tools are public:

- `find_callers`
- `find_callees`
- `get_dependencies`
- `get_dependents`
- `get_subclasses`
- `get_superclasses`
- `find_code_path`
- `get_related`
- `search_code`
- `get_code_source`
- `repo_stats`
- `list_indexed_files`
- `rlm_analyze`

### 3.4 Python RLM Orchestration

- `rlm_analyze` launches `python3 -m happy_code.orchestrator`.
- Orchestrator builds `HappyRepo` and RLM tool namespace.
- Delegation is exposed under both `delegate(...)` and backward-compatible alias `rlm_query(...)`.
- Model/provider selection is configuration and environment driven (`litellm`), not hardcoded to specific vendors.

### 3.5 Integration Tier Isolation

The repository now isolates integration surfaces into explicit tiers:

- `all-in-one`: in-process full fork integration (default).
- `mcp`: MCP-oriented adapter path.
- `skills`: lightweight skills/scripts path.

Mode selection is available through:

- `happy-launch --mode all-in-one|mcp|skills`
- `HAPPY_MODE=all-in-one|mcp|skills`

Shared adapter contract location:

- `adapters/tool_contracts/code_graph_tools.json`

## 4. Technical Stack (Current)

| Component | Technology |
|---|---|
| Structural indexing/graph | Rust, tree-sitter, petgraph |
| Parallelism | rayon + ignore |
| Concurrent state | dashmap |
| Keyword retrieval | Custom BM25 (`happy-core`) |
| Vector retrieval | Brute-force cosine index (optional path in PyO3) |
| Python bridge | PyO3 (feature-gated) |
| Agent runtime | Codex CLI fork (tokio/ratatui/reqwest) |
| RLM orchestration | Python package + `rlm` + `litellm` |

## 5. Performance and Memory Notes

- The repository currently does not ship formal benchmark artifacts proving specific numeric claims (for example, fixed microsecond latency or exact memory reduction percentages).
- Performance is expected to scale better than grep-first approaches due to indexed graph traversal, but numbers should be treated as targets until benchmarked in-repo.

## 6. Security and Execution Model

- `rlm_analyze` currently executes Python as a local subprocess.
- It does not currently run inside gVisor/E2B by default.
- Command execution sandboxing behavior for shell/tool calls remains inherited from Codex runtime controls.

## 7. Near-Term Roadmap

- Add benchmark harnesses and publish reproducible latency/memory metrics.
- Expand direct integration tests for code graph tool handlers in `core`.
- Keep Python orchestration docs and runtime aliases in sync as APIs evolve.

## 8. Document Metadata

- Status: Implemented architecture baseline
- Target platforms: x86_64 and arm64 (Darwin/Linux)
