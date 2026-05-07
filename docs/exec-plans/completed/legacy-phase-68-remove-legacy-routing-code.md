# Phase 68 - Remove Legacy Routing Code

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

彻底删除 Phase 67 之前遗留的 routing 代码，而不是继续隐藏、兼容或导入。

完成后，代码仓库里的 routing 主模型只有：

```text
RoutePool
```

不再保留这些 legacy routing 概念的代码实现：

- `Failover`
- `FailoverConfig`
- `FailoverProvider`
- `ProviderPool`
- `Router.default`
- `Primary Route`
- legacy failover import
- default route fallback
- failover candidates

这不是文案清理。目标是让这些 legacy 机制从代码、测试、配置 schema、UI、server runtime、docs 中消失。

## 背景

Phase 67 已经把 UI 和 server 主路径改为 Route Pool 优先：

- Router tab 不再显示 Primary Route / Default Route。
- Router tab 使用左 list + 中间 `>` / `<` + 右 list。
- Status routing 主状态不再显示 Primary Route。
- server request 主路径不再回退到 `Router.default` 或 legacy failover。

但 Phase 67 仍留下了历史兼容代码：

- `ccr-types` 里仍有 `FailoverConfig` / `FailoverProvider`。
- `Config` 里仍有 `failover` 字段。
- `RoutePool` 仍有 `alias = "ProviderPool"`。
- `ccr-app-core` 仍有 `ensure_failover` / `add_failover_route` / `import_legacy_failover_into_route_pool` 等函数。
- `ccr-server` lib 仍有 `failover_candidates`。
- UI 仍可能显示 legacy failover import。
- CLI / tray / preset / client injection 仍可能读取或写入 `config.router.default`。
- docs 和 tests 仍可能把 legacy routing 当成有效行为。

Phase 68 要把这些残留全部删除。

## 设计方向

### 1. 类型层删除 legacy 字段

`ccr-types` 中删除：

- `Config.failover`
- `FailoverConfig`
- `FailoverProvider`
- `RoutePool` 对 `ProviderPool` 的 serde alias

`RouterConfig.default` 不再作为 routing 字段存在。

如果其他 Router 场景字段还需要保留，例如 `background` / `think` / `longContext` / `webSearch` / `image`，它们必须重新定义为 Route Pool 上的可选策略字段，或者在本 phase 中明确删除。不能继续通过 `Router.default` 做 fallback。

### 2. 配置读写不再迁移 legacy routing

删除所有 legacy routing 迁移逻辑：

- 不读取 `ProviderPool`。
- 不把 `ProviderPool` 保存成 `RoutePool`。
- 不读取 `Failover`。
- 不导入 `Failover.providers`。
- 不保留 unknown top-level `ProviderPool` / `Failover` 作为特殊 routing 兼容字段。

如果用户配置文件里仍有这些字段：

- 普通 unknown field 可以按现有 unknown-preserving 机制处理，前提是不会进入 runtime。
- UI 不显示导入提示。
- server 不读取。
- 测试不把它们当受支持行为。

### 3. App core 删除 failover 操作

`ccr-app-core/src/settings.rs` 删除：

- `ensure_failover`
- `add_failover_route`
- `remove_failover_index`
- `move_failover_up`
- `move_failover_down`
- `import_legacy_failover_into_route_pool`
- `normalize_failover_priorities`
- 所有 failover tests

Endpoint test 里如果仍有 `apply_to_failover`，必须改成 Route Pool：

- `request_apply_to_route_pool`
- `cancel_apply_to_route_pool`
- `apply_to_route_pool`
- 写入 `RoutePool.candidates`

不允许继续写 `Failover.providers`。

### 4. Server 删除 legacy routing

`ccr-server` 删除：

- `failover_candidates`
- `is_failover_status` 如果只服务 legacy failover
- default route fallback
- `select_model(...).default` 主路径依赖
- 任何 `router_default` log field

server runtime 只从 `RoutePool.candidates` 产生 attempts。

当 Route Pool 缺失、disabled 或无 enabled candidates：

```text
Route Pool is not configured or has no enabled routes
```

不尝试 default route。

不尝试 legacy failover。

### 5. Router / Status UI 删除 legacy UI

Router tab 删除：

- legacy failover import 提示
- `Import legacy failover routes` 按钮
- 任何 `Primary Route` / `Default Route` 显示或编辑
- 每行 Add / Remove 形式的旧双列表实现

Status tab 删除：

- legacy failover summary
- Primary Route 主状态
- default fallback 文案

UI 只显示 Route Pool 配置状态和 runtime 状态。

### 6. CLI / Tray / Preset / Client 注入改为 Route Pool

这些位置不再读取 `config.router.default`：

- `ccr-cli`
- Codex config injection
- Claude config injection
- OpenCode config injection
- OpenClaw config injection
- tray provider switching
- preset install / builtin profiles

新的推导规则：

- 当前 route = Route Pool 中第一个 enabled candidate。
- 如果没有 enabled candidate，显示明确错误，不回退。
- tray provider switching 应改成调整 Route Pool candidates 或选择 Route Pool 首项，而不是写 default route。
- presets / default profiles 应写入 Route Pool，而不是 `Router.default`。

### 7. 文档删除 legacy 支持表述

文档中不能再把这些说成支持或兼容：

- Failover
- ProviderPool
- Primary Route
- Router.default
- default route fallback
- legacy failover import

长期差距文档可以保留历史说明，但必须明确它们已经被删除，不是待兼容能力。

## 非目标

- 不删除 Route Pool circuit breaker。
- 不删除 provider-only route 语义。
- 不删除 `provider,model` pinned route 语义。
- 不删除 `background` / `think` 等场景路由能力，除非它们当前只能依赖 `Router.default` 且无法在本 phase 内转换为 Route Pool 语义。
- 不引入新的 CLI-first 管理方式；项目仍是 UI 优先。

## 预计修改文件

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-config/src/lib.rs`
- `ccr-rust/crates/ccr-app-core/src/settings.rs`
- `ccr-rust/crates/ccr-app-core/src/endpoint.rs`
- `ccr-rust/crates/ccr-server/src/lib.rs`
- `ccr-rust/crates/ccr-server/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/router_tab.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- `ccr-rust/crates/ccr-ui/src/tray_manager.rs`
- `ccr-rust/crates/ccr-cli/src/claude_config.rs`
- `ccr-rust/crates/ccr-cli/src/codex_config.rs`
- `ccr-rust/crates/ccr-cli/src/opencode_config.rs`
- `ccr-rust/crates/ccr-cli/src/openclaw_config.rs`
- `ccr-rust/crates/ccr-preset/src/lib.rs`
- `ccr-rust/crates/ccr-router/src/lib.rs`
- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`
- `ccr-rust/docs/ccr-original-feature-gap-analysis.md`
- 相关 tests

## 规划的测试用例

### Type and config cleanup

- `Config` 不再有 `failover` 字段。
- `RoutePool` 不再反序列化 `ProviderPool` alias。
- 保存配置不会产生 `Failover` / `ProviderPool` / `Router.default`。
- legacy `ProviderPool` 输入不会进入 runtime Route Pool。

### App core

- endpoint test apply 写入 Route Pool candidates。
- endpoint test apply 不写入 Failover。
- Route Pool 添加、删除、排序仍归一化 priority。
- Route Pool 去重仍生效。

### Server runtime

- Route Pool disabled 时返回未配置错误。
- Route Pool 缺失时返回未配置错误。
- Route Pool enabled 但无 enabled candidates 时返回未配置错误。
- server 不调用 default route。
- server 不调用 legacy failover candidates。
- 失败 ban / 成功恢复逻辑仍生效。
- 所有 provider 不可用时仍返回最后一个实际尝试 provider 的错误。

### UI

- Router tab 没有 Primary Route / Default Route。
- Router tab 没有 legacy failover import。
- Router tab 没有 per-row Add / Remove。
- Router tab 仍有左 list、中间 `>` / `<`、右 list。
- Status tab 没有 Primary Route。
- Status tab 没有 legacy failover summary。

### CLI / client injection / tray / preset

- Claude injection 从 Route Pool 第一 enabled candidate 推导默认 model。
- Codex injection 从 Route Pool 第一 enabled candidate 推导默认 model。
- OpenCode injection 从 Route Pool 第一 enabled candidate 推导默认 model。
- OpenClaw injection 从 Route Pool 第一 enabled candidate 推导默认 model。
- Route Pool 为空时 client injection 返回明确错误。
- tray switching 不写 `Router.default`。
- builtin profiles 写 Route Pool。
- preset install 不依赖 `Router.default`。

### Repository-wide legacy search gate

实现完成后必须跑这些搜索，并保证没有 runtime/source/test 中的 legacy routing 残留：

```bash
rg -n "Failover|failover|ProviderPool|provider_pool|Primary Route|Default Route|Router\\.default|router\\.default|router_default|default route|legacy failover|Import legacy failover" ccr-rust/crates ccr-rust/docs ccr-rust/plans
```

允许命中的范围仅限：

- 本 phase 文档自身对删除目标的描述。
- 旧 phase 文档中的历史记录，且必须标注为历史，不作为当前能力。

如果源码或测试里仍有命中，本 phase 不算完成。

### 验证命令

- `cargo fmt`
- `cargo test --package ccr-types --lib`
- `cargo test --package ccr-config --lib`
- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-server`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-ui --bin ccr-server`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。
- legacy search gate 必须通过。

## 验收标准

- 代码仓库源码中没有 legacy routing runtime 实现。
- UI 中没有 legacy routing 入口。
- server 不再读取或执行 legacy routing。
- client injection / tray / preset 不再通过旧 default route 推导。
- tests 不再验证 legacy routing 行为。
- docs 和长期计划不再把 legacy routing 表述为当前支持能力。
- 覆盖率保持大于 80%。

## 预留偏差

本 phase 按“当前源码、测试、配置 schema、运行时路径、UI 和长期文档”完成删除。`plans/` 下更早 phase 文档保留当时的设计和验收记录，仍可能出现旧 routing 名称；这些文件是历史计划，不作为当前能力、当前 schema 或当前运行时行为。

实际实现中保留了 `Router.background` / `Router.think` / `Router.longContext` / `Router.webSearch` / `Router.image` 场景路由字段，但它们的空值 fallback 已改为 Route Pool 第一条 enabled route，不再依赖旧默认路由字段。

验证结果：

- `cargo fmt`
- `cargo test --package ccr-types --lib`
- `cargo test --package ccr-config --lib`
- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-server`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-ui --bin ccr-server`
- `cargo llvm-cov --workspace --lib --summary-only`：line coverage `80.74%`
- `rg -n "Failover|failover|ProviderPool|provider_pool|Primary Route|Default Route|Router\\.default|router\\.default|router_default|default route|legacy failover|Import legacy failover" crates -g '*.rs'`：无命中
- `rg -n "Failover|failover|ProviderPool|provider_pool|Primary Route|Default Route|Router\\.default|router\\.default|router_default|default route|legacy failover|Import legacy failover" docs/cc-switch-feature-gap-analysis.md docs/client-model-mapping.md docs/ccr-original-feature-gap-analysis.md`：无命中
