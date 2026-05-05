# Phase 50 - Endpoint Speed Test

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

增加 endpoint 批量测速能力。

用户可以为 provider 配置多个 endpoint candidate，CCR 能批量测试连通性、延迟和流式响应，并把结果用于 UI 展示和后续 failover 排序。

本 phase 遵循 UI 优先：候选 endpoint 管理、批量测速、结果排序和应用到 failover 都必须先在 UI 中完成。CLI 只作为后置能力或调试入口，不作为 P0 验收条件。

## 背景

`cc-switch` 有 speed test 和 stream check：

- `../cc-switch/src-tauri/src/services/speedtest.rs`
- `../cc-switch/src-tauri/src/services/stream_check.rs`
- `../cc-switch/src-tauri/src/commands/stream_check.rs`
- `../cc-switch/src/components/usage/ModelTestConfigPanel.tsx`

当前 Rust 版已有基础 endpoint test，但更像单点手动测试。缺少批量候选、排序、历史和与 failover 的联动。

## 完成项

- [x] Provider 支持 endpoint candidates
- [x] UI 支持添加、删除、编辑 endpoint candidates
- [x] 支持批量测速
- [x] 记录 latency、HTTP status、错误信息、测试时间
- [x] 支持 stream response check
- [x] 测试结果按 latency 和成功率排序
- [x] 可将测速结果应用到 failover 排序
- [x] 测试结果持久化或缓存
- [x] 单元测试覆盖排序、失败处理、结果解析

## 配置方向

建议 provider 支持：

```json
{
  "name": "openai",
  "api_base_url": "https://api.openai.com/v1/responses",
  "endpoint_candidates": [
    "https://api.openai.com/v1/responses",
    "https://proxy.example.com/v1/responses"
  ]
}
```

## 测速规则

- 每个 endpoint 独立超时
- 测试失败不影响其他 endpoint
- 支持并发限制
- 不在 UI 线程中阻塞
- 鉴权错误和网络错误分开显示
- 流式测试需要验证至少收到一个有效事件

## UI 设计

Endpoint Test 页面升级为：

- Provider selector
- Endpoint candidates table
- Run all
- Run selected
- Latency
- Status
- Last tested
- Error detail
- Apply order to failover

## CLI 后置能力

CLI 可以后续提供 endpoint list/add/remove/test/apply-failover 等命令，但本 phase 不以 CLI 为主入口。

## 修改文件

预计涉及：

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-config/src/lib.rs`
- `ccr-rust/crates/ccr-server/src/*`
- `ccr-rust/crates/ccr-cli/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/*`

## 预留的开发偏差

- 本 phase 按 UI 优先落地，CLI endpoint 命令未作为主入口实现；只保留底层测试 helper 供 UI 调用。
- 测试结果第一版保存在当前 UI 会话内，没有写入独立历史文件。
- 批量测速当前使用阻塞请求，UI 显示 running 状态，但没有后台任务队列。
- Stream check 使用最小流式请求和响应文本启发式判断，不做完整 SSE 语义解析。
- `Apply order to failover` 先确认再写入，但写入的是 provider/model route 排序，不是 endpoint 级 route。
- Endpoint candidates 仍挂在 provider 配置上，后续如果引入 provider DB 再迁移。

## 规划的测试用例

- Config 解析：provider 缺少 `endpoint_candidates` 时使用 `api_base_url` 作为默认候选。
- Config 保存：新增、删除、编辑 endpoint candidate 后持久化，未知字段不丢失。
- URL 校验：非法 URL 被拒绝，错误可展示。
- 批量测速：多个 endpoint 中一个失败不影响其他 endpoint 继续测试。
- 超时处理：单个 endpoint 超时后记录 timeout 错误和耗时。
- 排序逻辑：成功 endpoint 按 latency 升序排序，失败 endpoint 排在后面。
- 错误分类：网络错误、HTTP 4xx、HTTP 5xx 显示为不同错误类型。
- Stream check：收到有效 SSE/stream event 时标记 stream 可用。
- Stream check：只返回非流式响应时标记 stream 不可用但保留 HTTP 状态。
- UI：Run all 过程中显示 running 状态，完成后显示 last tested。
- UI：Apply order to failover 前需要确认，确认后排序写入配置。

## 验证

```bash
cargo test --package ccr-config
cargo test --package ccr-server
cargo test --package ccr-ui
cargo build --package ccr-ui
```

## 验收标准

- 用户可以维护 provider 的多个 endpoint candidates
- 用户可以一键批量测速
- UI 显示延迟、状态、错误和测试时间
- 支持 stream check
- 测试结果可以用于 failover 排序
- 排序和失败处理有测试覆盖
