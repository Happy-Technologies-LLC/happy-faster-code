"""Backward-compatibility shim for happy_faster_code.orchestrator."""

from happy_code.orchestrator import *  # noqa: F401,F403

if __name__ == "__main__":
    from happy_code.orchestrator import main

    main()
