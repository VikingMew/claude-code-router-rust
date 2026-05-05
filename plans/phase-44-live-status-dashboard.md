# Phase 44 — Live Status Dashboard

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

让 `ccr-ui` 的 Status 页面成为桌面应用的默认工作台，并确保页面上的所有状态都是实时状态。

Status 页面不能依赖旧缓存或上一次按钮操作结果。用户点开 Status 页面时，server 状态、Claude Code 注入状态、Codex 注入状态都必须立刻读取当前真实状态。

## 背景

当前 Status 页面已经能展示 server、Claude Code config、Codex config，并提供 start/stop/activate/deactivate 操作。但这些展示和按钮状态需要进一步收敛为统一的实时快照：

- 页面打开时立即刷新
- 手动刷新按钮保留
- 操作完成后立即刷新
- 按钮是否可点击由当前真实状态决定
- 程序启动后默认进入 Status 页面
- 程序启动时默认启动 CCR server

## 完成项

- [x] `ccr-ui` 启动后默认显示 `Status` tab
- [x] `ccr-ui` 启动时默认尝试启动 CCR server
- [x] 如果 server 已运行，不重复启动
- [x] 如果 server 启动失败，在 Status 页面显示错误
- [x] Status 页面打开/显示时立即刷新 server 状态
- [x] Status 页面打开/显示时立即刷新 Claude Code 注入状态
- [x] Status 页面打开/显示时立即刷新 Codex 注入状态
- [x] 保留 `Refresh Status` 按钮
- [x] `Refresh Status` 刷新所有状态，而不是只刷新 health
- [x] start/stop/activate/deactivate 操作完成后立即刷新所有状态
- [x] 根据实时状态启用或禁用按钮
- [x] 页面展示和按钮判断使用同一份状态快照
- [x] 补充 UI 状态 helper 的单元测试

## 实时状态定义

### Server 状态

每次刷新时读取：

- PID 文件路径：`~/.claude-code-router/.ccr.pid`
- PID 文件是否存在
- PID 对应进程是否仍然存活
- 当前配置端口，默认 `3456`

状态结果：

- `Running { pid, port }`
- `Stopped`
- `StalePid { pid }`
- `Error { message }`

### Claude Code 注入状态

每次刷新时调用现有 activation status 逻辑：

- 已注入：存在 Claude config backup
- 未注入：没有 backup
- 显示 backup path
- 显示 backup modified time

### Codex 注入状态

每次刷新时调用现有 Codex activation status 逻辑：

- 已注入：存在 Codex config backup 或 missing marker
- 未注入：没有 backup 和 marker
- 显示 backup path
- 显示 backup modified time

## 按钮规则

### Server 操作

- server running：
  - `Start Server` 禁用
  - `Stop Server` 启用
- server stopped：
  - `Start Server` 启用
  - `Stop Server` 禁用
- stale PID：
  - `Start Server` 启用
  - `Stop Server` 禁用或显示清理提示

### Claude Code 注入

- server stopped：
  - `Activate CCR for Claude Code` 禁用，并提示需要先启动 server
- Claude Code 已注入：
  - `Activate CCR for Claude Code` 禁用
  - `Deactivate CCR for Claude Code` 启用
- Claude Code 未注入：
  - `Activate CCR for Claude Code` 启用
  - `Deactivate CCR for Claude Code` 禁用

### Codex 注入

- server stopped：
  - `Activate CCR for Codex` 禁用，并提示需要先启动 server
- Codex 已注入：
  - `Activate CCR for Codex` 禁用
  - `Deactivate CCR for Codex` 启用
- Codex 未注入：
  - `Activate CCR for Codex` 启用
  - `Deactivate CCR for Codex` 禁用

## UI 行为

Status 页面顶部提供：

```text
Refresh Status
```

点击后刷新：

- server runtime status
- health check status
- Claude Code injection status
- Codex injection status
- button enabled/disabled state

`Health Check` 可以保留为单独请求，但 `Refresh Status` 应该负责所有状态同步。

## 实现方向

### 1. 增加状态快照

在 `ccr-ui/src/status_tab.rs` 中新增内部状态结构：

```rust
struct StatusSnapshot {
    server: ServerSnapshot,
    claude: InjectionSnapshot,
    codex: InjectionSnapshot,
}
```

渲染 UI 时只读取 `StatusSnapshot`，避免展示状态和按钮判断分散调用不同函数。

### 2. 增加统一刷新函数

```rust
fn refresh_status(&mut self)
```

该函数负责读取 PID、config、Claude backup、Codex backup，并更新 `StatusSnapshot`。

### 3. 页面显示时刷新

Status tab 第一次显示时立即调用 `refresh_status()`。

如果从其他 tab 切换回 Status tab，也要刷新。

### 4. 操作后刷新

以下操作成功或失败后都调用 `refresh_status()`：

- `start_server`
- `stop_server`
- `activate_claude_config`
- `deactivate_claude_config`
- `activate_codex_config`
- `deactivate_codex_config`

### 5. 应用启动默认行为

在 `ccr-ui/src/app.rs`：

- 默认 `tab = Tab::Status`
- 初始化后尝试启动 server
- 如果已运行则跳过
- 启动结果写入 Status tab 的 operation status

## 修改文件

- `ccr-rust/crates/ccr-ui/src/app.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`

## 完成说明

- `StatusSnapshot` 统一承载 server、Claude Code、Codex 的当前状态。
- Status tab 每次显示都会重新读取快照，因此从其他 tab 切回 Status tab 时会立刻更新。
- `Refresh Status` 会刷新快照并执行 health check。
- 启动、停止、注入、恢复操作结束后都会重新读取快照。
- `Start Server` / `Stop Server` / `Activate` / `Deactivate` 的可用性由当前快照决定。
- `ccr-ui` 初始化时默认进入 Status tab，并尝试启动 server。
- server executable 支持开发环境 sibling binary 和打包后 `Resources/bin/ccr-server`。

## 验证

```bash
cargo build --package ccr-ui
cargo test --package ccr-ui
```

## 验收标准

- 启动 `ccr-ui` 后第一个页面是 Status
- 启动 `ccr-ui` 后默认尝试启动 CCR server
- server 已运行时不会重复启动
- Status 页面显示时能看到当前真实 server 状态
- Status 页面显示时能看到当前真实 Claude Code 注入状态
- Status 页面显示时能看到当前真实 Codex 注入状态
- 点击 `Refresh Status` 后所有状态同步更新
- server running 时不能点击 `Start Server`
- server stopped 时不能点击 `Stop Server`
- 已注入时不能重复 activate
- 未注入时不能 deactivate
- server 未运行时不能 activate 注入
- 操作完成后页面状态立即更新
