"""RLM orchestrator - runs the Recursive Language Model with HappyRepo tools."""

import argparse
import sys


def run(path: str, query: str, model: str = "gpt-4o", max_depth: int = 2, verbose: bool = True):
    """Run the RLM agent with HappyRepo tools against a repository."""
    from rlm import RLM
    from happy_faster_code import HappyRepo
    from happy_faster_code.rlm_tools import build_rlm_tools

    repo = HappyRepo(path)
    tools = build_rlm_tools(repo)

    agent = RLM(
        backend="litellm",
        backend_kwargs={"model_name": model},
        custom_tools=tools,
        max_depth=max_depth,
        verbose=verbose,
    )

    result = agent.completion(query)
    return result.response


def main():
    parser = argparse.ArgumentParser(description="HappyFasterCode RLM Orchestrator")
    parser.add_argument("--path", required=True, help="Path to the repository")
    parser.add_argument("--query", required=True, help="Query to run")
    parser.add_argument("--model", default="gpt-4o", help="LLM model to use")
    parser.add_argument("--max-depth", type=int, default=2, help="Max recursion depth")
    parser.add_argument("--quiet", action="store_true", help="Disable verbose output")

    args = parser.parse_args()

    response = run(
        path=args.path,
        query=args.query,
        model=args.model,
        max_depth=args.max_depth,
        verbose=not args.quiet,
    )

    print(response)


if __name__ == "__main__":
    main()
