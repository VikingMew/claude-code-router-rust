# TD-010 - Historical Request Records

**状态：** resolved
**优先级：** P1
**关联技术债：** TD-010
**最后更新：** 2026-05-07

## 目标

记录真实历史请求和 upstream attempts，让用户和后续 agent 能回看一段时间内发生过什么，而不是只能查看即时状态或一次 endpoint test。

第一阶段应记录：

- request 或 attempt 时间。
- inbound protocol。
- route。
- provider。
- endpoint。
- upstream model。
- HTTP status 或 network error。
- latency ms。
- retry attempt index。
- final/attempt outcome。

## 非目标

- 不把 endpoint test history 当作历史请求记录。
- 不实现完整会话管理。
- 不保存完整 prompt、response body 或敏感 header。
- 不实现价格、token 成本或长期聚合。
- 不把历史请求记录直接作为 Route Pool 排序的唯一依据。

## 背景

原 TD-010 被写成 “Endpoint Test History And Model Results”，但这不准确。真正需要的是历史请求记录：真实 client request 经过 CCR server 后产生的 upstream attempts 和结果。Endpoint test 是配置探测，不能代表真实请求历史。

TD-008 runtime metrics 已经实现了最小 upstream attempt metrics，因此它实际上覆盖了 TD-010 的第一阶段核心需求。

## 设计方向

以 runtime metrics 作为第一阶段 request history 基础：

- 使用 `ccr_app_core::metrics::UpstreamAttemptMetric` 作为历史 attempt 记录。
- 写入 `~/.claude-code-router/runtime-metrics.jsonl`。
- server 启动时加载近期历史。
- 提供 `/api/runtime-metrics/attempts` 查询最近 attempts。
- 提供 `/api/runtime-metrics/summary` 查询聚合视图。
- Status 页展示 summary，后续可以新增独立 Request History 页或 Logs/Status 子视图。

## 修改文件

- `crates/ccr-app-core/src/metrics.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-ui/src/status_tab.rs`
- `docs/provider-runtime-metrics.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

```sh
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --workspace
```

测试覆盖：

- upstream attempt metrics 可持久化并读取。
- malformed JSONL 行不会导致读取失败。
- summary 按 route/provider 聚合。
- server API 通过现有 auth check。

## 决策日志

- 2026-05-07：纠正 TD-010 范围，从 endpoint test history 改为真实历史请求记录。
- 2026-05-07：第一阶段复用 TD-008 runtime metrics 作为历史 request/attempt 基础，不额外保存请求正文。

## 完成记录

完成：2026-05-07。

- `ccr_app_core::metrics` 已记录 upstream attempt history。
- 历史记录持久化到 `~/.claude-code-router/runtime-metrics.jsonl`。
- server 提供 `/api/runtime-metrics/attempts` 和 `/api/runtime-metrics/summary`。
- Status 页展示 runtime metrics summary。
- 验证：`cargo test --workspace` 通过。

## 完成偏差

- 原计划误写成 endpoint test history；实际纠正为 historical request records。
- 当前记录粒度是 upstream attempt，不是完整 client request envelope。
- 未保存完整 prompt/response body，避免引入隐私和密钥泄漏风险。
- 没有独立 Request History UI；Status 页只展示 summary。
- 曾短暂实现 endpoint test history helper；已撤回，避免把配置探测误当成历史请求记录。
