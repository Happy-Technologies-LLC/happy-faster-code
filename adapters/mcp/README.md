# mcp Adapter

This adapter isolates graph and analysis capabilities behind MCP interfaces.

Current state:

- `happy-launch --mode mcp` starts the existing Codex MCP server path (`happycode mcp-server`).
- Shared tool names and parameter contracts are defined in `adapters/tool_contracts/code_graph_tools.json`.

Recommended next step for contributors:

- Implement a dedicated happycode MCP service that serves the contract toolset directly from `happy-core`.

Run:

```bash
happy-launch --mode mcp
```
