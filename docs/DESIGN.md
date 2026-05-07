# Design

**状态：** 长期设计原则
**最后验证：** 2026-05-07

## Product Experience

CCR is an operational desktop tool. The interface should feel calm, dense and predictable. Users are configuring local routing, providers, clients and diagnostics; they need fast scanning and safe actions more than marketing-style presentation.

## Primary Workflows

The product should make these workflows first-class:

- See whether the server is running.
- See whether each supported client points to the current local CCR server.
- Start, stop and check server health.
- Add and edit providers.
- Test provider endpoints.
- Build and reorder Route Pool candidates.
- Inspect logs and route-pool runtime state.
- Apply presets without losing user-owned config.

## Interaction Principles

- Prefer explicit status over hidden state.
- Show current config and live runtime state separately.
- Destructive or config-overwriting operations need clear preview or rollback.
- Additive client config writes must not remove unrelated user config.
- Endpoint test results should be diagnostic, not just pass/fail.
- Route Pool state should explain why a candidate is skipped, banned or retried.

## Copy Principles

- Use through-CCR terminology consistently.
- Do not call Route Pool candidates "default route" or "primary route".
- Do not call OpenCode/OpenClaw additive provider operations "activate" when the action only adds a CCR provider fragment.
- Error messages should include the failed provider, endpoint, API kind and concise upstream summary when available.

## Non-Goals

- No landing page inside the desktop app.
- No decorative dashboard that hides operational information.
- No provider kind leakage into default client injection flows.
- No automatic direct-to-provider behavior without explicit advanced mode.
