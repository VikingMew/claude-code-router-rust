# Phase 63 - Status Page Information Architecture

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

重组 Status 页面，解决当前多个 “status” 混在一起导致用户无法判断真实状态的问题。

这个 phase 的核心是把不同层级的状态分清楚：

- Server process：本机 `ccr-server` 进程是否运行。
- Server health：HTTP `/health` 是否可访问。
- Routing configuration：failover / provider pool 是否在配置中启用。
- Routing runtime：provider pool 的运行时失败次数、ban 状态、最后错误。
- Last operation：用户点击 Start / Stop / Activate / Deactivate 后的一次性操作结果。
- Client injection：Claude / Codex 等 client 是否已经接入 CCR。

Status 页面应该默认是用户打开后能立刻判断“现在能不能用、哪里不能用”的第一屏，而不是一组含义相近但来源不同的 status 文案。

## 背景

当前 Status 页面会显示类似内容：

```text
Server Status
Status: ● Running (PID 18560)
Address: http://127.0.0.1:3456

✓ Server started (PID 18560)

Failover Status
Not configured

Provider Pool Status
Not configured
Runtime status unavailable: HTTP 401 Unauthorized

Health Check

✓ Server is healthy
```

这里有几个问题：

1. `Server Status` 是进程状态，但 `✓ Server started` 是操作反馈，两者混在同一区块。
2. `Failover Status` 是静态配置状态。
3. `Provider Pool Status` 先显示静态配置状态，又继续请求运行时 API。
4. Provider Pool 明明 `Not configured`，却还显示 `Runtime status unavailable: HTTP 401 Unauthorized`。
5. `Health Check` 只说明 HTTP server 活着，不代表 provider pool、routing、provider 可用。
6. 多个区块都叫 status，用户无法区分“配置没有启用”和“运行时读取失败”。

相关当前实现：

- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- `ccr-rust/crates/ccr-app-core/src/status.rs`
- `ccr-rust/crates/ccr-server/src/main.rs`

## 当前行为问题

### 1. Provider Pool 未配置时仍请求 runtime API

当前 UI 在 server running 时会调用：

```text
GET /api/route-pool/status
```

即使本地配置中 `RoutePool` 缺失或 disabled，也会请求 runtime API。

这会导致：

- 未配置状态下出现 runtime 错误。
- 如果 UI 读到的 API key 和 server 运行时 API key 不一致，会显示 `HTTP 401 Unauthorized`。
- 用户会误以为 provider pool 有运行时故障。

正确行为：

- `RoutePool` 缺失时，不请求 runtime API。
- `RoutePool.enabled = false` 时，不请求 runtime API。
- 只有 `RoutePool.enabled = true` 且 server running 时，才请求 runtime API。

### 2. Health Check 的语义太宽

`/health` 只证明 HTTP server 可访问。

它不代表：

- default route 存在。
- provider API key 正确。
- provider endpoint 可用。
- failover / provider pool 可用。
- Claude/Codex/OpenCode/OpenClaw 已接入。

UI 文案需要明确这是 server HTTP health，而不是整体系统健康。

### 3. 操作反馈和长期状态混在一起

`✓ Server started (PID ...)` 是一次操作结果。

它应该显示在 `Last operation` 或当前区块的短暂 feedback 行，而不应该让用户误认为这是另一个状态源。

### 4. 静态配置和运行时状态混在一起

Failover / Provider Pool 有两层信息：

- 配置层：是否配置、是否 enabled、有几个 candidate。
- 运行时层：失败次数、ban 状态、last error、last success。

配置层可以从磁盘 config 直接读。

运行时层只能从正在运行的 server 读取，并且必须和当前 server 进程的 auth/config 对齐。

UI 需要显式区分这两层。

## 设计方向

### 1. 页面结构

Status 页面改成以下结构：

```text
Status

Server
  Process: Running / Stopped / Stale PID
  Address: http://127.0.0.1:<port>
  HTTP Health: Healthy / Failed / Not checked / Server stopped
  [Start Server] [Stop Server] [Check Health]
  Last operation: ...

Routing
  Default route: ...
  Failover config: Not configured / Disabled / Enabled
  Provider pool config: Not configured / Disabled / Enabled
  Provider pool runtime: Not loaded / Unavailable / Ready

Client Injection
  Claude Code: ...
  Codex: ...
```

后续 OpenCode/OpenClaw phase 完成后，在 `Client Injection` 下继续添加：

```text
  OpenCode: ...
  OpenClaw: ...
```

### 2. Server 区块

Server 区块只显示 server 进程和 HTTP server 可达性：

- Process:
  - `Running (PID <pid>)`
  - `Stopped`
  - `Stopped (stale PID <pid>)`
- Address:
  - `http://127.0.0.1:<port>`
- HTTP Health:
  - `Healthy`
  - `Failed: <error>`
  - `Not checked`
  - `Unavailable while server is stopped`
- Last operation:
  - `Started server (PID <pid>)`
  - `Stopped server`
  - `Start failed: <error>`

`Health Check` 不再作为独立大区块，避免它看起来和 `Server Status` 是两个同级状态。

### 3. Routing 区块

Routing 区块负责 routing config 和 routing runtime。

显示：

- Default route:
  - 当前 `Router.default`
- Failover config:
  - `Not configured`
  - `Disabled (<n> candidates)`
  - `Enabled (<n> candidates, trigger <trigger>)`
- Provider pool config:
  - `Not configured`
  - `Disabled (<n> candidates)`
  - `Enabled (<n> candidates, <threshold> failures, <ban>s ban)`
- Provider pool runtime:
  - `Not applicable`：provider pool 未配置或 disabled
  - `Unavailable while server is stopped`
  - `Unauthorized: UI config API key does not match running server`
  - `Ready: no failures recorded`
  - route list with ready/banned/failure details

关键规则：

- Provider pool config 未配置：不请求 runtime API。
- Provider pool disabled：不请求 runtime API。
- Server stopped：不请求 runtime API。
- 只有 provider pool enabled 且 server running 时，请求 runtime API。

### 4. Runtime auth 错误文案

当 `/api/route-pool/status` 返回 401 时，不显示裸 `HTTP 401 Unauthorized`。

建议文案：

```text
Provider pool runtime unavailable: unauthorized. The UI config API key does not match the running server.
```

如果可能，补充行动建议：

```text
Restart the server after saving config, or refresh status after the server reloads.
```

但不应该把它显示在 provider pool `Not configured` 后面。

### 5. Client Injection 区块

Claude / Codex 配置切换仍然保留，但移动到独立 `Client Injection` 区块。

文案保持语义明确：

- `Claude Code: Using CCR router`
- `Claude Code: Original configuration`
- `Claude Code: Drifted from CCR`
- `Codex: Using CCR router`
- `Codex: Original configuration`
- `Codex: Drifted from CCR`

按钮：

- `Activate CCR for Claude Code`
- `Deactivate CCR for Claude Code`
- `Activate CCR for Codex`
- `Deactivate CCR for Codex`

按钮启用规则保持当前逻辑：

- Activate 需要 server running。
- Deactivate 不需要 server running。

## 非目标

- 不改变 provider pool / circuit breaker 的核心算法。
- 不改变 failover 的执行逻辑。
- 不改变 server `/health` 语义。
- 不引入新的 provider 测速功能。
- 不实现 OpenCode/OpenClaw client 注入；它们由 phase 61 / 62 处理。
- 不把 CLI status 作为本 phase 的主要边界；本项目方向是 UI 优先。

## 预计修改文件

- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- `ccr-rust/crates/ccr-app-core/src/status.rs`

可能需要：

- `ccr-rust/crates/ccr-ui/src/app.rs`
- `ccr-rust/crates/ccr-server/src/main.rs`（如果需要调整 runtime status 返回体，但优先只改 UI）
- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`

## 规划的测试用例

### Status model

- server running snapshot 显示 process running 和 address。
- stale PID snapshot 显示 stale PID，不显示健康。
- server stopped snapshot 显示 health unavailable。
- last operation message 不改变 server process 状态。
- health success 只更新 HTTP Health 字段。
- health failure 只更新 HTTP Health 字段。

### Routing config

- `Failover = None` 显示 `Not configured`。
- `Failover.enabled = false` 显示 disabled 和 configured candidate 数。
- `Failover.enabled = true` 显示 enabled、trigger、enabled candidate 数。
- `RoutePool = None` 显示 `Not configured`，不触发 runtime fetch。
- `RoutePool.enabled = false` 显示 disabled，不触发 runtime fetch。
- `RoutePool.enabled = true` 且 server stopped，不触发 runtime fetch。
- `RoutePool.enabled = true` 且 server running，触发 runtime fetch。

### Provider pool runtime

- runtime 返回空 routes 时显示 no failures recorded。
- runtime route ready 时显示 failures / last success。
- runtime route banned 时显示 banned until / last error。
- runtime HTTP 401 显示明确 unauthorized 文案。
- runtime HTTP 500 显示 runtime unavailable，但不覆盖 config 状态。
- runtime request timeout 显示 timeout/error，但不影响 server process 状态。

### Client injection

- Claude active/current 显示 using CCR router。
- Claude inactive 显示 original configuration。
- Claude drifted 显示 drifted。
- Codex active/current 显示 using CCR router。
- Codex inactive 显示 original configuration。
- Codex drifted 显示 drifted。
- server stopped 时 Activate 禁用、Deactivate 保持可用。

### UI 行为

- 打开 Status 页面时立即刷新 process/config/injection snapshot。
- `Refresh Status` 刷新 process/config/injection，并清空 provider pool runtime stale cache。
- `Check Health` 只刷新 HTTP Health。
- provider pool 未配置时页面不出现 `Runtime status unavailable: HTTP 401 Unauthorized`。
- provider pool disabled 时页面不出现 runtime error。
- operation success message 显示在 `Last operation`，不作为独立 status 区块。

### 验证命令

- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-ui --bin ccr-server`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- Status 页面不再出现多个语义不清的 “Status” 区块。
- Server process、HTTP health、routing config、routing runtime、client injection 分区清晰。
- Provider pool 未配置或 disabled 时不会请求 runtime API。
- Provider pool 未配置或 disabled 时不会显示 runtime 401。
- Provider pool runtime 401 有明确原因文案。
- Health Check 不再被误读成整体系统可用性。
- Last operation 不再和长期状态混在一起。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 本次优先完成 Status 页面信息架构和 provider pool runtime 请求条件；没有为 `status_tab.rs` 新增细粒度 UI 单测，现有 snapshot/activation 测试保持通过。
- `HTTP Health` 仍沿用现有手动 `Check Health` 行为，没有改成打开页面自动探测。
