# Frontend

**状态：** 长期 UI 实现指南
**最后验证：** 2026-05-07

## Scope

The current frontend is a native egui app in `crates/ccr-ui`.

UI code should render state and dispatch actions. Business logic should move to `ccr-app-core`, server APIs or CLI helper modules when it needs testing or reuse.

## Layout Principles

- Status is the first screen.
- Use full-width sections and compact controls.
- Avoid nested card layouts.
- Keep tables, lists and route controls stable under dynamic content.
- Long text should wrap without overlapping neighboring controls.
- Logs and diagnostics should be easy to filter and copy from the UI.

## State Principles

- Separate local UI operation status from long-lived runtime status.
- Avoid blocking network calls in egui rendering paths.
- Represent loading/checking states explicitly.
- Keep apply/cancel flows for config changes where the user can lose context.

## Expected Tabs

- Status: server, clients, routing runtime and health.
- Providers: provider config, API kind, models, endpoint candidates and inline test.
- Router: Route Pool construction and ordering.
- Transformers: available transformers and provider transformer selection.
- Presets: list, apply, export and delete.
- Logs: app/server diagnostics with filtering.
- Token counter: request token estimate support.
- Settings: app, server, client paths, proxy, backup and advanced options.

## Testing Direction

- Put data transformation, sorting and config mutation tests in `ccr-app-core`.
- Keep `ccr-ui` tests focused on smoke and view-model behavior.
- When adding a user-visible workflow, add at least one test at the lowest practical layer.
