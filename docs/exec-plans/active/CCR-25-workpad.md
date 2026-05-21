# CCR-25 Workpad

**Status:** active
**Last updated:** 2026-05-22

## Scope Interpretation

- Latest human comment: "和master有冲突，需要解决后再推送".
- Interpretation: prior implementation scope remains accepted; current work is to integrate current `master` into `vikingmew-ccr-25`, resolve conflicts without changing client injection semantics, run verification, and push the required branch.

## Do

- Merged current `origin/master` into `vikingmew-ccr-25`.
- Resolved conflicts in Claude/Codex client config modules by preserving CCR-25 shared helpers and master path override behavior for Claude plugin config and Codex auth.
- Resolved `status_tab.rs` by keeping master app-core `read_status_snapshot` usage and CCR-25 additive snapshot behavior from app-core.

## Check

- `cargo fmt --check` passed.
- `cargo test --package ccr-app-core client_config` passed: 77 tests.
- `cargo test --package ccr-ui` passed: 32 tests.
- `cargo test --workspace` passed.
