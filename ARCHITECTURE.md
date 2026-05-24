# Architecture

**状态：** 长期架构地图
**最后验证：** 2026-05-21

## Product Shape

`ccr-rust` is a Rust workspace for a UI-first local desktop router. Clients such as Claude Code, Codex, OpenCode, OpenClaw and Hermes Agent connect to a local CCR server. The server selects an upstream route from Route Pool, rewrites request shape when needed, sends the request to the configured provider and returns the response.

Default mode is through-CCR:

```text
Client app -> local CCR server -> Route Pool -> provider/model endpoint
```

Client config injection only points the client to the local CCR server. It should not need to know whether the upstream provider is Anthropic, OpenAI, OpenRouter, DeepSeek, Groq or another compatible service.

## Workspace Crates

### `ccr-types`

Shared config and API data types.

Primary objects:

- `Config`
- `Provider`
- `RouterConfig`
- `RoutePoolConfig`
- `AppSettings`
- client request/response structs

This crate should stay mostly dependency-light and should not own runtime behavior.

### `ccr-config`

Configuration loading, saving, backup and reload support.

Current default config path:

```text
~/.claude-code-router/config.json
```

Responsibilities:

- JSON/JSON5 config parsing.
- Environment variable interpolation.
- Unknown top-level field preservation where supported.
- Backup creation before writes.
- reloadable config wrapper.

### `ccr-router`

Routing helpers and token counting.

Responsibilities:

- Resolve provider from a route string.
- Count request tokens using configured tokenizer backend.

Runtime routing invariant: Route Pool is the only primary routing model. Server upstream attempts come from enabled Route Pool candidates. Do not restore `Router.default`, scenario route fallback, project/custom router overrides or failover fallback.

### `ccr-transformer`

Protocol and provider request transformers.

Responsibilities:

- Register built-in transformers.
- Apply configured transformer chains.
- Adapt request fields for OpenAI-compatible, Anthropic-compatible and other provider-specific behaviors.

Transformers should be deterministic and covered with small input/output tests.

### `ccr-server`

Local HTTP runtime.

Key endpoints:

- `/health`
- `/v1/messages`
- `/v1/responses`
- `/v1/messages/count_tokens`
- `/api/config`
- `/api/logs`
- `/api/route-pool/status`
- `/api/presets`

Responsibilities:

- Load config and expose reloadable state.
- Authorize local/config API access.
- Select Route Pool attempts.
- Build upstream requests according to provider API kind.
- Apply transformers.
- Retry/fail over through Route Pool candidates.
- Record server, upstream and route-pool logs.

### `ccr-cli`

Command-line helper and client config injection.

Responsibilities:

- Start/stop/restart/status helpers.
- Claude Code config activate/deactivate/status.
- Codex config activate/deactivate/status.
- OpenCode additive provider write/remove/status.
- OpenClaw additive provider write/remove/status.
- Hermes additive custom provider write/remove/status.
- Preset commands and model helper commands.

CLI should not become the primary product surface for P0/P1 workflows.

### `ccr-ui`

Native egui desktop application.

Responsibilities:

- Status tab and server lifecycle controls.
- Provider, router, transformer, preset, logs, token counter and settings UI.
- System tray integration.
- Dispatch user actions to app-core/CLI/server helpers.

Long-term direction: keep business logic out of egui rendering code and move testable behavior to `ccr-app-core`.

### `ccr-app-core`

UI-independent application logic.

Responsibilities:

- Status snapshots.
- Endpoint test request/result logic.
- Settings state helpers.
- Client config injection helpers.
- Logging helpers.
- Provider kind resolution support.

This is the preferred place for logic that needs unit tests but does not belong in server runtime.

### `ccr-preset`

Preset/profile management.

Responsibilities:

- Built-in default profiles.
- Preset listing, loading, export and delete.
- Future preset install/merge behavior.

Presets should write Route Pool, provider kind and client-visible mapping deliberately.

### `ccr-sse`

SSE parsing and rewriting support.

Responsibilities:

- Parse streaming events.
- Rewrite events when tool interception/continuation behavior is needed.

### `ccr-agent`

Agent extensions for request handling.

Current example:

- Image agent detection and tool interception path.

### `ccr-plugin`

Archived plugin experiments and supporting abstractions.

This crate is inactive experimental code and is intentionally not an active
workspace member. It is not a runtime extension boundary for the desktop app,
server, CLI, app-core, routing, transformer, SSE or agent paths.

## Runtime Request Paths

### Claude Code / Anthropic Messages

```text
Claude Code
  -> local CCR /v1/messages
  -> parse MessagesRequest
  -> optional agent detection
  -> Route Pool candidate list
  -> provider lookup
  -> upstream request build
  -> transformer chain
  -> upstream provider
  -> stream or JSON passthrough
```

### Codex / OpenAI Responses

```text
Codex
  -> local CCR /v1/responses
  -> parse request body
  -> Route Pool candidate list
  -> provider lookup
  -> Responses or Chat/Anthropic-compatible body conversion
  -> upstream provider
  -> stream or JSON passthrough
```

## Client Injection Paths

### Claude Code

Writes Claude Code settings/env so Claude Code uses local CCR.

Boundary:

- client-visible model mapping is separate from upstream Route Pool model.
- provider kind must not drive default Claude Code injection.

### Codex

Writes Codex config so Codex uses local CCR as a model provider.

Boundary:

- Codex `/v1/responses` support is handled by server.
- upstream provider selection remains Route Pool behavior.

### OpenCode, OpenClaw and Hermes

Additive provider configuration.

Boundary:

- add/update/remove only CCR-managed provider fragments.
- do not overwrite unrelated user provider, plugin or MCP config.
- Hermes writes a `custom_providers` entry named `ccr` and points `model.provider` at `custom:ccr`; it does not edit `.env` secrets.

## Core Invariants

- UI first, CLI auxiliary.
- Route Pool is the primary routing model.
- through-CCR is default.
- Provider kind affects upstream behavior, not default client injection.
- Config writes should preserve unrelated user-owned data where possible.
- Logs and metrics should be readable by both users and agents.
- Long-term docs should be short, indexed and mechanically checkable.
