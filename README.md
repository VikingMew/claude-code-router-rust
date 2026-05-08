# Claude Code Router Rust

CCR Rust is a local desktop router for AI coding clients. It lets Claude Code,
Codex, OpenCode and OpenClaw connect to one local server, then routes requests
to your configured upstream providers and models.

```text
Claude Code / Codex / OpenCode / OpenClaw
        -> local CCR server
        -> Route Pool
        -> OpenAI / Anthropic / OpenRouter / DeepSeek / Groq / compatible APIs
```

The default workflow is through-CCR: clients talk to `127.0.0.1`, and CCR owns
provider selection, model routing, protocol conversion, retries, diagnostics and
logs.

## Why Use It

- Use multiple coding clients through one local router.
- Switch upstream providers and models without rewriting every client config.
- Route requests through an ordered Route Pool with retry and ban state.
- Connect Claude Code and Codex to the local CCR server safely.
- Test provider endpoints and inspect logs from a desktop UI.
- Keep client-visible model names separate from upstream provider/model routes.

## Status

This repository is the Rust implementation of CCR. The core server, CLI, desktop
UI, Route Pool runtime, client config injection and basic runtime metrics are in
place. Request-level response metrics and deeper observability are active work.

## Quick Start

CCR is a desktop app first. In the current development version, you start that
desktop app from source with Cargo.

### 1. Install System Dependencies

Ubuntu/Linux hosts need native libraries for the UI, tray icon, TLS and X11
helpers:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev libgtk-3-dev libxdo-dev libayatana-appindicator3-dev
```

Install Rust stable if you do not already have it:

```sh
rustup default stable
```

### 2. Start The Desktop App

```sh
cargo run --bin ccr-ui
```

Use the UI to start/stop the local server, manage providers, test endpoints,
edit Route Pool candidates and inspect logs.

Linux/WSL fallback:

```sh
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET CCR_DISABLE_TRAY=1 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui
```

`CCR_DISABLE_TRAY=1` skips the system tray. Removing `WAYLAND_DISPLAY` and
setting `GDK_BACKEND=x11` avoids Wayland/glutin configuration failures seen in
some WSL sessions.
`LIBGL_ALWAYS_SOFTWARE=1` asks Mesa to use software rendering when WSL, remote
desktops or incomplete GPU/EGL drivers cannot create a hardware context.

### 3. Configure From The UI

In the desktop app:

- add at least one provider;
- test the provider endpoint;
- add one or more Route Pool candidates;
- start the local CCR server from the Status tab;
- connect Claude Code, Codex, OpenCode or OpenClaw from the client controls.

The UI is the intended place for normal setup and daily use. Client config
actions should preserve unrelated user-owned config and provide rollback where
supported.

### 4. Check The Local Server

After starting the server from the UI:

```sh
curl http://127.0.0.1:3456/health
```

## Configuration

Default config path:

```text
~/.claude-code-router/config.json
```

Minimal Route Pool shape:

```json
{
  "HOST": "127.0.0.1",
  "PORT": 3456,
  "Providers": [
    {
      "name": "openai",
      "api_base_url": "https://api.openai.com/v1/responses",
      "api_key": "$OPENAI_API_KEY",
      "models": ["gpt-5-codex"],
      "api_kind": "openai_responses"
    }
  ],
  "RoutePool": {
    "enabled": true,
    "candidates": [
      {
        "route": "openai,gpt-5-codex",
        "enabled": true,
        "priority": 1
      }
    ]
  }
}
```

Route formats:

- `provider,model` means CCR always sends that upstream model.
- `provider` means CCR uses the model from the inbound client request.

Do not configure `Router.default`, `Failover`, `ProviderPool`, Primary Route or
default route fallback. Route Pool is the primary routing model.

## CLI

The CLI exists for automation, debugging and development. It is not the primary
product surface.

Common commands:

```sh
cargo run --bin ccr -- start
cargo run --bin ccr -- stop
cargo run --bin ccr -- restart
cargo run --bin ccr -- status
cargo run --bin ccr -- preset list
```

For installed binaries, use `ccr` directly instead of `cargo run --bin ccr --`.

## API

Main local endpoints:

- `GET /health`
- `POST /v1/messages`
- `POST /v1/responses`
- `POST /v1/messages/count_tokens`
- `GET /api/config`
- `GET /api/logs`
- `GET /api/logs/query`
- `GET /api/route-pool/status`
- `GET /api/runtime-metrics/attempts`
- `GET /api/runtime-metrics/summary`
- `GET /api/presets`

Claude-style clients use `/v1/messages`. Codex-style clients use
`/v1/responses`.

## Development

```sh
cargo fmt --check
cargo test --package ccr-server
cargo test --package ccr-app-core --lib
cargo test --package ccr-cli --lib
cargo test --workspace
```

Agent-friendly local server run:

```sh
scripts/agent-run-local
```

The script uses a temporary `HOME`, random port and isolated config/log paths so
it does not modify your real CCR config.

## Repository Map

- `crates/ccr-server` - local HTTP server and upstream routing runtime.
- `crates/ccr-ui` - egui desktop app.
- `crates/ccr-cli` - command-line helper and client config injection.
- `crates/ccr-app-core` - UI-independent application logic.
- `crates/ccr-types` - shared config and API types.
- `crates/ccr-router` - route selection and token counting helpers.
- `crates/ccr-transformer` - protocol/provider transformers.
- `crates/ccr-config` - config load/save/reload and backups.
- `crates/ccr-preset` - preset/profile management.
- `docs/` - architecture, design, reliability, security and plans.
- `packaging/` - macOS and Windows packaging scripts.

Start with `ARCHITECTURE.md` and `docs/index.md` when changing internals.

## Docs

- `ARCHITECTURE.md` - crate boundaries, runtime paths and invariants.
- `docs/index.md` - documentation entry point.
- `docs/long-term-roadmap.md` - long-term product and engineering direction.
- `docs/release.md` - release and platform prerequisite notes.
- `docs/exec-plans/tech-debt-tracker.md` - active debt and follow-up work.

## License

No license file is currently present in this repository.
