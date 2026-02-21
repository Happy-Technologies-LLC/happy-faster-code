"""happycode - Rust-native AI coding agent with high-performance code indexing."""

try:
    from .happy_core import HappyRepo
except ImportError:
    try:
        # Backward-compatible fallback for environments still loading the
        # extension module from the old package path.
        from happy_faster_code.happy_core import HappyRepo
    except ImportError:
        HappyRepo = None

from .config import load_config
from .launch import main as launch_main
from .orchestrator import run as rlm_run

__all__ = ["HappyRepo", "rlm_run", "load_config", "launch_main"]
