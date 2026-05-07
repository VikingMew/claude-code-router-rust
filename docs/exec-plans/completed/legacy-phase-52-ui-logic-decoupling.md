# Phase 52 - UI Logic Decoupling

**状态：** ✅ 已完成（第一轮 UI/core 解耦完成）  
**优先级：** P0

## 目标

把 UI 展示和业务逻辑解耦，提高 UI 相关功能的可测试性。

当前 `ccr-ui` 的 egui 代码同时承担展示、状态转换、配置读写、平台调用和网络测速。这会导致 UI crate 覆盖率很低，也让业务行为难以通过单元测试验证。本 phase 要把可测试逻辑下沉到 UI-independent core 层，让 UI 只负责渲染和派发 action。

本 phase 遵循 UI 优先：用户入口仍然是桌面 UI，但业务逻辑不能继续写在 egui 渲染函数里。

## 背景

当前覆盖率中 `ccr-ui` 大量文件接近 0% 或很低：

- `ccr-ui/src/app.rs`
- `ccr-ui/src/config_tab.rs`
- `ccr-ui/src/endpoint_test_tab.rs`
- `ccr-ui/src/settings_tab.rs`
- `ccr-ui/src/status_tab.rs`
- `ccr-ui/src/token_counter_tab.rs`
- `ccr-ui/src/tray_manager.rs`

根因不是 UI 一定不可测，而是业务逻辑和 egui widget 回调混在一起。正确方向是把逻辑抽成 state / action / reducer / service，UI 层只消费 view model。

## 完成项

- [x] 新增 UI-independent core crate 或 core module
- [x] 从 `ccr-ui` 中抽出 Settings 业务逻辑
- [x] 从 `ccr-ui` 中抽出 Failover 排序逻辑
- [x] 从 `ccr-ui` 中抽出 Endpoint candidates 管理逻辑
- [x] 从 `ccr-ui` 中抽出 Endpoint test result 排序与 apply-to-failover 逻辑
- [x] 从 `ccr-ui` 中抽出 Status snapshot 计算逻辑
- [x] 抽象 AutoLaunch platform adapter，UI 只调用 trait/service
- [ ] UI tab 渲染函数只保留 view rendering 和 action dispatch
- [x] 给 core 层补单元测试
- [x] UI crate 中保留少量 smoke tests
- [x] 更新 phase 文档偏差和覆盖率结果

## 推荐结构

优先新增一个小的 core crate：

```text
ccr-rust/crates/ccr-app-core/
  src/settings.rs
  src/failover.rs
  src/endpoint.rs
  src/status.rs
  src/platform.rs
  src/lib.rs
```

依赖方向：

```text
ccr-types
ccr-config
ccr-app-core
ccr-ui
```

原则：

- `ccr-app-core` 不依赖 `egui`
- `ccr-app-core` 不依赖 `ccr-ui`
- `ccr-ui` 可以依赖 `ccr-app-core`
- 后续 CLI 如需要，也依赖 `ccr-app-core`，而不是复制 UI 逻辑

## State / Action 方向

Settings 示例：

```rust
pub struct SettingsState {
    pub config: Config,
    pub restart_required: bool,
    pub message: Option<String>,
}

pub enum SettingsAction {
    SetHost(String),
    SetPort(u16),
    SetLogLevel(String),
    SetServerAutoStart(bool),
    SetAutoLaunch(bool),
}
```

Failover 示例：

```rust
pub enum FailoverAction {
    Enable(bool),
    AddRoute(String),
    RemoveRoute(String),
    MoveUp(String),
    MoveDown(String),
}
```

Endpoint 示例：

```rust
pub enum EndpointAction {
    AddCandidate { provider: String, url: String },
    RemoveCandidate { provider: String, url: String },
    ApplyResultsToFailover { provider: String },
}
```

## UI 改造规则

- egui 回调里不直接改复杂业务结构
- egui 回调只创建 action
- action 交给 core state/controller 执行
- core 返回新的 view model 或状态
- 文件读写、平台操作、网络测速通过 trait 注入，测试里用 mock
- UI tab 不直接依赖 CLI helper

## 规划的测试用例

- Settings reducer：修改 port 后 `restart_required = true`。
- Settings reducer：修改 log level 后保存到 AppSettings。
- Settings reducer：修改 client config path 不触发注入。
- Settings service：保存配置时保留未知字段。
- Startup service：auto launch unsupported 平台返回可展示状态。
- Failover reducer：AddRoute 会追加到末尾并生成连续 priority。
- Failover reducer：MoveUp 第一项不变。
- Failover reducer：MoveDown 最后一项不变。
- Failover reducer：RemoveRoute 后 priority 重新归一化。
- Endpoint reducer：非法 URL 返回 validation error。
- Endpoint reducer：重复 endpoint 不重复插入。
- Endpoint reducer：RemoveCandidate 删除目标 URL。
- Endpoint result：成功 endpoint 按 latency 升序，失败排后。
- Endpoint apply：apply-to-failover 需要确认状态。
- Endpoint apply：确认后写入 provider/model route。
- Status snapshot：server running/stopped/stale pid 映射正确。
- Status snapshot：Claude/Codex injection 状态映射正确。
- UI smoke：SettingsTab 创建后默认分组为 Startup。
- UI smoke：EndpointTestTab 创建后没有结果且 selected provider 为 0。

## 预留的开发偏差

- 本轮新增 `ccr-app-core`，并将 Settings、Failover、Endpoint、Status snapshot、AutoLaunch 平台逻辑下沉到该 crate。`ccr-ui` 已改为消费这些 core state/service。
- Status 页的 snapshot 类型已使用 `ccr-app-core::status`，但 `activate/deactivate/start/stop` 等实际操作仍暂时调用 `ccr-cli` helper 和本地进程逻辑。后续应把 Claude/Codex 注入与 server lifecycle operation 继续迁移到 `ccr-app-core`，让 UI 不再直接依赖 CLI helper。
- UI tab 仍是 egui 回调直接调用 state 方法，未引入完整 `Action` enum/reducer。当前已满足“展示与业务状态逻辑初步分离”，但严格的 action dispatch 结构留到下一轮。
- Endpoint 真实网络测速分支未通过本地 mock server 覆盖，因为当前执行环境禁止监听本地端口；本轮只覆盖 invalid URL、candidate 管理、排序和 apply-to-failover 等纯逻辑。
- `cargo llvm-cov --workspace --lib --summary-only` 的 line coverage 为 `80.25%`。`cargo llvm-cov --workspace --exclude ccr-ui --summary-only` 为 `71.26%`，低值主要来自 binary 入口，而非本轮新增 core 逻辑。

## 验证

```bash
cargo test --package ccr-app-core
cargo test --package ccr-ui
cargo test --workspace
cargo llvm-cov --workspace --lib --summary-only
```

验证结果：

- `cargo test --package ccr-app-core --package ccr-ui` 通过
- `cargo test --workspace` 通过
- `cargo build --package ccr-ui` 通过
- `cargo build --package ccr-server` 通过
- `cargo llvm-cov --workspace --lib --summary-only`：Lines `80.25%`

## 验收标准

- UI 业务逻辑从 egui 渲染函数中拆出
- 新 core 层不依赖 egui
- Settings、Failover、Endpoint、Status 的核心状态转换都有单元测试
- `ccr-ui` 仍然是用户主入口
- `ccr-ui` 不再直接依赖 CLI helper 作为业务逻辑来源
- 后续 CLI 可以复用同一套 core，而不是复制 UI 逻辑
