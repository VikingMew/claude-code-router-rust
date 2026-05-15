# plan-001 - Real Request Response Metrics Alignment

**状态：** completed
**优先级：** P1
**计划编号：** plan-001
**最后更新：** 2026-05-13

## 目标

修正“真实使用中的响应数据采集”和“endpoint test 点击测速”之间的概念混淆。

本计划讨论的目标是：当 Claude Code、Codex、OpenCode、OpenClaw 等 client 通过 CCR server 实际发起请求时，CCR 应该记录 request、attempt 和 response 层面的运行数据，用于诊断真实 provider 质量、Route Pool 健康、重试效果和用户体感延迟。

Endpoint test 是另一个边界清晰的功能：用户在 provider/config UI 中点击测试按钮，CCR 主动向某个 endpoint 发送探测请求，用来验证配置、payload/header、stream 支持和一次性测速。它不能代表真实流量质量，不能写入真实 request history，也不能作为 Route Pool health 的主要输入。

本计划要完成两类修正：

- 文档修正：长期文档、历史完成记录和计划索引必须清楚区分 endpoint test、request metrics、attempt metrics 和 response metrics。
- 实现修正：runtime metrics 要从“最小 upstream attempt 结果列表”推进到真实 request/attempt/response 数据模型，至少建立 request id 关联和 request-level record。

## 非目标

- 不删除 endpoint test。
- 不阻止 endpoint test 继续写诊断日志。
- 不把 endpoint test 的测速结果写入真实 request history。
- 不保存 prompt、response body、API key 或完整敏感 header。
- 不在本计划中接入外部 metrics backend。
- 不把单次 endpoint test latency 用作 Route Pool health score。
- 不要求一次性实现成本、价格或完整 token 账单。

## 当前偏差

长期设计文档的方向大体正确，但执行口径出现了偏差：

- `docs/provider-runtime-metrics.md` 已写明 provider 指标必须来自真实业务请求，不应来自 endpoint test。
- `docs/long-term-roadmap.md` 已写明 Endpoint test 是主动探测，Runtime metrics 是真实请求观测。
- 旧完成计划把最小 `UpstreamAttemptMetric` 存储描述成 request history 的第一阶段完成，容易误导后续工作。
- 当前实现只有 attempt-level 的最小字段，缺少 request-level record、request id correlation、first byte、TTFT、stream timing、token throughput、client app、request kind 和可解释错误分类。
- UI 当前展示 runtime summary，但还没有独立表达“这是实际流量统计，不是测速结果”的边界。

## 术语边界

后续文档和代码命名必须按下面边界使用：

- `Endpoint test`：用户点击测试按钮触发的主动探测。用于配置诊断和一次性测速。
- `Request metrics`：一个真实 client request 的整体记录。一个 request 可能包含多个 upstream attempts。
- `Attempt metrics`：真实 request 内某一次 provider/model/endpoint upstream attempt 的记录。
- `Response metrics`：真实 response 侧指标，包括 first byte、TTFT、total latency、stream duration、chunk interval、token throughput 等。
- `Request history`：真实 request 和 attempts 的历史记录。不包含 endpoint test。

## 执行步骤

### 1. 文档纠偏

更新这些文件：

- `docs/long-term-roadmap.md`
- `docs/provider-runtime-metrics.md`
- `docs/RELIABILITY.md`
- `docs/QUALITY_SCORE.md`
- `docs/exec-plans/tech-debt-tracker.md`
- `docs/exec-plans/completed/td-008-runtime-metrics-store.md`
- `docs/exec-plans/completed/td-010-request-history.md`

要求：

- 明确当前实现只是最小 attempt store，不是完整 request/response metrics。
- 明确 endpoint test 只属于配置诊断，不属于 request history。
- 历史完成记录必须在“完成偏差”里写清真实缺口。
- 新计划、债务和后续工作统一使用 `plan-001`、`plan-002` 这套编号，不再新增 TD 编号。

### 2. 数据模型纠偏

在 `ccr-app-core` 中把 metrics 模型拆清楚：

- 保留 attempt-level record，但补充 request id。
- 新增 request-level record。
- 明确哪些字段当前可采集，哪些字段暂时为空。
- 读写仍使用本地 JSONL，避免引入数据库迁移。

第一阶段必须包含：

- request id。
- request started/finished timestamp。
- inbound protocol 或 request kind。
- selected route。
- requested model。
- final outcome。
- attempt count。
- attempt provider、endpoint、model、HTTP status/network error、latency、retry index、outcome。

后续可选字段：

- first byte latency。
- TTFT。
- stream duration。
- stream chunk count。
- max stream chunk gap。
- token counts。
- tokens per second。

### 3. Server 采集纠偏

在 `ccr-server` 的真实请求路径中采集 metrics：

- 每个 inbound request 创建 request id。
- 每次 upstream attempt 记录 attempt metrics，并附带 request id。
- request 完成时记录 request metrics。
- retry、provider-not-found、body build error、HTTP error、network error 都要有可解释 outcome。
- endpoint test 路径不写入 request metrics 或 attempt metrics。

### 4. API 和 UI 边界

API 边界：

- `/api/runtime-metrics/attempts` 返回真实 upstream attempts。
- `/api/runtime-metrics/requests` 返回真实 client requests。
- `/api/runtime-metrics/summary` 只聚合真实流量数据。

UI 边界：

- Endpoint test 结果只在 provider/config 诊断区域展示。
- Runtime metrics summary 必须标识为真实 traffic 数据。
- 如果增加 request history UI，它只能读取真实 request/attempt metrics。

## 修改文件

预计修改：

- `docs/long-term-roadmap.md`
- `docs/provider-runtime-metrics.md`
- `docs/RELIABILITY.md`
- `docs/QUALITY_SCORE.md`
- `docs/exec-plans/tech-debt-tracker.md`
- `docs/exec-plans/completed/td-008-runtime-metrics-store.md`
- `docs/exec-plans/completed/td-010-request-history.md`
- `crates/ccr-app-core/src/metrics.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-ui/src/status_tab.rs`

## 验收测试

文档检查：

```sh
rg -n "endpoint test history|Endpoint test history|test history" docs
rg -n "plan-001|Request metrics|Attempt metrics|Response metrics|真实流量|真实请求" docs
```

代码检查：

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --workspace
```

行为验收：

- 点击 endpoint test 不会写入 request history。
- 一次真实 client request 会产生 request-level record。
- 一个真实 request 内的每次 upstream retry 都有 attempt-level record。
- attempts 可以通过 request id 关联回 request。
- runtime summary 不读取 endpoint test 结果。
- UI 不把 endpoint test latency 说成 provider health 或真实响应质量。

## 决策日志

- 2026-05-07：确认 endpoint test 是点击按钮测速/配置验证；真实响应数据采集必须来自实际 client traffic。
- 2026-05-07：当前最小 attempt store 可以保留，但必须在文档中降级为基础设施，不等同完整 request history。
- 2026-05-13：新工作统一使用 `plan-001`、`plan-002` 编号；本计划不再引入新的 TD 编号。
- 2026-05-13：本计划范围从“文档概念纠偏”扩展为“文档和最小真实 request/response metrics 实现纠偏”。

## 完成记录

完成：2026-05-13。

- `ccr_app_core::metrics::UpstreamAttemptMetric` 增加 `request_id`，用于把真实 upstream attempts 关联回 client request。
- 新增 `ccr_app_core::metrics::ClientRequestMetric` 和 `RequestOutcome`。
- 新增 request metrics JSONL 持久化，路径为 `~/.claude-code-router/runtime-request-metrics.jsonl`。
- `RuntimeMetricsStore` 现在同时管理 recent attempts 和 recent requests。
- `ccr-server` 在真实 through-CCR 请求路径中为每个 request 生成 request id。
- `send_with_route_pool` 在真实 upstream attempts 上记录带 request id 的 attempt metrics。
- request 成功、最终 HTTP error、Route Pool 未配置或 exhaust 时会写 request-level record。
- 新增 `/api/runtime-metrics/requests`，用于查询真实 client request history。
- Status UI 文案改为 `Runtime metrics from real client traffic`，避免和 endpoint test 测速混淆。
- 长期文档和旧完成计划已补充偏差说明，明确 endpoint test 不属于 request history。

验证：

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --package ccr-ui
cargo test --workspace
```

## 完成偏差

- 本计划完成了最小真实 request/attempt history，没有实现完整 response metrics。
- `first byte latency`、`TTFT`、`stream duration`、`stream chunk count`、`max stream chunk gap`、token counts 和 tokens per second 仍是后续工作。
- request-level record 当前在 upstream response 返回时记录；stream body 完整消费后的最终流式质量还未纳入。
- UI 只修正了 Status summary 文案，没有新增独立 Request History 页面。
- Route Pool health score 仍未使用 runtime metrics，当前只保留真实流量 metrics 作为后续输入。
