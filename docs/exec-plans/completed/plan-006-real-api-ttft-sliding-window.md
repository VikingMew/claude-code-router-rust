# plan-006 - Real API TTFT Sliding Window

**状态：** completed
**优先级：** P1
**计划编号：** plan-006
**最后更新：** 2026-05-13

## 目标

在真实 API 调用路径中计算 TTFT sliding window，用于观察 provider/model/route 在实际使用中的首 token 响应速度。

这里的“真实 API 调用”指 Claude Code、Codex、OpenCode、OpenClaw 等 client 通过 CCR server 发起的 through-CCR 请求，例如 `/v1/messages` 和 `/v1/responses`。这些数据必须来自实际 upstream response 流，不来自 provider 页面里的手动 endpoint test。

本计划要完成：

- 在真实 upstream attempt 中采集 TTFT。
- 将 TTFT 写入 attempt metrics，并通过 request id 关联到 request metrics。
- 提供按 provider、route、model 聚合的 sliding window TTFT summary。
- 在 API 或 UI 中明确标识这些 TTFT 来自真实流量，不是手动测速。

## 非目标

- 不修改 endpoint test 的测速逻辑。
- 不把 endpoint test latency 写入 TTFT window。
- 不把非真实 client traffic 写入 request history。
- 不保存 prompt、response body、API key 或完整敏感 header。
- 不在本计划中实现 token throughput、成本统计或外部 metrics sink。
- 不把 TTFT window 直接接入 Route Pool 自动排序；本计划只提供可查询指标。

## 背景

`plan-001` 已经把真实 request history 和 endpoint test 分开，并新增 request id 关联。当前仍缺关键 response metric：TTFT。

TTFT 必须在真实 upstream response 返回路径上测量：

- streaming response：从发送 upstream request 到收到第一个有效 body chunk / SSE bytes。
- non-stream response：可以记录 first byte 或完整 response 可用时间，但不能假装它是严格 streaming TTFT；需要在字段或文档里区分。

手动 endpoint test 的 latency 只表示一次主动探测，不能进入 TTFT sliding window。否则用户点击测试按钮会污染真实 provider 质量指标。

## 设计方向

### 1. Metrics 模型

扩展 attempt metrics：

- `ttft_ms: Option<u64>`。
- `ttft_source: Option<String>`，例如 `stream_first_chunk`、`non_stream_response`。
- 保留现有 `latency_ms` 作为 attempt 总体 upstream round-trip 或 response headers 可用时间。

新增 sliding window summary 类型：

- route。
- provider。
- model。
- window seconds。
- samples。
- average ttft ms。
- p50 / p90 / p95 ttft ms。
- latest ttft ms。
- latest request id。

第一版可以使用 in-memory recent attempts 计算窗口；持久化 JSONL 用于 server 重启后恢复最近数据。

### 2. Server 采集点

只在真实 API 路径采集：

- `/v1/messages`
- `/v1/responses`

不采集：

- endpoint test。
- health check。
- logs/config/admin API。

采集方式：

- 发送 upstream request 前记录 start time。
- streaming response 返回前不要只记录 response header latency；需要包装 response body stream，在第一个 chunk 到达时记录 TTFT。
- non-stream response 可以记录 `ttft_source=non_stream_response`，语义是 first response availability，不与 streaming TTFT 混为同一类。
- 记录 TTFT 时更新对应 attempt metric，或在 attempt 完成前延迟写入 metrics，避免先写一条缺 TTFT 的 attempt 再难以更新。

### 3. Sliding Window

支持至少这些窗口：

- 1m
- 5m
- 15m

API 可以先提供：

```text
GET /api/runtime-metrics/ttft-summary?window=300
```

返回真实 traffic 聚合，不包含 endpoint test。

### 4. UI 边界

Status 或后续 Request History UI 可以展示：

- `TTFT from real client traffic`
- samples 数量。
- p50/p90/p95。
- latest TTFT。

UI 不能把 endpoint test latency 放在同一个 TTFT summary 中。

## 修改文件

预计修改：

- `crates/ccr-app-core/src/metrics.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-ui/src/status_tab.rs`
- `docs/provider-runtime-metrics.md`
- `docs/long-term-roadmap.md`
- `docs/RELIABILITY.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

代码检查：

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --package ccr-ui
cargo test --workspace
```

行为验收：

- endpoint test 不会产生 TTFT sample。
- 真实 streaming API 调用产生 TTFT sample。
- TTFT sample 带 request id，并能关联到 attempt/request history。
- `/api/runtime-metrics/ttft-summary?window=300` 返回真实流量窗口聚合。
- summary 至少包含 samples、average、p50、p90、p95 和 latest。
- UI 文案明确 TTFT 来自真实 client traffic。

## 决策日志

- 2026-05-13：创建 `plan-006`，专门处理真实 API 调用路径上的 TTFT sliding window。
- 2026-05-13：明确 endpoint test 是手动测速/配置诊断，不参与 TTFT window。

## 完成记录

完成：2026-05-13。

- `UpstreamAttemptMetric` 新增 `ttft_ms` 和 `ttft_source`。
- `RuntimeMetricsStore` 新增 TTFT sliding window 聚合。
- 新增 `TtftMetricSummary`，按 route/provider/model 聚合 samples、average、p50、p90、p95、latest 和 latest request id。
- `ccr-server` 新增 `/api/runtime-metrics/ttft-summary?window=300`。
- 真实 streaming API response 会在第一个 upstream body chunk 到达时记录 `ttft_source=stream_first_chunk`。
- 真实 non-stream API response 会记录 `ttft_source=non_stream_response`。
- tool interception streaming 路径会记录 `ttft_source=stream_interception_response_headers`，避免该真实请求完全缺失 TTFT sample。
- Status UI 新增 `TTFT from real client traffic (5m window)` 展示。
- endpoint test 路径未接入 TTFT metrics。

验证：

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --package ccr-ui
```

## 完成偏差

- 本计划没有实现真实 token throughput、成本统计或外部 metrics sink。
- non-stream response 的 TTFT 语义是 response availability，不等同 streaming first token。
- tool interception 路径当前记录 response headers availability；由于该路径会先完整读取 upstream response 再重写 SSE，未做到严格 first chunk TTFT。
- UI 展示的是 5m 固定窗口，没有提供窗口切换控件。
- Route Pool health 仍未直接使用 TTFT window。
