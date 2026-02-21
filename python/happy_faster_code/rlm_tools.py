"""RLM integration — builds REPL namespace and system prompt for the RLM agent."""

import glob as _glob
import os
from pathlib import Path


def build_rlm_namespace(repo, repo_path: str = ".") -> dict:
    """Build a namespace dict for RLM's code_tools parameter.

    The RLM agent writes Python code executed in a REPL. Objects in this
    namespace are available as variables the LM can reference directly,
    e.g. ``callers = repo.find_callers("main")``.
    """
    abs_root = str(Path(repo_path).resolve())

    def read_file(path: str) -> str:
        """Read a file from the repository. Accepts relative or absolute paths."""
        full = path if os.path.isabs(path) else os.path.join(abs_root, path)
        try:
            with open(full, "r", encoding="utf-8", errors="replace") as f:
                return f.read()
        except OSError as e:
            return f"Error reading {path}: {e}"

    def list_files(pattern: str = "**/*") -> list[str]:
        """Glob files in the repository. Returns paths relative to repo root."""
        matches = _glob.glob(os.path.join(abs_root, pattern), recursive=True)
        results = []
        for m in sorted(matches):
            if os.path.isfile(m):
                results.append(os.path.relpath(m, abs_root))
        return results

    return {
        "repo": repo,
        "read_file": read_file,
        "list_files": list_files,
    }


def build_system_prompt(repo) -> str:
    """Build the system prompt describing available tools for the RLM agent."""
    stats = repo.stats() if hasattr(repo, "stats") else {}
    node_count = stats.get("nodes", "?")
    file_count = stats.get("files", "?")

    return f"""You are a code analysis agent with access to a structural code graph.
The repository has been indexed: {node_count} nodes across {file_count} files.

You write Python code that is executed in a REPL. The following objects are
available in your namespace:

## repo (HappyRepo)
The primary interface to the indexed code graph.

Methods:
  repo.find_callers(symbol: str) -> list[str]
      Find all functions/methods that call the given symbol. Returns element IDs.

  repo.find_callees(symbol: str) -> list[str]
      Find all functions/methods called by the given symbol. Returns element IDs.

  repo.get_dependencies(file_path: str) -> list[str]
      Get all files imported by the given file. Returns element IDs.

  repo.get_dependents(file_path: str) -> list[str]
      Get all files that import the given file. Returns element IDs.

  repo.get_subclasses(class_name: str) -> list[str]
      Find all classes that inherit from the given class. Returns element IDs.

  repo.get_superclasses(class_name: str) -> list[str]
      Find all parent classes of the given class. Returns element IDs.

  repo.find_path(source: str, target: str) -> list[str] | None
      Find shortest path between two symbols in the graph. Returns list of
      element IDs or None if no path exists.

  repo.get_related(element: str, max_hops: int) -> list[str]
      Find all elements within N hops of the given symbol. Returns element IDs.

  repo.search(query: str, k: int) -> list[tuple[str, float]]
      BM25 keyword search across all indexed elements. Returns (element_id, score).

  repo.get_source(element_id: str) -> str | None
      Get source code of a specific element by its ID.

  repo.file_tree() -> list[str]
      List all indexed file paths.

  repo.stats() -> dict
      Repository statistics: nodes, edges, files, elements, bm25_docs, has_vectors.

  repo.resolve_symbol(symbol: str) -> list[tuple[str, str]]
      Resolve a symbol name to (file_path, module_path) pairs.

  repo.resolve_module(module_name: str) -> str | None
      Resolve a module name to its file path.

## read_file(path: str) -> str
  Read a file from disk. Accepts paths relative to the repo root.

## list_files(pattern: str) -> list[str]
  Glob files in the repo. E.g. list_files("**/*.py") for all Python files.

## delegate(prompt: str) -> str
  Delegate a sub-query to a worker model. Use this for parallelizable analysis.
  The worker has the same tools available.

## rlm_query(prompt: str) -> str
  Alias for delegate(prompt), kept for backward compatibility.

## Guidelines

1. Start by understanding the structure: use repo.search() or repo.file_tree()
   to locate relevant code, then repo.get_source() to read it.
2. Follow call chains: repo.find_callers() and repo.find_callees() trace
   function relationships. Combine with repo.get_source() for full context.
3. For complex analysis, break into sub-tasks using delegate() (or rlm_query())
   independent investigations to worker models.
4. Always store intermediate results in variables for later reference.
5. Print your final answer clearly — the printed output is returned to the user.

## Example

```python
# Find who calls "authenticate" and show the source
callers = repo.find_callers("authenticate")
for caller_id in callers:
    source = repo.get_source(caller_id)
    print(f"=== {{caller_id}} ===")
    print(source)
```
"""
