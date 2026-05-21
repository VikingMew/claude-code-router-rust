# TD-008 - Runtime Metrics Store

**状态：** resolved
**优先级：** P1
**关联技术债：** TD-008
**最后更新：** 2026-05-07

## 目标

实现最小 runtime metrics store，把真实 upstream attempts 记录为可查询数据，用于 provider 质量诊断和后续 Route Pool health。

第一阶段记录 attempt metrics：

- timestamp
- inbound protocol
- route
- provider
- endpoint
- upstream model
- HTTP status 或 error class
- latency ms
- retry attempt index
- outcome

## 非目标

- 不实现完整 `provider-runtime-metrics.md` 的全部指标。
- 不实现价格、token 成本或长期聚合。
- 不改变 Route Pool 排序策略。
- 不接入外部 metrics backend。

## 背景

`docs/provider-runtime-metrics.md` 已定义 attempt/request metrics 方向，但当前 runtime 主要依赖日志和 Route Pool consecutive failure state。Endpoint test 不能代表真实 provider 质量。

## 设计方向

新增 app-core metrics 模块：

- 定义 `UpstreamAttemptMetric`。
- 定义 in-memory store 或 append-only local JSONL store。
- server 在每次 upstream attempt 结束时写入。
- 增加 API 返回最近窗口 summary。

建议第一阶段优先 in-memory + optional JSONL append，避免引入数据库。

API 初稿：

- `/api/runtime-metrics/attempts?limit=100`
- `/api/runtime-metrics/summary`

Summary 至少按 provider/route 聚合：

- attempts
- successes
- failures
- last status/error
- p50 或 average latency

## 修改文件

- `crates/ccr-app-core/src/metrics.rs`
- `crates/ccr-app-core/src/lib.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-server/src/lib.rs`
- `crates/ccr-ui/src/status_tab.rs`
- `docs/provider-runtime-metrics.md`
- `docs/RELIABILITY.md`
- `docs/exec-plans/completed/`

## 验收测试

```sh
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --workspace
```

测试覆盖：

- 成功 upstream attempt 记录 success。
- HTTP 429/5xx 记录 retryable failure。
- network error 记录 network failure。
- metrics summary 按 provider/route 聚合。
- metrics API 需要通过现有 auth check。

## 决策日志

- 2026-05-07：最小实现先记录 attempt metrics，不阻塞 Route Pool 当前行为。

## 完成记录

完成：2026-05-07。

- 新增 `ccr_app_core::metrics`，包含 `UpstreamAttemptMetric`、`RuntimeMetricsStore` 和 route/provider summary。
- metrics 会写入并读取 `~/.claude-code-router/runtime-metrics.jsonl`。
- server 启动时加载最近 metrics，upstream attempt 结束时记录 success、HTTP error、network error、build error 和 provider-not-found。
- 新增 `/api/runtime-metrics/attempts` 和 `/api/runtime-metrics/summary`。
- Status 页展示 runtime metrics summary。
- 验证：`cargo test --workspace` 通过。

## 完成偏差

- 原计划允许 in-memory 或 append-only JSONL；实际实现为 in-memory + JSONL 持久化。
- Summary 使用 average latency，没有实现 p50/p90/p95。
- 已在 Status 页展示 summary，超出第一版 API-only 的最低要求。
- 未把 runtime metrics 接入 Route Pool health score 或排序建议，Route Pool 当前行为保持不变。
- 未实现 request-level metrics、token metrics、长期窗口聚合或外部 metrics sink。
- 后续 `plan-001` 补齐 request id、request-level record 和真实 request/attempt 关联；TD-008 本身不应被解读为完整 response metrics 实现。
