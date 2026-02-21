# Adapters

This folder isolates integration surfaces for the same core capabilities.

- `all_in_one/`: full in-process Codex fork integration.
- `mcp/`: MCP-oriented adapter documentation and future implementation hooks.
- `skills/`: lightweight skills/scripts adapter documentation.
- `tool_contracts/`: shared tool schemas used across adapters.

Verify contract alignment with runtime registration:

```bash
python3 scripts/verify_code_graph_contract.py
```
