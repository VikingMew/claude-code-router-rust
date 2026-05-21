# TD-009 - UI Core Decoupling Follow-Up

**状态：** resolved
**优先级：** P1
**关联技术债：** TD-009
**最后更新：** 2026-05-21

## 目标

继续把 `ccr-ui` 中的业务逻辑下沉到 `ccr-app-core`，让 UI 层主要负责渲染和派发 action。

优先迁移：

- server lifecycle snapshot 和 operation result。
- client injection snapshot aggregation。
- activation/deactivation service wrappers。
- status tab view model。

## 非目标

- 不重写整个 egui app。
- 不引入大型 frontend framework。
- 不要求一次完成所有 tabs。
- 不改变现有用户可见行为。

## 背景

Phase 52 已完成第一轮 UI/core 解耦，但状态页、设置页和部分操作仍直接调用 CLI helper 或本地进程逻辑。业务行为散落在 egui tab 中会降低测试性。

## 设计方向

新增或扩展 app-core modules：

- `status`
- `server_lifecycle`
- `client_injection`
- `actions`

把 UI 所需数据整理为 view model：

- server status
- route pool runtime summary
- Claude/Codex/OpenCode/OpenClaw status
- operation availability
- warning/error messages

UI 只调用：

- `load_status_snapshot`
- `perform_status_action`
- `refresh_after_action`

## 修改文件

- `crates/ccr-app-core/src/status.rs`
- `crates/ccr-app-core/src/lib.rs`
- `crates/ccr-ui/src/status_tab.rs`
- `crates/ccr-ui/src/settings_tab.rs`
- `crates/ccr-cli/src/*_config.rs` as needed for shared service boundaries
- `docs/FRONTEND.md`
- `docs/exec-plans/completed/`

## 验收测试

```sh
cargo test --package ccr-app-core --lib
cargo test --package ccr-ui
cargo test --workspace
```

测试覆盖：

- server stopped/running/stale pid snapshot。
- client active/inactive/current-port-mismatch snapshot。
- action enablement rules。
- additive clients 不误用 overwrite restore 语义。

## 决策日志

- 2026-05-07：先从 Status tab 开始，因为它聚合 server、client 和 route runtime 状态，是业务逻辑最集中的 UI。

## 完成记录

完成：2026-05-07。

- 将 PID 路径、PID 读写、进程存活检查、server snapshot、server start/stop、health check 和 server executable resolution 下沉到 `ccr_app_core::status`。
- `ccr-cli` 通过 re-export 继续提供兼容入口，避免重复实现。
- Status tab 不再直接使用 `Command`、PID helper 或平台 signal 逻辑。
- `AdditiveClientSnapshot` 保持在 app-core 中，由 UI 只负责展示和按钮 enablement。
- 验证：`cargo test --package ccr-app-core --lib`、`cargo test --package ccr-ui`、`cargo test --workspace` 通过。

## 完成偏差

- 2026-05-07 原完成范围未覆盖完整 Status tab view model/action reducer。
- 2026-05-21 CCR-16 已迁移 Status snapshot 聚合、Route Pool config summary 和 runtime fetch bundle 到 `ccr-app-core`，并让 Status tab 通过后台任务刷新 server/client snapshots、health check、Route Pool runtime、runtime metrics 和 TTFT metrics。
- Status tab 仍直接调度具体按钮动作，但动作在后台线程执行，egui render path 只消费 cached state 和轮询 in-memory result。
