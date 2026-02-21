"""Mode-aware launcher for happycode integration tiers."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys

from happy_faster_code.config import load_config
from happy_faster_code.integration import VALID_MODES, normalize_mode, resolve_mode


def build_launch_command(mode: str, passthrough: list[str]) -> tuple[list[str], dict[str, str]]:
    """Build the command and extra env vars for a given integration mode."""
    normalized = normalize_mode(mode)

    if normalized == "all-in-one":
        return ["happycode", *passthrough], {}

    if normalized == "mcp":
        return ["happycode", "mcp-server", *passthrough], {}

    # skills mode
    return ["happycode", *passthrough], {"HAPPY_INTEGRATION_MODE": "skills"}


def run_launch(
    mode: str,
    passthrough: list[str],
    *,
    print_only: bool = False,
) -> int:
    """Execute (or print) the launch command for the chosen mode."""
    cmd, extra_env = build_launch_command(mode, passthrough)

    if print_only:
        print(
            json.dumps(
                {
                    "mode": normalize_mode(mode),
                    "command": cmd,
                    "env": extra_env,
                }
            )
        )
        return 0

    env = os.environ.copy()
    env.update(extra_env)
    try:
        completed = subprocess.run(cmd, env=env, check=False)
    except FileNotFoundError:
        print(
            "happycode binary not found on PATH. Build/install the CLI first.",
            file=sys.stderr,
        )
        return 127
    return completed.returncode


def main(argv: list[str] | None = None) -> int:
    """CLI entry point for `happy-launch`."""
    parser = argparse.ArgumentParser(
        description="Launch happycode in all-in-one, mcp, or skills mode"
    )
    parser.add_argument(
        "--mode",
        choices=list(VALID_MODES),
        default=None,
        help="Integration mode override. Defaults to HAPPY_MODE/.happy/agent.toml/all-in-one.",
    )
    parser.add_argument(
        "--path",
        default=".",
        help="Repo path used for reading .happy/agent.toml (default: current dir).",
    )
    parser.add_argument(
        "--print-only",
        action="store_true",
        help="Print resolved command/env as JSON without executing.",
    )
    parser.add_argument(
        "passthrough",
        nargs=argparse.REMAINDER,
        help="Arguments to pass through to happycode. Use -- before args.",
    )

    args = parser.parse_args(argv)

    passthrough = args.passthrough
    if passthrough and passthrough[0] == "--":
        passthrough = passthrough[1:]

    config = load_config(args.path)
    selected_mode = resolve_mode(args.mode, config.get("mode"))

    return run_launch(selected_mode, passthrough, print_only=args.print_only)


if __name__ == "__main__":
    raise SystemExit(main())
