# **Solution Design Document: HappyFasterCode (Rust-Native Edition)**

## **1\. Executive Summary**

**HappyFasterCode** is an extreme-performance AI software engineering agent. Unlike its predecessor, HKUDS/FastCode, it utilizes a **Rust-native indexing core** and a **Recursive Language Model (RLM)** architecture. By "oxidizing" the structural analysis layer from the start, HappyFasterCode achieves sub-millisecond repository navigation, enabling the LLM to treat multi-million line codebases as a local, queryable variable.

## **2\. Core Objectives**

* **Zero-Latency Navigation:** Sub-millisecond structural lookups (callers, dependencies, hierarchy) via Rust's Petgraph.  
* **Recursive Precision:** Leveraging the RLM "Librarian Pattern" to decompose complex repo tasks into parallel sub-agent calls.  
* **Memory Efficiency:** 80% reduction in footprint compared to Python-based indexing, allowing full-repo indexing on consumer hardware.

## **3\. Architectural Design**

### **3.1 The Rust-Native Engine (The "Core")**

We skip the Python/NetworkX implementation and build the indexer in Rust:

* **Tree-sitter Parser:** High-speed, incremental AST parsing for all major languages (Python, TS, Rust, Go, C++).  
* **Structural Graph (Petgraph):** A memory-dense, multi-layered graph representing imports, call chains, and inheritance.  
* **The Bridge (PyO3):** The Rust core is exposed as a high-performance Python module (happy\_core).

### **3.2 The RLM Orchestration Layer**

The agent interacts with the repository through a Python REPL. Instead of the LLM receiving a massive text context, it receives a Repo object.

1. **Orchestrator:** Writes a Python script: results \= repo.find\_callers("process\_payment").  
2. **Execution:** The script executes at C-speed in the Rust core.  
3. **Delegation:** If the results are too large, the agent spawns recursive child-agents to "skim" specific sub-graphs in parallel.

## **4\. Technical Stack**

| Component | Technology |
| :---- | :---- |
| **Indexing Engine** | **Rust** (Tree-sitter \+ Petgraph) |
| **Interface Bridge** | **PyO3** (Rust to Python bindings) |
| **RLM Framework** | Custom Async Python REPL |
| **Parallelism** | **Rayon** (Rust) & **Asyncio** (Python) |
| **Frontier Model** | Gemini 2.0 Flash / GPT-4o (Orchestration) |
| **Worker Models** | Qwen-2.5-Coder-7B / Gemini 1.5 Flash (Recursive Execution) |
| **Vector Engine** | **Hnswlib** (Rust implementation) |

## **5\. Key Modules**

### **Module A: happy-core (Rust)**

The "Source of Truth" for the repository.

* **Scanner:** Walks the filesystem, triggers Tree-sitter for each file.  
* **Indexer:** Builds a directed graph of the entire workspace.  
* **Query Engine:** Implements DFS/BFS and semantic similarity searches natively in Rust.

### **Module B: The Recursive Librarian (Python)**

The logic layer where the RLM lives.

* **Context Sandbox:** A controlled environment where the LLM can run Python scripts to query happy-core.  
* **Fan-out Manager:** Manages the lifecycle of recursive sub-agents (e.g., spawning 10 workers to analyze 10 different modules).

### **Module C: The Delta-Diff Engine**

A high-speed patch generator that applies changes validated by the Rust core (ensuring no syntax errors are introduced during the refactor).

## **6\. Implementation Roadmap**

### **Phase 1: The Oxidized Foundation (Weeks 1-3)**

* **Setup:** Initialize Rust workspace and integrate tree-sitter.  
* **Indexing:** Build the initial Petgraph structure for Python and TypeScript.  
* **Bindings:** Create the PyO3 bridge to expose the graph to Python.

### **Phase 2: RLM Integration (Weeks 4-6)**

* **REPL Development:** Build the sandboxed Python environment for the agent.  
* **Recursive Logic:** Implement the rlm\_call function that allows the agent to delegate sub-tasks.  
* **MCP Support:** Expose the Rust engine as a Model Context Protocol server for integration with Cursor/Claude Code.

### **Phase 3: Scaling & Refinement (Weeks 7-9)**

* **Multi-language Support:** Add Java, C++, and Go parsers.  
* **Incremental Indexing:** Implement file-watchers to update the Rust graph in real-time as the user types.

## **7\. Performance Benchmarks (Target)**

* **Index Time (100k LoC):** \< 1.5 seconds.  
* **Query Latency:** \< 500 microseconds.  
* **Peak Memory:** \< 200MB for medium-sized projects.

**Prepared for:** Antigravity Engineering Team

**Project Lead:** Gemini/User

**Version:** 2.0.0 (Rust-First)