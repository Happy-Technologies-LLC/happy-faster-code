"""HappyFasterCode - Rust-native AI coding agent with high-performance code indexing."""

try:
    from happy_faster_code.happy_core import HappyRepo
except ImportError:
    HappyRepo = None

__all__ = ["HappyRepo"]
