# Provider Runtime Metrics

**状态：** 长期设计文档  
**范围：** 记录 provider/model 在真实请求中产生的运行时指标、聚合窗口和 UI 使用方式
**最后验证：** 2026-05-21

## 产品边界

Provider 指标必须来自真实业务请求，而不是只来自 endpoint test。

Endpoint test 只能说明“某个 endpoint 在一次探测中是否可用、探测延迟是多少”。真实 provider 质量需要在 Claude Code、Codex、OpenCode、OpenClaw 等 client 请求经过 CCR server 时持续采集。

当前实现已经开始记录真实 request/attempt history，并用 request id 关联一个 client request 内的 upstream attempts。TTFT sliding window 已基于真实 API 调用采集；stream chunk timing、token throughput、长期窗口聚合和外部 sink 仍属于后续工作。

指标归属对象是：

- provider
- endpoint
- provider API kind
- upstream model
- route
- client app
- request kind，例如 Messages 或 Responses

其中 provider/model 是主要查询维度，endpoint、client app 和 request kind 是诊断维度。

## 核心指标

每一次真实 upstream attempt 都需要记录以下指标：

### 识别维度

- request start time
- request id
- selected route
- provider
- endpoint
- upstream model
- client app
- request kind

### 请求生命周期

- queue wait time
- route selection time
- request build time
- upstream connect latency
- TLS handshake latency
- request upload duration
- first byte latency
- TTFT
- total latency
- stream duration
- output completion time

### 错误和结果

- HTTP status 或 network error kind
- error class
- error retryability
- provider error code
- provider error type
- final attempt outcome
- final request outcome
- retry attempt index
- retry reason
- ban decision

### Token 和吞吐

- input token count
- output token count
- total token count
- cached input token count
- reasoning token count
- tokens per second
- output tokens per second
- stream chunk count
- average stream chunk interval
- max stream chunk gap

### 协议和响应形态

- streaming enabled
- response content type
- upstream request bytes
- upstream response bytes
- tool call count
- tool result count
- response finish reason
- response truncated flag

TTFT 指从 CCR 开始向 upstream 发起请求，到收到第一个有效 response chunk / SSE event / body byte 的时间。

非 stream 请求也需要记录 first byte latency；在 UI 中可以和 TTFT 分开展示，或标记为 non-stream first response latency。

## 指标分类

TTFT 只是指标之一。Provider 质量需要从多组指标共同判断。

### 可用性指标

- success rate
- error rate
- retryable error rate
- timeout rate
- rate limit rate
- auth failure rate
- server error rate
- consecutive failures
- ban count
- ban duration

### 延迟指标

- queue wait time
- route selection time
- request build time
- connect latency
- TLS handshake latency
- first byte latency
- TTFT
- p50 / p90 / p95 / p99 total latency
- p50 / p90 / p95 / p99 TTFT
- stream completion latency

### 流式质量指标

- stream chunk count
- average chunk interval
- max chunk gap
- tokens per second
- output tokens per second
- time to first meaningful token
- stream interruption rate
- incomplete stream rate

### 吞吐指标

- requests per minute
- attempts per minute
- input tokens per minute
- output tokens per minute
- total tokens per minute
- concurrent in-flight requests
- retry amplification ratio

### 路由和重试指标

- selected route count
- skipped route count
- retry count
- retry reason distribution
- fallback depth
- final selected provider after retry
- route pool ban/unban events

### 协议诊断指标

- transform failure count
- unsupported request shape count
- tool call mapping failure count
- malformed stream event count
- response parse failure count
- model mismatch count
- provider returned non-JSON count

## Attempt 与 Request 的关系

Route Pool 会让一个 client request 产生多个 upstream attempts。

指标模型必须区分：

- attempt metrics：每个 provider/model 尝试的真实表现。
- request metrics：一个 client request 的最终表现。

示例：

- 第一个 provider 429，耗时 300ms。
- 第二个 provider 成功，TTFT 900ms，总耗时 8s。

这时：

- 第一个 provider 记录一次 failed attempt。
- 第二个 provider 记录一次 successful attempt。
- 整个 request 记录最终成功，并记录经历过 retry。

Route Pool 的健康判断只能使用 attempt metrics，不能只看最终 request outcome。

## 错误率

错误率是一等指标，需要在 attempt 和 request 两层分别记录。

Attempt error rate：

```text
failed upstream attempts / total upstream attempts
```

Request error rate：

```text
failed client requests / total client requests
```

Route Pool、provider health 和 provider 排序应主要使用 attempt error rate，因为它反映每个 provider/model 被真实尝试时的质量。用户体验概览可以同时展示 request error rate，因为它反映 client 最终看到的失败率。

错误需要分类保存：

- network：DNS、connect、TLS、timeout、connection reset
- rate_limit：HTTP 429
- auth：HTTP 401 / 403
- server：HTTP 5xx
- client_request：HTTP 400 / 404 / 422 等由请求格式或模型名引起的问题
- transform：本地协议转换、body 构建或 stream 解析失败
- cancelled：client 取消或断开连接
- unknown：无法归类的错误

每个错误分类都需要单独的 count 和 rate：

- total error rate
- retryable error rate
- network error rate
- rate limit rate
- auth error rate
- server error rate
- client request error rate
- transform error rate

错误率分母必须显示请求量，避免少量样本造成误导。例如 `1 / 1 = 100%` 和 `100 / 100 = 100%` 在 UI 上不能被解释成同等置信度。

## 聚合窗口

UI 和 route policy 至少需要支持这些窗口：

- 1m
- 5m
- 15m
- 1h
- 24h
- 7d
- 30d

窗口指标包括：

- request count
- success count
- failure count
- error rate
- error count by class
- error rate by class
- retryable error rate
- p50 / p90 / p95 total latency
- p50 / p90 / p95 TTFT
- p50 / p90 / p95 first byte latency
- p50 / p90 / p95 tokens per second
- average TTFT
- average total latency
- tokens per second
- stream interruption rate
- average output tokens
- ban count
- retry count

5m 窗口用于短期健康和 Route Pool 决策。

30d 窗口用于趋势、长期质量比较和用户手动调整 provider 顺序。

## 趋势数据

长期走势需要 bucket 化保存，而不是每次 UI 打开都扫描全部原始请求。

建议保留这些 bucket 粒度：

- 1m bucket：用于最近 1h 到 24h 的短期图表。
- 1h bucket：用于最近 7d 到 30d 的趋势图。
- 1d bucket：用于更长周期的可用性和吞吐统计。

每个 bucket 按 provider/model/endpoint/request kind 聚合。

原始 attempt 事件需要有保留周期；聚合后的 bucket 可以保留更久。

## UI 设计要求

Provider 页面需要展示每个 provider/model 的真实运行质量：

- 当前 5m 平均 TTFT
- 当前 5m p95 first byte latency
- 当前 5m output tokens per second
- 当前 5m 成功率
- 当前 5m 错误率
- 当前 5m 错误分类分布
- 当前 5m p95 latency
- 当前 5m 请求量
- 30d TTFT 趋势
- 30d 成功率趋势
- 30d 错误率趋势
- 30d tokens per second 趋势
- 最近错误样本

Router 页面需要把这些指标用于 Route Pool 判断：

- 每个 active route 显示近期 TTFT、成功率和 ban 状态。
- 每个 active route 显示近期错误率和主要错误分类。
- Move / reorder 决策时能看到 provider 的真实健康指标。
- 被 ban 的 route 需要显示原因、剩余时间和最近失败样本。
- Route Pool 不应该只依赖用户手动排序，还要能展示运行时健康对实际选择的影响。

Status 页面需要展示 live 概览：

- 当前活跃 provider
- 最近一次 request 的 provider/model/latency/TTFT/outcome
- 当前 5m 成功率
- 当前 5m 错误率
- 当前 5m p95 TTFT
- 当前 ban 数量

Logs 页面需要能跳转到相关 request / attempt 记录。

## 数据存储要求

指标不应该只写普通文本日志。需要结构化存储，支持查询和聚合。

最低要求：

- append-only attempt event
- request summary event
- rolling aggregate bucket
- schema version
- 数据清理策略

指标采集需要支持多个 sink：

- local metrics store：默认启用，用于离线 UI、Route Pool 和本机诊断。
- PostHog：可选，用于产品级事件分析、趋势和 cohort 分析。
- OpenTelemetry-compatible backend：可选，用于接入 Prometheus、Grafana、Jaeger、Tempo、ClickHouse 等开源观测方案。
- file export：可选，用于用户手动导出和离线分析。

外部 sink 必须是显式 opt-in，不能默认上传。

local metrics store 是 Route Pool 决策的权威来源。PostHog 或其他外部 sink 只能用于分析和展示，不能成为本地请求路由的唯一依赖。

## 当前本地存储边界

当前实现把真实流量 metrics 存在本机 JSONL 文件中：

- attempt metrics：`~/.claude-code-router/runtime-metrics.jsonl`
- request metrics：`~/.claude-code-router/runtime-request-metrics.jsonl`

两个 metrics 文件默认最大 5 MiB。启动读取和 append 后会执行 fail-open retention：如果文件超过上限，CCR 会保留可解析的最近 JSONL 记录并丢弃坏行；如果 retention 或读取失败，server 仍然启动并继续使用已读取到的最近有效内存窗口。诊断结构会记录文件路径、读取行数、成功行数、坏行数、最近解析错误摘要、文件大小、retention 是否执行和保留行数，并通过受控 runtime metrics diagnostics API 供 status/UI 读取。

本地 app log 和 server tracing log 是独立诊断数据：

- app log：`~/.claude-code-router/claude-code-router-YYYY-MM-DD.log`
- server tracing log：`~/.claude-code-router/logs/ccr-server.log.YYYY-MM-DD`

app log 由受控 logs API 查询或清空；server tracing log 使用 daily rolling 文件。metrics JSONL 的 retention 不清理 app log，logs API 的清空也不清理 metrics JSONL。

默认不会上传、导出或同步这些本地 metrics 和日志。任何外部 sink、file export 或诊断包都必须由用户显式触发或配置，并继续遵守不保存 API key、完整 prompt 和完整 response 的边界。

敏感信息不能进入指标事件：

- API key
- auth token
- full prompt
- full response
- tool input body

允许保存用于诊断的非敏感字段：

- provider name
- endpoint host
- model name
- status code
- error class
- error retryability
- latency values
- token counts
- route name

## 外部指标后端

PostHog 和开源观测后端都应该通过统一 metrics sink 接口接入。

Sink 接口需要支持：

- attempt event
- request summary event
- aggregate bucket event
- provider health event
- route decision event
- error sample event

PostHog event 适合记录产品分析维度：

- provider/model 使用量
- route pool 选择结果
- 5m/30d 健康趋势摘要
- 错误分类分布
- UI 页面中用户如何查看和调整 provider

OpenTelemetry-compatible 后端适合记录工程观测维度：

- span：client request
- span：upstream attempt
- metric：TTFT histogram
- metric：latency histogram
- metric：error counter by class
- metric：request counter by provider/model
- metric：token counter

所有外部 sink 都必须经过 redaction：

- 不上传 prompt。
- 不上传 response。
- 不上传 tool input。
- 不上传 API key、auth token、完整 headers。
- endpoint 默认只上传 host，不上传包含 token 或 query 的完整 URL。
- project path、username、repo name 等本地身份信息默认不上传。

外部 sink 配置需要支持：

- enable / disable
- endpoint / host
- API key 或 write key
- sampling rate
- allowed event types
- redaction preview
- test connection
- flush interval
- failure retry policy

外部 sink 失败不能影响主请求路径。失败只记录本地诊断日志，并在 UI 中显示 telemetry sink 状态。

## 隐私和采样

指标采集默认只保存在本地。

外部上传需要用户明确开启，并且 UI 必须解释会上传哪些字段。

采样策略：

- error event 默认全量保留本地。
- success attempt 可以按比例上传外部 sink。
- aggregate bucket 可以全量上传，因为不含单次请求细节。
- 低请求量 provider/model 不应被过度采样，否则会破坏错误率和 TTFT 判断。

Route Pool 决策使用本地全量指标，不使用外部 sink 采样后的数据。

## Route Pool 使用方式

Route Pool 可以使用短窗口指标辅助选择 provider，但必须可解释。

可用信号：

- 5m error rate
- 5m retryable error rate
- 5m error rate by class
- 5m p95 TTFT
- consecutive failures
- active ban state
- recent HTTP 429 / 5xx
- recent network errors

不可用或需要谨慎使用的信号：

- 单次 endpoint test latency
- 单次成功请求
- 长期平均值覆盖短期故障
- 缺少请求量时的虚假高成功率

健康策略需要显示原因，例如：

- skipped because banned for 42m
- deprioritized because 5m error rate is 60%
- selected because previous candidates are unavailable

## 指标与 Endpoint Test 的关系

Endpoint test 是主动探测，runtime metrics 是真实请求观测。

二者都应该保留，但不能混用：

- Endpoint test 用于配置前验证 endpoint、headers、payload 和 stream 支持。
- Runtime metrics 用于真实 provider 质量、Route Pool 健康和趋势判断。
- Request history 只记录真实 client request 和 upstream attempts，不记录 endpoint test。
- TTFT window 只使用真实 API 调用产生的 samples，不使用 endpoint test latency。
- Endpoint test 结果可以作为初始提示，但不能替代真实请求指标。

## 设计原则

- 指标以真实 upstream attempt 为准。
- TTFT 必须在 provider/model 被真实调用时计算。
- 错误率必须区分 attempt error rate 和 request error rate。
- 错误率必须保留错误分类和分母样本量。
- Route Pool 决策必须能追溯到指标和具体失败原因。
- UI 优先展示 5m 健康和 30d 趋势。
- 原始请求内容不进入指标存储。
- 指标系统服务 provider 选择和问题诊断，不服务 prompt 记录。
