# CCR-21 Workpad

**Status:** implementation workpad
**Last updated:** 2026-05-22

## Interpretation

The newest human Linear comment says to decide whether legacy scenario routing is compatibility to remove/document, or to reintroduce it only as Route Pool policy/rule. This implementation treats `background`, `think`, `webSearch`, `longContext`, `image`, project router override and custom router override as not-current runtime goals. The code path remains Route Pool-only, so unused `ccr-router` primary/scenario routing APIs are removed and surviving config fields are documented as legacy deserialization compatibility.

## Continuation Notes

- 2026-05-22 continuation #6: latest human Linear comment says "有代码合并冲突". Interpretation: prior CCR-21 implementation scope remains accepted; this continuation integrates current `origin/master` into `vikingmew-ccr-21`, resolves conflicts while preserving Route Pool-only runtime semantics, re-runs required validation, and pushes the required branch.
- 2026-05-21 continuation #2: branch `vikingmew-ccr-21` was clean and pushed at `fea95f476cf624aee8c56d41e43ea69246c85645`; restricted Linear API returned `Workflow profile is unavailable for this Codex session`, so the issue state could not be read or transitioned.
- 2026-05-21 continuation #3: branch remained clean and pushed at `fea95f476cf624aee8c56d41e43ea69246c85645`; required legacy API `rg` remained empty; `cargo fmt --check` and `cargo test --package ccr-router` passed. Public GitHub API showed no PR for `VikingMew:vikingmew-ccr-21`, but no GitHub token and no `gh` executable were available to create one. Restricted Linear API still returned `Workflow profile is unavailable for this Codex session`, so the issue state still could not be read or transitioned from this session.
- 2026-05-21 continuation #5: branch remained clean and pushed at `b60232a85f81a909f1b6310dc318c0f8ad1ddac5`; public GitHub API still showed no PR for `VikingMew:vikingmew-ccr-21`; required legacy API `rg` remained empty; `cargo fmt --check` and `cargo test --package ccr-router` passed. Restricted Linear API still returned `Workflow profile is unavailable for this Codex session`. PR creation remained blocked because `gh`/`hub` were unavailable, `GITHUB_TOKEN`/`GH_TOKEN` were unset and the git credential helper could not provide a GitHub HTTPS credential.
