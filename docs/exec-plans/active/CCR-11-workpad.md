# CCR-11 Workpad

## Scope Interpretation

- Newest human comment confirms the task is scoped only to moving `StatusTab`
  Route Pool/runtime metrics fetch and DTO ownership into `ccr-app-core`.
- Preserve current Status tab behavior, including the server-running and active
  Route Pool refresh gates, two-second refresh cadence, clearing behavior when
  unavailable, and existing warning semantics.
- Do not expand this into broader UI HTTP cleanup.
