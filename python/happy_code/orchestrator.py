"""RLM orchestrator — runs the Recursive Language Model with HappyRepo tools."""

import argparse
import json
import sys


def run(
    path: str,
    query: str,
    model: str | None = None,
    max_depth: int = 3,
    verbose: bool = True,
    elements_file: str | None = None,
    graph_rpc_endpoint: str | None = None,
    graph_rpc_token: str | None = None,
    volt_conversation_id: str | None = None,
) -> str:
    """Run the RLM agent with HappyRepo tools against a repository.

    Args:
        path: Path to the repository to analyze.
        query: The analysis query to run.
        model: LiteLLM model string override. If None, reads from config.
        max_depth: Maximum recursion depth for sub-queries.
        verbose: Whether to print intermediate steps.
        elements_file: Optional path to serialized CodeElement snapshot
            exported by the Rust runtime to avoid re-indexing from disk.
        graph_rpc_endpoint: Optional host:port for local graph RPC.
        graph_rpc_token: Auth token for local graph RPC.
        volt_conversation_id: Optional conversation/thread id override for Volt lookup scope.

    Returns:
        The agent's final response string.
    """
    from rlm import RLM

    from . import HappyRepo
    from .config import load_config
    from .rlm_tools import build_rlm_namespace, build_system_prompt
    from .volt_memory import build_volt_memory_hooks
    from .worker import build_delegate

    # Load config (TOML + env vars), allow model override
    config = load_config(path)
    if volt_conversation_id:
        config["volt_conversation_id"] = volt_conversation_id
    litellm_model = model if model is not None else config["litellm_model"]

    # Set API key in environment for LiteLLM
    if config["api_key"]:
        provider = config["provider"]
        if provider == "anthropic":
            import os

            os.environ.setdefault("ANTHROPIC_API_KEY", config["api_key"])
        elif provider == "openai":
            import os

            os.environ.setdefault("OPENAI_API_KEY", config["api_key"])

    # Build repo and tools.
    # Priority:
    #   1. Live graph RPC bridge (no reconstruction)
    #   2. Serialized elements snapshot (fast reconstruction)
    #   3. Filesystem re-index from path (fallback)
    if graph_rpc_endpoint and graph_rpc_token:
        from .graph_rpc import GraphRpcRepo

        try:
            repo = GraphRpcRepo(
                graph_rpc_endpoint,
                graph_rpc_token,
                path=path,
            )
            # Verify RPC reachability once up front so we can fallback cleanly.
            repo.stats()
        except Exception:
            if elements_file:
                repo = HappyRepo.from_elements_file(elements_file, path)
            else:
                repo = HappyRepo(path)
    elif elements_file:
        repo = HappyRepo.from_elements_file(elements_file, path)
    else:
        repo = HappyRepo(path)
    memory_context, recall_memory = build_volt_memory_hooks(config, query)
    namespace = build_rlm_namespace(repo, path, recall_memory=recall_memory)
    system_prompt = build_system_prompt(repo, memory_context=memory_context)

    # Add delegate function for recursive sub-queries
    worker_model = config.get("worker_model") or litellm_model
    delegate_fn = build_delegate(
        repo,
        path,
        worker_model,
        recall_memory=recall_memory,
        memory_context=memory_context,
    )
    namespace["delegate"] = delegate_fn
    # Backward-compatible alias used in existing prompts/docs.
    namespace["rlm_query"] = delegate_fn

    # Create and run the RLM agent
    agent = RLM(
        backend="litellm",
        backend_kwargs={"model_name": litellm_model},
    )

    result = agent.completion(
        prompt=query,
        system_prompt=system_prompt,
        code_tools=namespace,
        max_depth=max_depth,
        verbose=verbose,
    )

    return result.response


def main():
    """CLI entry point for happy-rlm."""
    parser = argparse.ArgumentParser(
        description="happycode RLM Orchestrator — recursive code analysis"
    )
    parser.add_argument("--path", required=True, help="Path to the repository")
    parser.add_argument("--query", required=True, help="Analysis query to run")
    parser.add_argument(
        "--model", default=None, help="LiteLLM model override (e.g. gpt-4o, anthropic/claude-sonnet-4-6)"
    )
    parser.add_argument(
        "--max-depth", type=int, default=3, help="Max recursion depth (default: 3)"
    )
    parser.add_argument("--quiet", action="store_true", help="Disable verbose output")
    parser.add_argument(
        "--json", action="store_true", help="Output structured JSON"
    )
    parser.add_argument(
        "--elements-file",
        default=None,
        help="Optional path to serialized CodeElement snapshot from Rust runtime.",
    )
    parser.add_argument(
        "--graph-rpc-endpoint",
        default=None,
        help="Optional host:port for live graph RPC bridge from Rust runtime.",
    )
    parser.add_argument(
        "--graph-rpc-token",
        default=None,
        help="Auth token for --graph-rpc-endpoint.",
    )
    parser.add_argument(
        "--volt-conversation-id",
        default=None,
        help="Optional conversation/thread id override for Volt memory lookup scope.",
    )

    args = parser.parse_args()

    response = run(
        path=args.path,
        query=args.query,
        model=args.model,
        max_depth=args.max_depth,
        verbose=not args.quiet,
        elements_file=args.elements_file,
        graph_rpc_endpoint=args.graph_rpc_endpoint,
        graph_rpc_token=args.graph_rpc_token,
        volt_conversation_id=args.volt_conversation_id,
    )

    if args.json:
        print(json.dumps({"query": args.query, "response": response}))
    else:
        print(response)


if __name__ == "__main__":
    main()
