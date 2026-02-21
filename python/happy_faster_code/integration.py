"""Integration mode helpers for happycode."""

from __future__ import annotations


VALID_MODES = ("all-in-one", "mcp", "skills")


def normalize_mode(value: str | None) -> str:
    """Normalize user-provided mode names to canonical values."""
    if not value:
        return "all-in-one"

    raw = value.strip().lower().replace("_", "-")
    aliases = {
        "all": "all-in-one",
        "allinone": "all-in-one",
        "full": "all-in-one",
        "fork": "all-in-one",
        "all-in-one": "all-in-one",
        "mcp": "mcp",
        "skills": "skills",
    }
    mode = aliases.get(raw)
    if mode is None:
        raise ValueError(
            f"invalid mode '{value}'. Expected one of: {', '.join(VALID_MODES)}"
        )
    return mode


def resolve_mode(explicit_mode: str | None, config_mode: str | None) -> str:
    """Resolve mode with precedence: explicit arg > config > default."""
    return normalize_mode(explicit_mode or config_mode or "all-in-one")
