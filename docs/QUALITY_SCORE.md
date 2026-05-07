# Quality Score

**状态：** 长期质量评分文档
**最后验证：** 2026-05-07

This is a lightweight scorecard for judging whether the repository is becoming easier for users and agents to maintain.

## Current Scores

| Area | Score | Notes |
| --- | ---: | --- |
| Build and test basics | 6/10 | Workspace tests exist, but no visible CI workflow yet. |
| Architecture readability | 5/10 | Crate names are clear; top-level architecture doc was missing until this documentation pass. |
| UI/core separation | 6/10 | `ccr-app-core` exists, but UI still owns some operation logic. |
| Routing model clarity | 8/10 | Route Pool is current model; provider-pool status alias has been removed. |
| Client injection clarity | 6/10 | Multiple clients supported; unified client abstraction still missing. |
| Observability | 5/10 | Logs exist; structured query and runtime metrics are not complete. |
| Documentation system | 5/10 | Good phase history exists; indexes and ownership rules are new/incomplete. |
| Release process | 4/10 | Packaging scripts exist; unified release checklist missing. |

## Quality Gates

Before a broad change is considered done:

- `cargo fmt --check` passes.
- relevant package tests pass.
- `cargo test --workspace` passes unless a blocker is documented.
- user-facing behavior has docs or phase notes.
- routing/client/config changes update long-term docs or debt tracker when needed.

## Improvement Targets

P0:

- Add CI.
- Add architecture and agent docs.
- Keep Route Pool-only routing terminology consistent.

P1:

- Add docs/plans index validation.
- Move more UI behavior into app-core.
- Add structured log query.

P2:

- Add release checklist.
- Add agent-friendly local run script.
