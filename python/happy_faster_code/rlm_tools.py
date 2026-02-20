"""RLM integration - wraps HappyRepo methods as custom_tools for the RLM agent."""

from typing import Dict, Tuple, Callable, Any


def build_rlm_tools(repo) -> Dict[str, Tuple[Any, str]]:
    """Build a custom_tools dict for RLM from a HappyRepo instance.

    Returns a dict where each key is a tool name and each value is
    (callable, description) as expected by RLM's custom_tools interface.
    """
    return {
        "repo": (repo, "HappyRepo object for querying the codebase"),
        "find_callers": (repo.find_callers, "Find all callers of a symbol. Args: symbol (str)"),
        "find_callees": (repo.find_callees, "Find all callees of a symbol. Args: symbol (str)"),
        "get_deps": (repo.get_dependencies, "Get file dependencies. Args: file_path (str)"),
        "get_dependents": (repo.get_dependents, "Get files that depend on a file. Args: file_path (str)"),
        "get_subclasses": (repo.get_subclasses, "Get subclasses of a class. Args: class_name (str)"),
        "get_superclasses": (repo.get_superclasses, "Get superclasses of a class. Args: class_name (str)"),
        "find_path": (repo.find_path, "Find path between two symbols. Args: source (str), target (str)"),
        "get_related": (repo.get_related, "Get related elements within N hops. Args: element (str), max_hops (int)"),
        "search": (repo.search, "BM25 keyword search. Args: query (str), k (int)"),
        "get_source": (repo.get_source, "Get source code of an element. Args: element_id (str)"),
        "file_tree": (repo.file_tree, "Get repository file tree. No args."),
        "stats": (repo.stats, "Get repository statistics. No args."),
    }
