# **Detailed Technical Design: HappyFasterCode**

## **1\. System Overview**

HappyFasterCode is a hybrid Rust-Python engineering agent. It optimizes the "Code-to-Intelligence" pipeline by moving structural repo-graphing into a multi-threaded Rust core while exposing a high-level Python REPL for the agent's Recursive Language Model (RLM) reasoning.

## **2\. Core Components & Data Flow**

### **2.1 Component Diagram**

graph TD  
    User\[User Query\] \--\> Orchestrator\[Python RLM Orchestrator\]  
    Orchestrator \--\> Sandbox\[Python REPL Sandbox\]  
    Sandbox \--\> Bridge\[PyO3 Bridge Layer\]  
    Bridge \--\> RustCore\[Rust Happy-Core\]  
      
    subgraph "Rust Happy-Core"  
        Scanner\[Parallel File Scanner\] \--\> TS\[Tree-sitter Parsers\]  
        TS \--\> Petgraph\[Multi-layer Structural Graph\]  
        Petgraph \--\> Search\[Graph Query Engine\]  
        Vector\[HNSW Vector Index\] \<--\> Search  
    end  
      
    Orchestrator \--\> SubAgents\[Recursive Worker Agents\]  
    SubAgents \--\> Sandbox

## **3\. Module A: Happy-Core (Rust)**

### **3.1 Structural Graph (Petgraph)**

The core uses petgraph::stable\_graph to maintain a persistent, multi-edge graph of the repository.

* **Nodes:** Entities (File, Class, Function, Interface, Variable).  
* **Edges:** Relationships (DEFINES, CALLS, IMPORTS, IMPLEMENTS, REFERENCES).  
* **Performance:** Queries like "Find all transitive callers of X" are executed as a DFS/BFS in Rust, typically completing in \< 100μs.

### **3.2 Tree-sitter Indexing**

Instead of standard regex-based indexing, we use tree-sitter for accurate semantic extraction.

* **Incremental Parsing:** Only modified files are re-parsed.  
* **Language Support:** WASM-compiled grammars for Python, TS, Rust, Go, Java, and C++.

### **3.3 Vector Search (HNSW)**

We utilize hnswlib-rs for local semantic search.

* **Embeddings:** Local storage of text-embedding-3-small vectors.  
* **Hybrid Query:** The query engine combines Graph lookups with Vector results to find "The code that handles user login" by looking for both the LoginController class and functions semantically related to "authentication."

## **4\. Module B: The Bridge (PyO3)**

The Rust core is compiled into a shared library (happy\_core.so) that can be imported as a native Python module.

// Example PyO3 Bridge Structure  
\#\[pyclass\]  
struct HappyRepo {  
    graph: Arc\<RwLock\<RepositoryGraph\>\>,  
    vector\_index: Arc\<HnswIndex\>,  
}

\#\[pymethods\]  
impl HappyRepo {  
    \#\[new\]  
    fn new(path: String) \-\> PyResult\<Self\> { ... }

    fn find\_callers(\&self, symbol: String) \-\> PyResult\<Vec\<String\>\> {  
        // High-speed Rust traversal here  
    }  
}

## **5\. Module C: Recursive Language Model (RLM)**

### **5.1 The "Librarian" Loop**

The Orchestrator (Frontier Model) does not see code directly. It sees a Repo object and operates via a REPL:

1. **Orchestrator:** Writes Python: deps \= repo.get\_dependencies("payment\_service.ts")  
2. **REPL:** Executes code, returns structured metadata (e.g., "This file depends on 4 interfaces").  
3. **Recursive Call:** If the Orchestrator identifies 10 files to check, it calls rlm\_batch(sub\_query, file\_list).

### **5.2 Parallel Delegation**

* **Fan-out:** 10 sub-agents (e.g., Gemini 1.5 Flash) are invoked via asynchronous API calls.  
* **Context Compression:** Sub-agents are instructed to return *only* the specific logic relevant to the query, stripping boilerplate.  
* **Fan-in:** The Orchestrator receives 10 high-signal summaries instead of 10,000 lines of raw code.

## **6\. Implementation Specifications**

### **6.1 Performance Targets**

| Metric | Target |
| :---- | :---- |
| **Indexing Rate** | 50,000 LoC / second |
| **Max Graph Nodes** | 1,000,000+ |
| **REPL Turn Latency** | \< 200ms (Local execution) |
| **Memory Ceiling** | 512MB (Base index for 1k files) |

### **6.2 Security & Sandboxing**

The Python REPL is executed in a **gVisor** or **E2B** sandbox to prevent arbitrary code execution on the host machine while allowing the agent to run tests and verify its own fixes.

## **7\. Development Roadmap**

### **Sprint 1: The Oxidized Core**

* Initialize Rust crate with pyo3 and tree-sitter.  
* Implement ParallelWalker using ignore and rayon.  
* Build basic Call Graph for Python.

### **Sprint 2: The RLM REPL**

* Create the Python HappyRepo wrapper.  
* Implement the Async Orchestration loop.  
* Basic "File-Search and Summarize" RLM task.

### **Sprint 3: Full Agentic Loop**

* Integrate Delta-Diff (patch) generation.  
* Implement "Test-Driven Refactoring" where the agent runs the local test suite via the Rust core.  
* Release CLI: happy-faster-code \--path ./my-project.

**Document Status:** FINAL / FOR IMPLEMENTATION

**Target Architecture:** x86\_64 / arm64 (Darwin/Linux)