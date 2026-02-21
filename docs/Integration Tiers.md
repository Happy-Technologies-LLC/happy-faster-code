# Integration Tiers

This project supports multiple integration tiers so collaborators can choose complexity vs. control.

## Tier 1: Skills and Scripts (Lowest Integration)

- Keep Codex stock.
- Add workflows through skills and shell/Python scripts.
- Fastest path for experimentation.

Best when:

- You want minimal maintenance.
- You are validating ideas quickly.

## Tier 2: MCP Server (Preferred Extensibility Path)

- Run the graph/index capabilities as an MCP server.
- Expose tools like `find_callers`, `search_code`, and `rlm_analyze` through MCP.
- Reuse Codex tool calling without forking core runtime.

Best when:

- You want strong portability across Codex-compatible clients.
- You want lower long-term merge/rebase cost than a fork.

## Tier 3: Full Fork Integration (Highest Control)

- Integrate directly into Codex runtime (`core`, `cli`, session services, tool registry).
- Enables in-process shared state, startup indexing, and tailored UX.
- Highest power and highest maintenance burden.

Best when:

- You need tightly integrated behavior by default.
- You accept ongoing upstream sync effort.

## Current Repo State

happycode currently implements Tier 3, with optional Python orchestration tooling that can also support Tier 2 packaging.
