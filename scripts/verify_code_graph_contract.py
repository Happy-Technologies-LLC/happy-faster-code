#!/usr/bin/env python3
"""Verify tool contract JSON stays in sync with Rust code graph tool registration."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_FILE = REPO_ROOT / "core" / "src" / "tools" / "handlers" / "code_graph.rs"
CONTRACT_FILE = REPO_ROOT / "adapters" / "tool_contracts" / "code_graph_tools.json"


def load_rust_tools() -> list[str]:
    text = RUST_FILE.read_text(encoding="utf-8")
    start = text.find("pub static CODE_GRAPH_TOOL_NAMES")
    end = text.find("];", start)
    if start == -1 or end == -1:
        raise RuntimeError("Could not locate CODE_GRAPH_TOOL_NAMES in Rust source")
    block = text[start:end]
    return re.findall(r'\("([a-z_]+)",\s*"', block)


def load_contract_tools() -> list[str]:
    data = json.loads(CONTRACT_FILE.read_text(encoding="utf-8"))
    return [tool["name"] for tool in data.get("tools", [])]


def main() -> int:
    rust_tools = load_rust_tools()
    contract_tools = load_contract_tools()

    rust_set = set(rust_tools)
    contract_set = set(contract_tools)

    missing_in_contract = sorted(rust_set - contract_set)
    extra_in_contract = sorted(contract_set - rust_set)

    if missing_in_contract or extra_in_contract:
        if missing_in_contract:
            print("Missing in contract:", ", ".join(missing_in_contract), file=sys.stderr)
        if extra_in_contract:
            print("Extra in contract:", ", ".join(extra_in_contract), file=sys.stderr)
        return 1

    if rust_tools != contract_tools:
        print("Contract tool order differs from Rust registration order", file=sys.stderr)
        return 1

    print(f"OK: {len(rust_tools)} tools are in sync")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
