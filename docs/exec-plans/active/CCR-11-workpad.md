# CCR-11 Workpad

## Scope Interpretation

- Newest human comment confirms the task is scoped only to moving `StatusTab`
  Route Pool/runtime metrics fetch and DTO ownership into `ccr-app-core`.
- Preserve current Status tab behavior, including the server-running and active
  Route Pool refresh gates, two-second refresh cadence, clearing behavior when
  unavailable, and existing warning semantics.
- Do not expand this into broader UI HTTP cleanup.

## Continuation 2

- Local branch `vikingmew-ccr-11` is clean and pushed at `3d90310`.
- Remaining handoff is blocked in this continuation session:
  - `linear_task_read` returns `Workflow profile is unavailable for this Codex session`.
  - `gh` is not installed, no GitHub MCP resources are available, and no GitHub/GH token environment variable is present beyond `GH_PAGER`.
