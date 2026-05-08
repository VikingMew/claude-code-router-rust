# AGENTS.md

This repository is the Rust implementation of CCR, a local desktop router for Claude Code, Codex, OpenCode and OpenClaw.

Use this file as the starting map. Do not treat it as the full manual.

## Read First

- `ARCHITECTURE.md` - crate boundaries, runtime paths and core invariants.
- `docs/index.md` - product, design, reliability, security and long-term docs.
- `docs/long-term-roadmap.md` - long-term direction and decision principles.
- `docs/exec-plans/index.md` - execution plan workflow and active/completed structure.
- `docs/exec-plans/tech-debt-tracker.md` - known technical debt and follow-up work.

## Product Rules

- The product is UI first. CLI exists for automation and debugging.
- Default mode is through-CCR: clients connect to the local CCR server, not directly to upstream providers.
- Route Pool is the only primary routing model.
- Provider API kind affects server upstream requests, endpoint tests and transformer recommendations. It must not leak into default client injection semantics.
- Do not reintroduce `Router.default`, `Failover`, `ProviderPool` runtime behavior, Primary Route UI or default route fallback.

## Common Commands

```sh
cargo fmt --check
cargo test --workspace
cargo test --package ccr-server
cargo test --package ccr-ui
```

Use narrower package tests while iterating, then run workspace tests before finishing broad changes.

## Repository Map

- `crates/ccr-server` - local HTTP server and upstream routing runtime.
- `crates/ccr-ui` - egui desktop app.
- `crates/ccr-cli` - `ccr` command-line helper and client config injection.
- `crates/ccr-app-core` - UI-independent application logic.
- `crates/ccr-types` - shared config and API data types.
- `crates/ccr-router` - request routing and token counting helpers.
- `crates/ccr-transformer` - request/response protocol transformers.
- `crates/ccr-config` - config load/save/reload and backups.
- `docs/` - long-term product and engineering records.
- `docs/exec-plans/` - active and completed execution plans.
- `packaging/` - macOS and Windows packaging scripts.

## Working Rules

- Prefer existing crate boundaries and local helper APIs.
- Move business logic out of `ccr-ui` when practical; keep egui code focused on rendering and dispatch.
- Preserve user configuration unless the task explicitly asks for migration or removal.
- When changing routing behavior, update server tests, UI/status behavior and docs together.
- When changing client injection, verify Claude/Codex/OpenCode/OpenClaw semantics independently.
- When adding long-running behavior, add logs or metrics that Codex can inspect.

## Planning Rules

- Small changes can be implemented directly.
- Complex work should get an execution plan under `docs/exec-plans/active/`.
- Execution plans follow PDCA: the normal plan sections are Plan; record actual
  work in Do, verification and deviations in Check, and follow-up/debt/doc
  updates in Act.
- Completed execution plans should record verification commands and move to `docs/exec-plans/completed/`.
- Technical debt should be tracked in `docs/exec-plans/tech-debt-tracker.md`.
- Historical phase files live under `docs/exec-plans/completed/legacy-phase-*.md`.

## Documentation Rules

- Keep docs short, linked and verifiable.
- Long-term docs need status and last verification date.
- If code and docs disagree, trust code first, then update docs or open a debt item.
- Historical phase docs may contain old terms; current docs must not present removed behavior as supported.
