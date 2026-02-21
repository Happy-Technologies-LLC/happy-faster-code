"""happycode - Rust-native AI coding agent with high-performance code indexing."""

try:
    from happy_faster_code.happy_core import HappyRepo
except ImportError:
    HappyRepo = None

from happy_faster_code.config import load_config
from happy_faster_code.launch import main as launch_main
from happy_faster_code.orchestrator import run as rlm_run

__all__ = ["HappyRepo", "rlm_run", "load_config", "launch_main"]
