# Integration Tiers

This project supports multiple integration tiers so collaborators can choose complexity vs. control.

## Common Mode Switch

Use either:

- CLI: `happy-launch --mode all-in-one|mcp|skills`
- Env var: `HAPPY_MODE=all-in-one|mcp|skills`

Mode precedence is explicit CLI arg, then config/env, then default `all-in-one`.

## Tier 1: Skills and Scripts (Lowest Integration)

- Keep Codex stock.
- Add workflows through skills and shell/Python scripts.
- Fastest path for experimentation.

Adapter doc: `adapters/skills/README.md`

Best when:

- You want minimal maintenance.
- You are validating ideas quickly.

## Tier 2: MCP Server (Preferred Extensibility Path)

- Run graph/index capabilities behind MCP interfaces.
- Target shared contract: `adapters/tool_contracts/code_graph_tools.json`.
- Reuse Codex tool-calling without forking core runtime.

Adapter doc: `adapters/mcp/README.md`

Best when:

- You want portability across Codex-compatible clients.
- You want lower long-term merge/rebase cost than a full fork.

## Tier 3: Full Fork Integration (Highest Control)

- Integrate directly into Codex runtime (`core`, `cli`, session services, tool registry).
- Enables in-process shared state, startup indexing, and tailored UX.
- Highest power and highest maintenance burden.

Adapter doc: `adapters/all_in_one/README.md`

Best when:

- You need tightly integrated behavior by default.
- You accept ongoing upstream sync effort.

## Contract Sync

Check that adapter contract stays aligned with Rust tool registration:

```bash
python3 scripts/verify_code_graph_contract.py
```

## Current Repo State

happycode currently implements Tier 3, and now includes explicit scaffolding for Tier 2 and Tier 1 collaboration flows.
