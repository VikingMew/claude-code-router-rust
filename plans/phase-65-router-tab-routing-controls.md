# Phase 65 - Router Tab Routing Controls

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

新增独立 `Router` 页面 tab，把 routing 决策相关配置从 provider/config/settings 页面移动到这个页面：

- `Router.default`
- Failover
- Provider Pool

Provider 页面只负责 provider 本体配置：name、endpoint、API key、models、transformers、provider API kind、inline endpoint test。

Router 页面负责“请求应该走哪个 route、失败后怎么切、provider pool 怎么排序和 ban”。

## 背景

当前 UI 中 routing 相关能力分散：

- `Router.default` 在 provider config 页面顶部：
  - `ccr-rust/crates/ccr-ui/src/config_tab.rs`
  - `ccr-rust/crates/ccr-ui/src/config_tab_new.rs`
- Failover 在 Settings 页面：
  - `ccr-rust/crates/ccr-ui/src/settings_tab.rs`
  - `ccr-rust/crates/ccr-app-core/src/settings.rs`
- Provider Pool 也在 Settings 页面：
  - `ccr-rust/crates/ccr-ui/src/settings_tab.rs`
  - `ccr-rust/crates/ccr-app-core/src/settings.rs`
- Tray 里也可以切 default route：
  - `ccr-rust/crates/ccr-ui/src/tray_manager.rs`

这会导致用户心智混乱：

1. Provider 页面上方的 `Default` 看起来像 provider 的字段，但它其实是 routing default route。
2. Failover 和 Provider Pool 不属于 app settings，它们是 routing policy。
3. Provider Pool 已经是主要 route selection 能力，但还放在 Settings，发现成本太高。
4. Default / Failover / Provider Pool 三者互相影响，却不在同一个页面里。

## Default Route 是否还应该存在

应该存在，但需要改名和重新定位。

原因：

- `Router.default` 是 server 路由的基础入口。
- 当 Provider Pool 未启用时，它决定默认 route。
- 当入站请求没有明确 provider/model route 时，它提供默认 provider 或默认 `provider,model`。
- Claude/Codex/OpenCode/OpenClaw client 注入时，也需要从 default route 推导默认模型。
- Provider Pool 启用时，default route 仍可作为 fallback / compatibility route，避免旧配置失效。

但 UI 上不应该继续叫模糊的 `Default`。

建议文案：

```text
Primary Route
```

或：

```text
Default Route
```

并说明它是 routing default，不是某个 provider 的默认 endpoint。

推荐本 phase 使用：

```text
Primary Route
```

底层仍写入 `config.router.default`，保持配置兼容。

## 设计方向

### 1. 新增 Router tab

主导航增加：

```text
Router
```

建议页面顺序：

```text
Status | Config | Router | Presets | Logs | Transformers | Token Counter | Settings
```

如果 `Config` 实际只剩 provider 配置，后续可以改名为 `Providers`，但本 phase 不强制。

Router tab 页面结构：

```text
Router

Primary Route
  Route: [provider] or [provider,model]
  Resolved provider: ...
  Resolved model behavior: ...

Provider Pool
  Enable provider pool
  Failure threshold
  Ban seconds
  Ordered candidates

Failover
  Enable failover
  Trigger
  Ordered routes
```

### 2. Primary Route

Primary Route 编辑 `config.router.default`。

UI 行为：

- 支持直接输入 route。
- 支持从 provider/model 组合选择生成 route。
- provider-only route 合法。
- `provider,model` route 合法。
- 空 route 不建议保存，保存前显示 validation warning。
- route provider 不存在时显示 warning，但不强制阻止保存，避免破坏手写高级配置。

显示解释：

- provider-only：`Uses inbound model name with this provider.`
- provider,model：`Pins this route to the selected model.`

不要在 Provider 页面继续显示 `Default:` 输入框。

### 3. Provider Pool

Provider Pool 从 Settings 页面迁移到 Router tab。

Router tab 中显示和编辑：

- enabled
- failure threshold
- ban seconds
- ordered candidates
- add candidate
- remove candidate
- enable/disable candidate
- move up/down

候选 route 支持：

- provider-only
- provider,model

默认建议：

- 新增 candidate 时优先 provider-only route。
- UI 文案明确 provider-only route 会保留入站模型名。

和 Primary Route 的关系：

- Provider Pool enabled 时，请求优先使用 provider pool selection。
- Provider Pool disabled 或无可用 candidate 时，server 仍回到 Primary Route / existing server behavior。
- 这只是 UI 解释和文档，不在本 phase 改 server 算法，除非发现当前实现不符合 phase 60。

### 4. Failover

Failover 从 Settings 页面迁移到 Router tab。

Router tab 中显示和编辑：

- enabled
- trigger
- ordered routes
- add route
- remove route
- enable/disable route
- move up/down

需要在 UI 里弱化 Failover 的主入口地位：

- Provider Pool 是推荐的运行时 provider health 机制。
- Failover 是兼容旧配置的静态 fallback route 列表。

但不能删除 Failover，因为旧配置仍然需要被读写和维护。

### 5. Provider 页面清理

Provider / Config 页面移除：

- `Router` heading
- `Default:` 输入框

Provider 页面保留：

- provider list
- add/remove provider
- provider endpoint/API key/model/transformer/API kind
- inline endpoint test（phase 64）

Provider 页面不再负责 route selection。

### 6. Settings 页面清理

Settings 页面移除或隐藏：

- Failover section
- Provider Pool section

Settings 保留真正的 app/client/settings：

- config path
- server auto start
- Claude/Codex/OpenCode/OpenClaw client paths
- UI/log related settings
- model mapping advanced settings

如果为了过渡保留旧入口，应只显示跳转提示：

```text
Routing policy moved to Router.
```

本 phase 推荐直接移除 Settings 中的编辑入口，避免两处配置产生分叉。

### 7. Tray 行为

Tray 里现有切换 provider 的能力会直接写 `config.router.default`。

本 phase 不要求删除 tray 切换，但需要更新语义：

- tray item 文案可以继续是 `Switch Provider`，但内部含义是更新 Primary Route。
- Router tab 打开后应能立即看到 tray 修改后的 `Primary Route`。
- 如果 Router tab 有 dirty state，需要处理外部 config change 或刷新提示。

### 8. 状态页关系

Phase 63 的 Status 页面只显示 routing 状态摘要，不做编辑。

Status 页面可以显示：

- Primary Route
- Failover configured/enabled
- Provider Pool configured/enabled/runtime

编辑入口统一在 Router tab。

## 非目标

- 不删除底层 `Router.default` 配置字段。
- 不改变 server route selection 算法，除非当前实现和已有 phase 明确冲突。
- 不删除 Failover 配置结构。
- 不把 Provider Pool 合并进 Failover。
- 不实现 provider endpoint test；它由 phase 64 处理。
- 不实现 OpenCode/OpenClaw；它们由 phase 61 / 62 处理。
- 不把 CLI 作为本 phase 的主要边界；本项目方向是 UI 优先。

## 预计修改文件

- `ccr-rust/crates/ccr-ui/src/app.rs`
- `ccr-rust/crates/ccr-ui/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/config_tab.rs`
- `ccr-rust/crates/ccr-ui/src/config_tab_new.rs`
- `ccr-rust/crates/ccr-ui/src/settings_tab.rs`
- `ccr-rust/crates/ccr-ui/src/tray_manager.rs`
- `ccr-rust/crates/ccr-app-core/src/settings.rs`

新增可能：

- `ccr-rust/crates/ccr-ui/src/router_tab.rs`
- `ccr-rust/crates/ccr-app-core/src/router_settings.rs`

文档：

- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`
- `ccr-rust/docs/client-model-mapping.md`

## 规划的测试用例

### Router tab structure

- app tab list 包含 `Router`。
- Router tab 显示 Primary Route。
- Router tab 显示 Provider Pool。
- Router tab 显示 Failover。
- Config/Provider 页面不再显示 `Router` heading。
- Config/Provider 页面不再显示 `Default:` route 输入框。
- Settings 页面不再显示 Failover 编辑区。
- Settings 页面不再显示 Provider Pool 编辑区。

### Primary Route

- 编辑 Primary Route 会更新 `config.router.default`。
- provider-only route 保存为 provider name。
- `provider,model` route 保存为完整 route。
- 空 route 显示 validation warning。
- 不存在的 provider route 显示 validation warning。
- provider-only route 显示“保留入站模型名”的解释。
- `provider,model` route 显示“固定模型”的解释。

### Provider Pool

- Router tab 能 enable/disable provider pool。
- failure threshold 修改后保存到 `RoutePool.failureThreshold`。
- ban seconds 修改后保存到 `RoutePool.banSeconds`。
- 添加 candidate 使用 provider-only route。
- 添加 `provider,model` candidate 合法。
- 上移/下移 candidate 后 priority 正常归一化。
- 删除 candidate 后 priority 正常归一化。
- disabled candidate 不被计为 active candidate。

### Failover

- Router tab 能 enable/disable failover。
- trigger 修改后保存。
- 添加 route 后 priority 正常追加。
- 重复 route 不重复添加。
- 上移/下移 route 后 priority 正常归一化。
- 删除 route 后 priority 正常归一化。
- 旧配置中的 Failover 能被 Router tab 正确读取。

### Cross-page behavior

- Tray 切换 provider 后 Router tab 显示新的 Primary Route。
- Router tab 修改 Primary Route 后 Config 页面 provider 列表不受影响。
- Router tab 修改 Provider Pool 后 Status 页面 routing 摘要刷新能看到变化。
- Settings 页面保存不会丢失 Router tab 的 routing 配置。
- Config 页面保存不会丢失 Router tab 的 routing 配置。

### Regression

- `ccr_app_core::settings` 中 failover/provider pool 操作测试保持通过。
- config load/save 兼容旧 `Router.default`。
- server 仍能读取 `Router.default`。
- provider pool server runtime 行为不变。

### 验证命令

- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-ui --bin ccr-server`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- UI 有独立 Router tab。
- Provider 页面不再编辑 default route。
- Settings 页面不再编辑 failover/provider pool。
- Router tab 能编辑 Primary Route、Failover、Provider Pool。
- Default route 底层字段仍为 `config.router.default`，保持旧配置兼容。
- Provider Pool 被明确展示为推荐的 runtime provider health 机制。
- Failover 被保留为兼容旧配置的静态 fallback route。
- Status 页面只显示 routing 摘要，不承担编辑入口。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 新增了独立 Router tab，并把 Settings 中的 Failover / Provider Pool 编辑入口移除；底层 `SettingsState` 里的 routing 操作函数继续保留，供 Router tab 复用。
- `Config` tab 名称暂未改成 `Providers`，但页面内容已经不再编辑 `Router.default`。
- Tray 仍沿用 `Switch Provider` 文案，内部继续写 `config.router.default`；Router tab 可通过 Reload 读取 tray 修改后的 Primary Route。
