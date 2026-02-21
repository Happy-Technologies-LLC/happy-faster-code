# all-in-one Adapter

This adapter is the in-repo full integration path.

- Graph tools are registered directly in Codex runtime (`core/src/tools/handlers/code_graph.rs`).
- Background indexing and incremental file watching are session-native.
- Highest control, highest merge/rebase maintenance.

Run:

```bash
happy-launch --mode all-in-one
```
