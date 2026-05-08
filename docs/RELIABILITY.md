# Reliability

**状态：** 长期可靠性文档
**最后验证：** 2026-05-08

## Reliability Goals

CCR should fail in ways that are visible, recoverable and local.

Key goals:

- Starting and stopping the local server should be predictable.
- Client config injection should be reversible.
- Route Pool should explain skipped, failed, retried and banned candidates.
- Upstream errors should preserve enough context to diagnose provider, endpoint, model and API kind issues.
- Logs and metrics should be readable by both users and agents.
- Admin API should be default-off so recovery and management work stays inside
  the UI and documented diagnostics unless explicitly enabled.

## Important Failure Modes

- Server is not running.
- Server is running on a different port than client config expects.
- API key mismatch between UI/client and server.
- Provider endpoint is unavailable.
- Provider returns 401/403/429/5xx.
- Provider API kind is wrong.
- Upstream model is missing or ambiguous.
- Transformer chain produces unsupported request shape.
- Client config file contains user-owned fields that must not be overwritten.
- Route Pool candidates are all disabled or banned.
- Linux/WSL desktop startup has tray, Wayland/X11 and GL/EGL differences.

## Expected Diagnostics

For upstream attempts, logs should include:

- route
- provider
- endpoint
- upstream model
- inbound protocol
- provider API kind where available
- HTTP status or network error
- latency
- retry reason
- redacted headers
- concise response summary

For client injection, status should include:

- config path
- whether CCR-managed config exists
- whether endpoint points to the current server port
- whether action is additive or overwrite/restore based

## Recovery Principles

- Config writes should create backups when overwriting.
- Additive config writes should only remove CCR-managed fragments.
- Server runtime should return clear errors when Route Pool is not configured.
- UI should expose last operation result separately from long-lived status.
- Route Pool bans should have visible expiration and eventually a manual clear action.
- Desktop UI startup should prefer the main window over enhancements. System tray
  setup is best-effort and must not be a hard dependency for opening the UI.

## Linux/WSL UI Startup

For WSL2 and remote desktop sessions, treat Wayland and GL/EGL startup as
environment-dependent.

Current verified fallback:

```sh
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET CCR_DISABLE_TRAY=1 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui
```

Notes:

- `winit 0.30` no longer supports `WINIT_UNIX_BACKEND`; do not document it as a
  supported fallback.
- Removing `WAYLAND_DISPLAY` and `WAYLAND_SOCKET` forces winit away from the
  Wayland path when `DISPLAY` is available.
- `GDK_BACKEND=x11` keeps GTK/tray dependencies aligned with X11 when GTK is
  initialized.
- `LIBGL_ALWAYS_SOFTWARE=1` avoids depending on unavailable WSL GPU/EGL paths.
- `CCR_DISABLE_TRAY=1` keeps tray initialization out of the critical startup
  path.

## Long-Term Reliability Work

- Runtime metrics store for request and attempt metrics.
- Route Pool health score based on real traffic.
- Structured log query API.
- Agent-friendly local run script with isolated config and logs.
- CI running tests and documentation checks.
- Default-off admin API with tests for disabled/enabled behavior if retained.
- Linux UI startup smoke checks for WSL/X11 fallback once an automated GUI test
  harness exists.
