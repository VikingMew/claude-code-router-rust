# Phase 60 - Provider Pool and Circuit Breaker

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

把当前“单个默认 provider / 静态 failover route”升级为 UI 优先的 provider 池：

- 用户可以维护一个 provider 列表。
- provider 列表支持插入、删除、调换顺序。
- 请求时按列表顺序选择可用 provider。
- 一个 provider 连续 3 次调用失败后，自动 ban 1 小时。
- ban 期间跳过该 provider，选择下一个 provider。
- 如果所有 provider 都不可用或被 ban，返回最后一个实际尝试 provider 的错误。
- UI 能显示 provider 顺序、ban 状态、失败次数、ban 到期时间。

这个 phase 的核心不是再加一个静态 fallback，而是做运行时 provider pool/circuit breaker，让 CCR 在 provider 不稳定时自动避开坏节点。

## 背景

当前已有 phase 47 的静态 failover：

- `Failover.providers` 是有序 route 列表。
- server 遇到网络错误、5xx、429 时按 priority 尝试候选。
- UI 可以上移/下移、启用/禁用 failover route。

但它还不满足这个需求：

- 没有“连续失败 3 次”的状态记忆。
- 没有“ban 1 小时”的运行时状态。
- 没有 provider 级状态展示。
- failover list 更像静态 backup route，不是可维护 provider pool。
- 当前 default provider 仍然承担主要入口，其他 provider 只是失败后的候选。

本 phase 要把 provider 列表变成一等概念：请求先从 pool 中选择可用 provider，然后根据运行时健康状态自动跳过失败 provider。

## 术语

- Provider pool：用户维护的 provider/route 有序列表。
- Candidate：pool 中一个可尝试的 route，例如 `zenmux` 或 `openai,gpt-4o`。
- Consecutive failures：某个 candidate 连续失败次数，成功后清零。
- Banned provider：连续失败达到阈值后，暂时不可选。
- Ban window：默认 1 小时。
- Last provider error：本次请求最后一个实际尝试 provider 的错误响应或网络错误。

## 设计方向

### 1. 配置结构

建议新增独立配置，避免继续扩展旧 `Failover` 语义导致混乱：

```json
{
  "RoutePool": {
    "enabled": true,
    "failureThreshold": 3,
    "banSeconds": 3600,
    "candidates": [
      {
        "route": "zenmux",
        "enabled": true,
        "priority": 1
      },
      {
        "route": "siliconflow,zai-org/GLM-4.6",
        "enabled": true,
        "priority": 2
      }
    ]
  }
}
```

说明：

- `route` 可以是 provider-only，也可以是 `provider,model`。
- provider-only route 继续沿用 phase 57 语义：只切 provider，model 透传入站请求。
- `failureThreshold` 默认 3。
- `banSeconds` 默认 3600。
- `enabled = false` 的 candidate 不参与选择。
- 旧 `Failover` 暂时保留，但 server 请求选择优先使用 `RoutePool`；后续可迁移旧 Failover 到 RoutePool。

### 2. 运行时状态

运行时需要维护 provider/candidate 状态：

```rust
struct RoutePoolRuntimeState {
    route: String,
    consecutive_failures: u32,
    banned_until: Option<SystemTime>,
    last_error: Option<String>,
    last_failure_at: Option<SystemTime>,
    last_success_at: Option<SystemTime>,
}
```

状态范围：

- server 进程内内存状态即可。
- server 重启后 ban 状态清空。
- 本 phase 不要求持久化 ban 状态。

并发要求：

- 多请求并发时，状态更新必须线程安全。
- 使用 `Arc<Mutex<_>>` / `Arc<RwLock<_>>` 或等价结构。
- 更新粒度按 route/candidate。

### 3. 选择算法

请求开始：

1. 读取 RoutePool 配置。
2. 按 `priority` 排序 candidate。
3. 跳过 disabled candidate。
4. 跳过 `banned_until > now` 的 candidate。
5. 对剩余 candidate 按顺序尝试。

如果 RoutePool 关闭：

- 保持现有 `Router.default` + `Failover` 行为，避免破坏旧配置。

如果 RoutePool 开启但没有可用 candidate：

- 如果所有 candidate 都被 ban，选择“最早解 ban 的 candidate”或返回最后一次错误。
- 本 phase 推荐：不主动尝试 banned candidate，直接返回最近一次 `last_error`；如果没有 last_error，返回明确错误 `all providers are banned or disabled`。

请求尝试：

- 成功：
  - 清零该 candidate 的 `consecutive_failures`。
  - 清除 `banned_until`。
  - 记录 `last_success_at`。
  - 返回响应，不再尝试后续 candidate。
- 失败：
  - 记录 `last_error`。
  - `consecutive_failures += 1`。
  - 如果达到 threshold，设置 `banned_until = now + banSeconds`。
  - 尝试下一个可用 candidate。
- 所有 candidate 都失败：
  - 返回最后一个实际尝试 candidate 的错误。

### 4. 什么算失败

失败计数应覆盖：

- 网络错误。
- timeout。
- HTTP 5xx。
- HTTP 429。
- upstream response body 读取失败。
- build upstream request 失败，如果是 candidate 配置导致，也算该 candidate 失败。

不建议计入连续失败：

- 用户请求格式错误。
- inbound JSON 解析失败。
- 明确的 4xx 鉴权/参数错误是否计入需要谨慎。

本 phase 默认：

- 401/403 计入失败并可 ban，因为 API key/provider 配置错误会持续失败。
- 400/404 不计入 ban，直接返回错误或继续现有不可重试策略，避免因为模型参数问题误 ban provider。
- 429 计入失败并可 ban。
- 5xx 计入失败并可 ban。

### 5. UI 设计

Settings 页面增加 Provider Pool 区域，或替换/升级当前 Failover 区域：

控件：

- Enable provider pool
- Failure threshold 数字输入，默认 3
- Ban duration 数字输入，默认 3600 秒
- Candidate 列表：
  - enabled checkbox
  - route
  - priority/order
  - consecutive failures
  - ban 状态
  - banned until
  - last error 摘要
  - 操作：上移、下移、删除
- 添加 candidate：
  - route 下拉，来源于 provider/model routes
  - provider-only route 也要支持，尤其 Anthropic-compatible provider

Status 页面增加只读状态：

- 当前启用 provider pool 与否
- 可用 candidate 数
- banned candidate 数
- 每个 candidate 的健康状态

Logs：

- 每次 candidate 失败写 `route_pool_candidate_failed`
- ban 时写 `route_pool_candidate_banned`
- 跳过 ban candidate 写 `route_pool_candidate_skipped`
- 最终失败写 `route_pool_exhausted`
- 成功恢复写 `route_pool_candidate_recovered`

### 6. 与现有 Failover 的关系

短期兼容：

- `RoutePool.enabled = true` 时，server 使用 provider pool 逻辑。
- `RoutePool.enabled = false` 或缺失时，继续使用现有 `Router.default` + `Failover`。

迁移方向：

- 后续可以把旧 `Failover.providers` 显示为 legacy mode。
- 或提供按钮“Import Failover into Provider Pool”。

本 phase 不删除旧 Failover。

## 非目标

- 不持久化 ban 状态到磁盘。
- 不实现复杂加权负载均衡。
- 不实现随机选择。
- 不实现指数退避。
- 不删除旧 Failover 配置。
- 不要求 CLI 完整支持 provider pool。
- 不做跨进程共享状态。
- 不把 provider endpoint candidates 纳入同一个 pool；本 phase pool 单位是 route/provider。

## 预计修改文件

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-app-core/src/settings.rs`
- `ccr-rust/crates/ccr-server/src/lib.rs`
- `ccr-rust/crates/ccr-server/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/settings_tab.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- 可能新增 `ccr-rust/crates/ccr-server/src/route_pool.rs`
- 可能新增 `ccr-rust/crates/ccr-app-core/src/route_pool.rs`

## 规划的测试用例

### 配置与排序

- `RoutePool` 缺失时默认关闭，不影响现有行为。
- `RoutePool.enabled = true` 时使用 candidate list。
- candidate 按 priority 排序。
- disabled candidate 不参与选择。
- 重复 route 去重或返回明确校验错误。
- 插入 candidate 后 priority 正确。
- 删除 candidate 后 priority 归一化。
- 上移/下移 candidate 后 priority 正确。

### 运行时状态

- 成功请求会清零该 candidate 的 consecutive failures。
- 单次失败后 failure count +1。
- 连续三次失败后设置 `banned_until`。
- ban 未过期时 candidate 被跳过。
- ban 过期后 candidate 可再次尝试。
- ban 过期后成功会清除 ban 和 failure count。
- 多 candidate 下，第一个被 ban 后选择第二个。
- 所有 candidate 都被 ban 时返回最近 last error。
- 所有 candidate 都失败时返回最后一个实际尝试 provider 的错误。

### 错误分类

- network error 计入失败。
- timeout 计入失败。
- HTTP 429 计入失败。
- HTTP 5xx 计入失败。
- HTTP 401/403 计入失败。
- HTTP 400 默认不计入 ban。

### UI

- Settings 页面能显示 provider pool 开关。
- Settings 页面能添加 candidate。
- Settings 页面能删除 candidate。
- Settings 页面能上移/下移 candidate。
- Settings 页面能编辑 threshold 和 ban duration。
- Status 页面能显示 banned/unbanned 状态。
- Status 页面能显示 consecutive failures 和 banned until。

### 日志

- candidate failed 会写 app log。
- candidate banned 会写 app log。
- skipped banned candidate 会写 app log。
- exhausted provider pool 会写 app log。
- 日志不包含明文 API key。

### 验证命令

- `cargo test --package ccr-types --lib`
- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-server --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --package ccr-ui`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- UI 中可以维护 provider pool candidate 列表。
- candidate 支持插入、删除、上移、下移。
- provider/candidate 连续 3 次失败后自动 ban 1 小时。
- ban 期间不会选择该 candidate。
- 第一个 candidate 被 ban 或失败时会选择下一个 candidate。
- 所有 candidate 都不可用时返回最后一个实际 provider 错误。
- Status 页面能看到 ban 状态和失败次数。
- Logs 能看到 candidate 失败、ban、跳过和 exhausted 事件。
- 旧 `Router.default` + `Failover` 在 RoutePool 未启用时不回退。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 本 phase 没有拆出独立 `route_pool.rs` 模块；为了保持改动集中，配置 helper 留在 `ccr-server/src/lib.rs` 和 `ccr-app-core/src/settings.rs`，运行时状态留在 `ccr-server/src/main.rs`。
- Status 页面读取的是服务端进程内 runtime snapshot；server 未启动时只显示配置状态，不会显示 ban 状态。
- `banned_until` 在 UI 中先显示 epoch 秒，后续可以再做本地时间格式化。
- RoutePool 启用时优先使用 pool candidate 列表；旧 `Failover` 仍作为 RoutePool 未启用时的 legacy 行为保留。

## 实际修改

- `ccr-types/src/lib.rs`
  - 新增 `Config.RoutePool`、`RoutePoolConfig`、`RoutePoolCandidate`。
  - `failureThreshold` 默认 `3`，`banSeconds` 默认 `3600`，并补齐 Rust `Default` 实现。
- `ccr-app-core/src/settings.rs`
  - 新增 provider pool 初始化、route 添加、删除、上移、下移、priority 归一化。
  - provider pool route 同时支持 provider-only 和 `provider,model`。
- `ccr-server/src/lib.rs`
  - 新增 provider pool candidate 排序、去重、启用判断、阈值/ban 时长 clamp helper。
- `ccr-server/src/main.rs`
  - 新增进程内 provider pool runtime state。
  - 请求路径在 RoutePool 启用时按 pool 顺序尝试候选。
  - 网络错误、构建失败、HTTP 5xx/429/401/403 计入连续失败。
  - 达到阈值后 ban 对应 route，ban 期间跳过。
  - 成功后清除失败计数和 ban。
  - 所有 candidate 不可用时返回最后一次 provider 错误。
  - 新增 `/api/route-pool/status` 供 UI 读取运行时状态。
  - 写入 route-pool 事件日志：失败、ban、跳过、恢复、耗尽。
- `ccr-ui/src/settings_tab.rs`
  - Failover 设置页增加 Provider Pool 区域。
  - 支持启用、编辑阈值/ban 秒数、添加、删除、上移、下移、启用/禁用 candidate。
- `ccr-ui/src/status_tab.rs`
  - Status 页面显示 Provider Pool 配置状态。
  - server 运行时读取 `/api/route-pool/status`，展示失败次数、ban 到期、最近错误、最近成功/失败时间。

## 验证结果

- `cargo test --workspace` 通过。
- `cargo build --bin ccr-server --bin ccr-ui` 通过。
- `cargo llvm-cov --workspace --lib --summary-only` 通过，行覆盖率 `82.33%`。
