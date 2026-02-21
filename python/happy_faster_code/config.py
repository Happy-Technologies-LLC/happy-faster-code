"""Config reader for HappyFasterCode — loads .happy/agent.toml with env var overrides."""

import os
import sys
from pathlib import Path

if sys.version_info >= (3, 11):
    import tomllib
else:
    try:
        import tomli as tomllib
    except ImportError:
        tomllib = None


def load_config(repo_path: str = ".") -> dict:
    """Load model configuration from .happy/agent.toml with env var overrides.

    Priority: env vars > TOML > defaults.

    Returns:
        dict with keys: model, api_key, api_base, worker_model
    """
    config = {
        "provider": "anthropic",
        "model": "claude-sonnet-4-6",
        "api_key": "",
        "api_base": None,
        "worker_model": None,
    }

    # Load .happy/agent.toml
    toml_path = Path(repo_path) / ".happy" / "agent.toml"
    if toml_path.exists() and tomllib is not None:
        try:
            with open(toml_path, "rb") as f:
                data = tomllib.load(f)
            for key in ("provider", "model", "api_key", "api_base", "worker_model"):
                if key in data and data[key] is not None:
                    config[key] = data[key]
        except Exception:
            pass  # Silently fall through to defaults/env vars

    # Env var overrides
    if env_provider := os.environ.get("HAPPY_PROVIDER"):
        config["provider"] = env_provider.lower()

    if env_model := os.environ.get("HAPPY_MODEL"):
        config["model"] = env_model

    if env_worker := os.environ.get("HAPPY_WORKER_MODEL"):
        config["worker_model"] = env_worker

    # Auto-detect provider and API key from environment
    if not config["api_key"]:
        if anthropic_key := os.environ.get("ANTHROPIC_API_KEY"):
            config["api_key"] = anthropic_key
            if "HAPPY_PROVIDER" not in os.environ:
                config["provider"] = "anthropic"
        elif openai_key := os.environ.get("OPENAI_API_KEY"):
            config["api_key"] = openai_key
            if "HAPPY_PROVIDER" not in os.environ:
                config["provider"] = "openai"

    # Match API key to provider if both env vars are present
    if config["provider"] == "anthropic":
        if anthropic_key := os.environ.get("ANTHROPIC_API_KEY"):
            config["api_key"] = anthropic_key
    elif config["provider"] == "openai":
        if openai_key := os.environ.get("OPENAI_API_KEY"):
            config["api_key"] = openai_key

    # Build LiteLLM model string
    config["litellm_model"] = _build_litellm_model(config)

    return config


def _build_litellm_model(config: dict) -> str:
    """Convert provider/model to a LiteLLM-compatible model string."""
    provider = config["provider"]
    model = config["model"]

    if config.get("api_base"):
        # Custom endpoint — use openai/ prefix for LiteLLM routing
        os.environ.setdefault("OPENAI_API_BASE", config["api_base"])
        return f"openai/{model}"

    if provider == "anthropic":
        return f"anthropic/{model}"

    # OpenAI and others pass through directly
    return model
