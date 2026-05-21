# CCR-15 - Claude/Codex Path Overrides

**状态：** Completed
**Last verified:** 2026-05-21

## Plan

Wire Claude and Codex client config path overrides through `ccr-app-core` so
Settings, status, activate and deactivate resolve the same target files.

This closes the plan-002/TD-009 follow-up where client config business logic was
moved out of UI but Claude/Codex still ignored their saved path overrides.

## Do

- Claude now resolves `AppSettings.claude_config_path` to the settings file and
  resolves the plugin `config.json` beside it.
- Codex now resolves `AppSettings.codex_config_path` to `config.toml` and
  resolves `auth.json` beside it.
- Empty or missing overrides continue to use the existing home-directory
  defaults.
- Status, activate, deactivate, backup and missing-marker helpers operate on
  the resolved target paths.

## Check

Unit coverage was added for default and override path resolution, current and
drifted status classification, and activation/restore helpers using override
targets.

Verification commands:

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
```

## Act

No follow-up debt is currently recorded for this ticket.
