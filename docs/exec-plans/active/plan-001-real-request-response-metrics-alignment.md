# plan-001 - Real Request Response Metrics Alignment

**状态：** active
**优先级：** P1
**计划编号：** plan-001
**最后更新：** 2026-05-08

## 目标

纠正 runtime metrics、request history 和 endpoint test 之间的边界混淆。

长期文档真正讨论的是用户实际使用 CCR 时的响应数据采集：Claude Code、Codex、OpenCode、OpenClaw 等 client 请求经过 CCR server 后产生的 request/attempt 指标、响应延迟、错误分类、重试链路和流式体验。

Endpoint test 是用户点击测试按钮后的主动测速/配置验证行为。它只能证明某个 endpoint 在一次探测里是否可用、探测延迟是多少、payload/header/stream 是否基本可用。它不能代表真实使用中的 provider 质量，也不能作为 request history 的主体。

本计划要把文档、指标模型、API 命名和 UI 展示重新对齐到这个边界。

## PDCA

### Plan

Problem:

- Runtime metrics、request history、response metrics 和 endpoint test 的边界仍容易混淆。
- 历史完成口径把最小 attempt store 误读成完整 request history 的风险较高。

Scope:

- 纠正文档、指标模型、API 命名和 UI 展示边界。
- 明确真实 request/attempt metrics 必须来自实际 client traffic。

Non-goals:

- 不删除 endpoint test。
- 不把 endpoint test latency 用作 Route Pool health。
- 不保存敏感请求/响应正文。

Risks:

- 如果缺 request id 和 request-level record，attempt metrics 仍无法构成真实 request history。
- 如果 UI 文案继续复用 endpoint test 结果，用户会误判 provider 真实质量。

Intended verification:

- 文档中 endpoint test 和 runtime metrics 术语可检索区分。
- 真实 client request 产生 request-level record。
- endpoint test 不写入 request history。

### Do

Implementation steps:

- 更新长期文档和历史完成偏差。
- 扩展或拆分 metrics record。
- 增加 request-level API 和 UI 展示边界。

Files expected to change:

- `docs/long-term-roadmap.md`
- `docs/provider-runtime-metrics.md`
- `docs/RELIABILITY.md`
- `docs/QUALITY_SCORE.md`
- `docs/exec-plans/tech-debt-tracker.md`
- `crates/ccr-app-core/src/metrics.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-ui/src/status_tab.rs`

### Check

Verification commands:

```sh
rg -n "endpoint test history|Endpoint test history|test history" docs
rg -n "plan-001|Request metrics|Attempt metrics|Response metrics" docs
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --workspace
```

Manual QA:

- 点击 endpoint test，确认只产生配置诊断，不进入 request history。
- 发起真实 client request，确认 request-level record 和 attempt-level record 可关联。

Observed result:

- 未完成。

### Act

Follow-up:

- 未完成后续决策。

Documentation updates:

- 未完成。

Remaining debt:

- 未完成。

## 非目标

- 不删除 endpoint test 功能。
- 不把 endpoint test 结果写入真实 request history。
- 不保存 prompt、response body、API key 或完整敏感 header。
- 不在本计划内实现外部 metrics backend。
- 不把单次 endpoint test latency 接入 Route Pool health score。
- 不立即实现完整 token 计费或成本统计。

## 背景

检查长期文档后发现：

- `docs/provider-runtime-metrics.md` 的产品边界基本正确：provider 指标必须来自真实业务请求，而不是 endpoint test。
- `docs/long-term-roadmap.md` 也写了 Endpoint test 和 Runtime metrics 必须区分。
- 偏差主要发生在执行计划和完成口径上：两个旧完成计划曾把最小 upstream attempt store 近似描述成 request history 第一阶段完成，容易让人误以为“endpoint test 历史”或“简单 attempt 列表”已经覆盖了真实响应数据采集。
- 当前实现只记录了最小 attempt 结果，缺 request-level record、request/attempt correlation、TTFT/first byte、stream chunk timing、token throughput、client app/request kind 和更细的错误分类。

这会导致后续 agent 继续把三类概念混在一起：

- endpoint test：点击按钮触发的配置探测和测速。
- attempt metrics：真实请求中某一次 upstream provider/model 尝试。
- request metrics：一个 client request 的整体最终结果，可能包含多个 attempts。

## 设计方向

### 1. 统一术语

文档中使用固定术语：

- `Endpoint test`：主动探测，只属于配置诊断。
- `Request metrics`：真实 client request 的整体记录。
- `Attempt metrics`：真实 request 内每次 upstream attempt 的记录。
- `Response metrics`：TTFT、first byte、total latency、stream duration、chunk gap、token throughput 等响应侧指标集合。
- `Request history`：真实 request/attempt 历史，不包含 endpoint test 历史。

### 2. 调整长期文档

需要更新：

- `docs/long-term-roadmap.md`
- `docs/provider-runtime-metrics.md`
- `docs/RELIABILITY.md`
- `docs/QUALITY_SCORE.md`
- `docs/exec-plans/tech-debt-tracker.md`
- `docs/exec-plans/completed/td-008-runtime-metrics-store.md`
- `docs/exec-plans/completed/td-010-request-history.md`

更新重点：

- 明确当前 runtime metrics 只是最小 attempt store，不是完整 request/response metrics。
- 把“历史请求记录”从 endpoint test 历史里彻底剥离。
- 把 Route Pool health 的输入限定为真实 traffic metrics。
- 把 endpoint test 的输出限定为配置诊断和一次性测速提示。
- 在完成计划的完成偏差里写清楚实际完成范围和未完成响应数据指标。

### 3. 调整指标模型

现有 `UpstreamAttemptMetric` 需要后续扩展或拆分：

- 添加 request id，用于关联一个 client request 内的多个 attempts。
- 添加 client app 和 inbound protocol/request kind。
- 添加 response timing 字段：first byte latency、TTFT、total latency、stream duration。
- 添加 streaming fields：stream chunk count、max chunk gap、average chunk interval。
- 添加 token fields：input/output/total token counts、tokens per second，无法可靠计算时允许为空。
- 添加 error class、retryable flag、retry reason、final request outcome。

需要新增 request-level record：

- request id。
- started_at / finished_at。
- client app。
- inbound protocol / request kind。
- requested model。
- selected route。
- final provider/model。
- final status/outcome。
- total attempts。
- end-to-end latency。

### 4. 调整 API 和 UI 边界

API 应区分：

- `/api/runtime-metrics/attempts`：真实 upstream attempts。
- `/api/runtime-metrics/requests`：真实 client requests。
- `/api/runtime-metrics/summary`：真实 traffic 聚合。

UI 应区分：

- Endpoint test 结果显示在 provider/config 诊断区域。
- Request/response history 显示在 Status、Logs 或独立 Request History 视图。
- Route Pool health 只能展示真实 request/attempt metrics 产生的健康原因。

## 修改文件

预计修改：

- `docs/long-term-roadmap.md`
- `docs/provider-runtime-metrics.md`
- `docs/RELIABILITY.md`
- `docs/QUALITY_SCORE.md`
- `docs/exec-plans/index.md`
- `docs/exec-plans/active/README.md`
- `docs/exec-plans/tech-debt-tracker.md`
- `docs/exec-plans/completed/td-008-runtime-metrics-store.md`
- `docs/exec-plans/completed/td-010-request-history.md`
- `crates/ccr-app-core/src/metrics.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-ui/src/status_tab.rs`

## 验收测试

文档验收：

```sh
rg -n "endpoint test history|Endpoint test history|test history" docs
rg -n "plan-001|Request metrics|Attempt metrics|Response metrics" docs
```

代码验收：

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --workspace
```

行为验收：

- 点击 endpoint test 只产生配置诊断结果和日志，不写入 request history。
- 一次真实 client request 至少产生 request-level record。
- 一个真实 request 内的每次 upstream retry 都有 attempt-level record，并能通过 request id 关联。
- Runtime summary 基于真实 request/attempt metrics，不使用 endpoint test 结果。
- UI 文案不会把 endpoint test latency 描述为 provider health 或真实响应质量。

## 决策日志

- 2026-05-07：计划编号改用 `plan-001` 格式，不再为新工作分配 TD 编号。
- 2026-05-07：确认 endpoint test 是点击按钮测速/配置验证；真实响应数据采集必须来自实际 client traffic。
- 2026-05-07：当前最小 attempt store 可以保留，但必须在文档中降级为第一块基础设施，而不是完整 request history。

## 完成记录

未完成。
