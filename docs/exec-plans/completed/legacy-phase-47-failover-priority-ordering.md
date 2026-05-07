# Phase 47 - Failover Priority Ordering

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

增加 provider failover 排序能力。

当主 provider 不可用时，CCR 应该可以根据用户配置的有序候选列表选择下一个 provider，而不是只依赖单一默认 route。排序需要能被 UI 查看和调整，并且后续可以和 endpoint 测速结果联动。

本 phase 遵循 UI 优先：设置、排序、状态展示都必须先在 UI 中完成。CLI 只作为后置能力或调试入口，不作为 P0 验收条件。

## 背景

`cc-switch` 支持 failover queue、failover toggle、候选 provider 管理和相关 UI：

- `../cc-switch/src-tauri/src/proxy/failover_switch.rs`
- `../cc-switch/src-tauri/src/commands/failover.rs`
- `../cc-switch/src/components/proxy/FailoverToggle.tsx`
- `../cc-switch/src/components/proxy/AutoFailoverConfigPanel.tsx`

当前 Rust 版没有 provider failover 排序。用户只能配置默认路由或手动切换 provider。

## 完成项

- [x] 新增 failover 配置结构
- [x] 支持 provider 有序列表
- [x] 支持 provider enabled/disabled 状态
- [x] 支持手动上移、下移、拖拽或等价排序操作
- [x] 支持选择 failover trigger policy
- [x] Server 路由失败时按顺序尝试候选 provider
- [x] Status 页面显示当前 failover 状态
- [x] UI 提供 failover 排序面板
- [x] 失败事件写入日志
- [x] 单元测试覆盖排序、禁用、 fallback 选择

## 配置方向

建议在 config 中加入独立 failover section：

```json
{
  "Failover": {
    "enabled": true,
    "trigger": "network_or_5xx",
    "providers": [
      {
        "route": "anthropic,claude-sonnet-4",
        "enabled": true,
        "priority": 1
      },
      {
        "route": "openai,gpt-5-codex",
        "enabled": true,
        "priority": 2
      }
    ]
  }
}
```

## 路由规则

- 如果 `Failover.enabled = false`，保持现有路由行为
- 如果 primary route 成功，不触发 failover
- 如果 primary route 发生可重试错误，则按排序尝试候选 provider
- 不对 4xx 鉴权错误无限重试
- 禁用 provider 不参与候选
- 同一次请求内需要记录尝试过的 route，避免循环

## UI 设计

Settings 或 Provider 页面增加 `Failover` 分组：

- 开关：Enable failover
- 触发策略选择
- Provider 有序列表
- 每行显示 provider、model、enabled、最近状态
- 操作：上移、下移、禁用、删除
- 可选：根据测速结果自动排序

## CLI 后置能力

CLI 可以后续提供 `status/list/move/enable/disable` 等命令，但本 phase 不以 CLI 为主入口，也不要求 CLI 覆盖所有 UI 操作。

## 修改文件

预计涉及：

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-config/src/lib.rs`
- `ccr-rust/crates/ccr-router/src/lib.rs`
- `ccr-rust/crates/ccr-server/src/lib.rs`
- `ccr-rust/crates/ccr-cli/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/*`

## 预留的开发偏差

- 本 phase 按 UI 优先落地，CLI failover 命令未作为主入口实现；后续如需要自动化再补。
- UI 第一版使用按钮上移/下移排序，没有做拖拽排序。
- Failover 触发范围落在 server upstream 请求的网络错误、5xx 和 429；没有实现完整 circuit breaker。
- Failover 事件通过 server tracing 日志记录，没有新增独立事件表或 UI 事件历史。
- Endpoint 测速可以写入 failover 顺序，但当前 failover route 仍以 provider/model 为单位，不以 endpoint 为单位。

## 规划的测试用例

- Config 默认值：未配置 `Failover` 时，failover 默认为关闭，不影响现有路由。
- Config 解析：有序 provider 列表能正确解析 `route`、`enabled`、`priority`。
- Config 保存：调整排序后写回配置，未知字段不丢失。
- 排序逻辑：priority 乱序时按 priority 升序生成候选队列。
- 禁用逻辑：`enabled = false` 的 provider 不进入候选队列。
- 去重逻辑：同一个 route 重复出现时，只尝试一次或返回明确校验错误。
- 路由成功：primary route 成功时不触发 failover。
- 可重试错误：primary route 返回网络错误或 5xx 时尝试下一个候选。
- 不可重试错误：4xx 鉴权类错误不无限重试。
- 循环防护：同一次请求内不会反复尝试已经失败过的 route。
- UI 状态：Status 页面能显示 failover enabled、当前候选数量和最近失败原因。
- UI 排序：用户可以在设置页调整 provider 顺序，保存后顺序持久化。

## 验证

```bash
cargo test --package ccr-router
cargo test --package ccr-config
cargo test --package ccr-ui
cargo build --package ccr-server
```

## 验收标准

- 用户可以配置有序 failover provider 列表
- 用户可以在 UI 中查看和调整排序
- 主 provider 失败时按顺序尝试候选 provider
- 禁用 provider 不会被尝试
- Failover 状态在 Status 页面可见
- 排序逻辑有测试覆盖
