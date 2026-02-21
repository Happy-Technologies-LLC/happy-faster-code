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
) -> str:
    """Run the RLM agent with HappyRepo tools against a repository.

    Args:
        path: Path to the repository to analyze.
        query: The analysis query to run.
        model: LiteLLM model string override. If None, reads from config.
        max_depth: Maximum recursion depth for sub-queries.
        verbose: Whether to print intermediate steps.

    Returns:
        The agent's final response string.
    """
    from rlm import RLM

    from happy_faster_code import HappyRepo
    from happy_faster_code.config import load_config
    from happy_faster_code.rlm_tools import build_rlm_namespace, build_system_prompt
    from happy_faster_code.worker import build_delegate

    # Load config (TOML + env vars), allow model override
    config = load_config(path)
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

    # Build repo and tools
    repo = HappyRepo(path)
    namespace = build_rlm_namespace(repo, path)
    system_prompt = build_system_prompt(repo)

    # Add delegate function for recursive sub-queries
    worker_model = config.get("worker_model") or litellm_model
    delegate_fn = build_delegate(repo, path, worker_model)
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

    args = parser.parse_args()

    response = run(
        path=args.path,
        query=args.query,
        model=args.model,
        max_depth=args.max_depth,
        verbose=not args.quiet,
    )

    if args.json:
        print(json.dumps({"query": args.query, "response": response}))
    else:
        print(response)


if __name__ == "__main__":
    main()
