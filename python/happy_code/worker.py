"""Worker model delegation for recursive RLM sub-queries."""


def build_delegate(
    repo,
    repo_path: str,
    worker_model: str,
    *,
    recall_memory=None,
    memory_context: str | None = None,
):
    """Build a delegate function that runs sub-queries on a worker model.

    The returned function is injected into the RLM namespace so the frontier
    model can call ``delegate("analyze the auth module")`` to fan out work.

    Args:
        repo: HappyRepo instance (shared with the parent agent).
        repo_path: Path to the repository.
        worker_model: LiteLLM model string for the worker.

    Returns:
        A callable ``delegate(prompt: str) -> str``.
    """

    def delegate(prompt: str) -> str:
        """Delegate a sub-query to a worker model with full repo access."""
        from rlm import RLM

        from .rlm_tools import build_rlm_namespace, build_system_prompt

        namespace = build_rlm_namespace(
            repo,
            repo_path,
            recall_memory=recall_memory,
        )
        system_prompt = build_system_prompt(repo, memory_context=memory_context)

        worker = RLM(
            backend="litellm",
            backend_kwargs={"model_name": worker_model},
        )

        result = worker.completion(
            prompt=prompt,
            system_prompt=system_prompt,
            code_tools=namespace,
            max_depth=1,  # Workers don't recurse further
            verbose=False,
        )

        return result.response

    return delegate
