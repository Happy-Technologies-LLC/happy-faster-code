"""Config reader for happycode — loads .happy/agent.toml with env var overrides."""

import os
import sys
from pathlib import Path

from happy_faster_code.integration import normalize_mode

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
        dict with keys: provider, model, api_key, api_base, worker_model, mode
    """
    config = {
        "provider": "anthropic",
        "model": "claude-sonnet-4-6",
        "api_key": "",
        "api_base": None,
        "worker_model": None,
        "mode": "all-in-one",
        "volt_enabled": False,
        "volt_api_base": None,
        "volt_api_key": None,
        "volt_search_path": "/api/memory/search",
        "volt_conversation_id": None,
        "volt_top_k": 8,
        "volt_bootstrap_query": None,
    }

    # Load .happy/agent.toml
    toml_path = Path(repo_path) / ".happy" / "agent.toml"
    if toml_path.exists() and tomllib is not None:
        try:
            with open(toml_path, "rb") as f:
                data = tomllib.load(f)
            for key in (
                "provider",
                "model",
                "api_key",
                "api_base",
                "worker_model",
                "mode",
                "volt_enabled",
                "volt_api_base",
                "volt_api_key",
                "volt_search_path",
                "volt_conversation_id",
                "volt_top_k",
                "volt_bootstrap_query",
            ):
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

    if env_mode := os.environ.get("HAPPY_MODE"):
        config["mode"] = env_mode

    if env_volt_enabled := os.environ.get("HAPPY_VOLT_ENABLED"):
        config["volt_enabled"] = env_volt_enabled.lower() in ("1", "true", "yes", "on")

    if env_volt_api_base := os.environ.get("HAPPY_VOLT_API_BASE"):
        config["volt_api_base"] = env_volt_api_base

    if env_volt_api_key := os.environ.get("HAPPY_VOLT_API_KEY"):
        config["volt_api_key"] = env_volt_api_key

    if env_volt_search_path := os.environ.get("HAPPY_VOLT_SEARCH_PATH"):
        config["volt_search_path"] = env_volt_search_path

    if env_volt_conversation_id := os.environ.get("HAPPY_VOLT_CONVERSATION_ID"):
        config["volt_conversation_id"] = env_volt_conversation_id

    if env_volt_top_k := os.environ.get("HAPPY_VOLT_TOP_K"):
        try:
            config["volt_top_k"] = max(1, int(env_volt_top_k))
        except ValueError:
            pass

    if env_volt_bootstrap_query := os.environ.get("HAPPY_VOLT_BOOTSTRAP_QUERY"):
        config["volt_bootstrap_query"] = env_volt_bootstrap_query

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

    try:
        config["mode"] = normalize_mode(config["mode"])
    except ValueError:
        config["mode"] = "all-in-one"

    # Normalize optional Volt fields loaded from TOML.
    try:
        config["volt_top_k"] = max(1, int(config.get("volt_top_k", 8)))
    except (TypeError, ValueError):
        config["volt_top_k"] = 8

    config["volt_enabled"] = bool(config.get("volt_enabled", False))

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
