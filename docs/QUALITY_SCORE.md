# Quality Score

**状态：** 长期质量评分文档
**最后验证：** 2026-05-08

This is a lightweight scorecard for judging whether the repository is becoming easier for users and agents to maintain.

## Current Scores

| Area | Score | Notes |
| --- | ---: | --- |
| Build and test basics | 7/10 | CI exists and runs docs structure, fmt and workspace tests; Ubuntu hosts need documented native UI dependencies such as `libxdo-dev`. |
| Architecture readability | 7/10 | Top-level `ARCHITECTURE.md` exists and maps crate responsibilities and runtime paths. |
| UI/core separation | 6/10 | `ccr-app-core` exists, but UI still owns some operation logic. |
| Routing model clarity | 8/10 | Route Pool is current model; provider-pool status alias has been removed. |
| Client injection clarity | 6/10 | Multiple clients supported; unified client abstraction still missing. |
| Observability | 6/10 | Structured log query and minimal runtime metrics exist; request-level response metrics are still active work. |
| Documentation system | 7/10 | Docs index, execution-plan structure and tech debt tracker exist. |
| Release process | 5/10 | Packaging scripts and release checklist exist; signing/notarization and platform smoke automation are incomplete. |

## Quality Gates

Before a broad change is considered done:

- `cargo fmt --check` passes.
- relevant package tests pass.
- `cargo test --workspace` passes unless a blocker is documented.
- user-facing behavior has docs or phase notes.
- routing/client/config changes update long-term docs or debt tracker when needed.

## Improvement Targets

P0:

- Keep Route Pool-only routing terminology consistent.
- Keep CI green on Ubuntu with documented native dependencies.

P1:

- Add docs/plans index validation.
- Move more UI behavior into app-core.
- Complete request-level runtime metrics alignment.

P2:

- Add more release smoke automation.
- Document signing and notarization when implemented.
