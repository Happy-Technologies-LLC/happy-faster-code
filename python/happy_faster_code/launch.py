"""Backward-compatibility shim for happy_faster_code.launch."""

from happy_code.launch import *  # noqa: F401,F403

if __name__ == "__main__":
    from happy_code.launch import main

    main()
