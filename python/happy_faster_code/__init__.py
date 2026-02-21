"""Backward-compatibility shim for legacy happy_faster_code imports."""

try:
    from .happy_core import HappyRepo
except ImportError:
    try:
        from happy_code.happy_core import HappyRepo
    except ImportError:
        HappyRepo = None


def load_config(*args, **kwargs):
    from happy_code.config import load_config as _load_config

    return _load_config(*args, **kwargs)


def launch_main(*args, **kwargs):
    from happy_code.launch import main as _main

    return _main(*args, **kwargs)


def rlm_run(*args, **kwargs):
    from happy_code.orchestrator import run as _run

    return _run(*args, **kwargs)


__all__ = ["HappyRepo", "rlm_run", "load_config", "launch_main"]
