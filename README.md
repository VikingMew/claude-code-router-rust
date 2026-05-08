# Claude Code Router Rust

CCR Rust is a local desktop app for routing AI coding clients through your own
provider setup.

It gives Claude Code, Codex, OpenCode and OpenClaw one local CCR endpoint, then
lets you manage upstream providers, model routes, endpoint checks and logs from
the UI.

```text
Claude Code / Codex / OpenCode / OpenClaw
        -> CCR desktop app + local server
        -> your provider routes
        -> OpenAI / Anthropic / OpenRouter / DeepSeek / Groq / compatible APIs
```

## What It Does

- Connect multiple coding clients to one local router.
- Add and test upstream providers from the desktop UI.
- Manage Route Pool candidates and priority from the UI.
- Start and stop the local CCR server from the Status tab.
- Connect or restore supported client apps from UI controls.
- Inspect status, diagnostics and logs without editing config files.

CCR is UI-first. Normal setup and daily operation should happen in the desktop
app.

## Current Status

This is the Rust implementation of CCR. The desktop UI, local server, Route Pool
runtime, client connection flows, endpoint tests and logs are in place.

Current development focus:

- complete UI workflows;
- reliable log writing and logs query;
- request/response observability from real client traffic.

## Quick Start

The current development version is started from source.

### 1. Install Dependencies

Ubuntu/Linux:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev libgtk-3-dev libxdo-dev libayatana-appindicator3-dev
```

Rust:

```sh
rustup default stable
```

### 2. Start The Desktop App

```sh
cargo run --bin ccr-ui
```

### 3. Set Up From The UI

In the desktop app:

1. Add a provider.
2. Test its endpoint.
3. Add one or more Route Pool candidates.
4. Start the local CCR server from Status.
5. Connect Claude Code, Codex, OpenCode or OpenClaw from the client controls.
6. Use Logs and Status to inspect behavior.

## Linux / WSL

If the UI starts with GTK, Wayland, EGL or glutin errors in WSL, use the X11 and
software-rendering fallback:

```sh
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET CCR_DISABLE_TRAY=1 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui
```

This disables the tray, avoids the Wayland startup path and asks Mesa to use
software rendering.

## Development

Common checks:

```sh
cargo fmt --check
cargo test --workspace
./scripts/check-docs-structure.sh
```

Useful docs:

- `ARCHITECTURE.md` - crate boundaries and runtime paths.
- `docs/index.md` - documentation entry point.
- `docs/long-term-roadmap.md` - product and engineering direction.
- `docs/release.md` - platform prerequisites and release notes.
- `docs/exec-plans/index.md` - execution plan workflow.

## License

No license file is currently present in this repository.
