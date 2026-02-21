# Detailed Technical Design: happycode

## 1. System Overview

happycode combines:

- A Rust-native structural code graph engine (`crates/happy-core`)
- A Codex CLI runtime fork (`core`, `cli`, `tui`, `exec`, ...)
- A Python orchestration package for recursive analysis (`python/happy_faster_code`)
- Adapter tier scaffolding (`adapters/`) for `all-in-one`, `mcp`, and `skills`

The design goal is to make repository structure queryable by tools, instead of repeatedly scanning raw files per question.

## 2. Runtime Data Flow (Implemented)

```mermaid
flowchart TD
    U[User Query] --> C[Codex Session]
    C --> TR[Tool Router]

    TR -->|code graph tools| CG[CodeGraphDispatcher]
    CG --> SRH[SharedRepoHandle Arc/RwLock]
    SRH --> RG[RepositoryGraph + BM25]

    C -->|startup| IDX[start_code_graph_indexing]
    IDX --> WAI[walk_and_index]
    WAI --> BLD[build_from_elements]
    BLD --> RG

    C -->|background| FW[file watcher]
    FW --> UPD[index_single_file/update_file]
    UPD --> RG

    TR -->|rlm_analyze| PY[python3 -m happy_faster_code.orchestrator]
    PY --> HR[HappyRepo (PyO3)]
    HR --> RG2[Fresh graph in Python process]
```

Notes:

- The in-session shared graph (`SharedRepoHandle`) is used by graph tools.
- `rlm_analyze` currently builds/uses a Python-side `HappyRepo` in a subprocess path.

## 3. Module A: Rust Core (`happy-core`)

### 3.1 Parsing and Element Extraction

- Parser dispatches by extension to tree-sitter grammars.
- Indexer walks repo with gitignore-aware traversal.
- Extracted elements include file/module/class/function/method/etc.

### 3.2 Graph Model

Node kinds include file/module/class/function/method/variable/interface/struct/enum.

Edge kinds include:

- `Defines`
- `Calls`
- `Imports`
- `Inherits`
- `References`
- `Implements`

### 3.3 Relationship Construction

Build sequence:

1. Add nodes and element arena entries.
2. Build global index (module map + symbol map).
3. Add `Defines` edges.
4. Add semantic edges: imports, calls, inheritance.

Call target resolution order:

1. same file
2. symbol resolver (import-aware)
3. import/path heuristic
4. first-name fallback

### 3.4 Query Layer

Core graph queries include:

- callers/callees
- dependencies/dependents
- subclasses/superclasses
- shortest path
- N-hop related nodes
- file/source lookups and stats

## 4. Module B: Codex Tool Integration

### 4.1 Registered Public Code Graph Tools

13 tools:

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

### 4.2 Startup and Incremental Updates

- Session creation starts background indexing for CWD.
- Repo handle remains `None` until index is ready.
- File watcher batches create/modify/remove and applies incremental updates:
  - remove stale BM25 docs
  - update graph nodes/edges for changed files
  - reinsert BM25 docs

### 4.3 Failure Behavior

- If graph is not ready, tools return a retry message.
- If `rlm_analyze` Python invocation fails, error explains package/install expectation.

## 5. Module C: Adapter Isolation Layer

### 5.1 Mode Selection

Modes are resolved through:

1. explicit CLI flag (`happy-launch --mode ...`)
2. config/env (`HAPPY_MODE` / `.happy/agent.toml`)
3. default (`all-in-one`)

Supported modes:

- `all-in-one`
- `mcp`
- `skills`

### 5.2 Adapter Paths

- `adapters/all_in_one/`: full in-process integration guidance
- `adapters/mcp/`: MCP-oriented adapter guidance and future service boundary
- `adapters/skills/`: skills/scripts adapter guidance
- `adapters/tool_contracts/code_graph_tools.json`: shared tool names/params contract

### 5.3 Contract Validation

Use the repo script to detect drift between shared contract and Rust registration:

```bash
python3 scripts/verify_code_graph_contract.py
```

## 6. Module D: Python RLM Orchestration

### 6.1 Orchestrator Responsibilities

- Load config (`.happy/agent.toml` + env overrides)
- Create `HappyRepo(path)`
- Build namespace with:
  - `repo`
  - `read_file`
  - `list_files`
  - `delegate` (worker call)
  - `rlm_query` alias (backward compatibility)
- Run `RLM(...).completion(...)`

### 6.2 Worker Delegation

- Worker delegation currently occurs through repeated `delegate(prompt)` calls.
- Workers are configured by `worker_model` (or fallback to primary model).
- This is model/provider-configurable via `litellm`, not hardcoded to single vendors.

## 7. Retrieval and Search Design

- Primary search in tool surface: BM25 over indexed element text.
- Vector search exists in Python bindings as optional brute-force cosine index.
- No HNSW ANN backend is currently used in this repository.

## 8. Security and Sandboxing

- `rlm_analyze` currently runs as local `python3` subprocess.
- gVisor/E2B sandbox integration is not currently part of this path.
- Broader command sandboxing remains governed by Codex runtime policies.

## 9. Testing and Validation

Current validated paths include:

- `happy-core` unit tests for parsing/indexing/graph/query behavior.
- Python tests for RLM helpers/orchestrator wiring.
- Python launcher tests for mode and command mapping.
- Python `HappyRepo` integration tests use an in-test synthetic fixture repo.

## 10. Known Gaps / Next Steps

- Add direct `core`-level tests for `code_graph` tool handler behavior.
- Add benchmark harnesses for reproducible latency/memory metrics.
- Continue tightening docs and comments when tool surface changes.

## 11. Document Metadata

- Status: Current implementation reference
- Target architecture: x86_64 / arm64 (Darwin/Linux)
