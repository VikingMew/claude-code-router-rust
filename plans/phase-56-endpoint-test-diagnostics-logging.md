# Phase 56 - Endpoint Test Diagnostics Logging

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

Endpoint Speed Test 现在只在 UI 表格里显示简短结果，例如：

```text
HTTP 400
HTTP 429
HTTP 500
NetworkError
Timeout
```

这些信息不足以判断 provider endpoint 为什么失败。本 phase 要给 endpoint test 增加可诊断日志，让用户能从日志里看到每次测速的关键上下文、请求协议类型、HTTP 状态、响应摘要和失败原因，同时不能泄漏 API key。

## 背景

当前测速逻辑在 `ccr-app-core::endpoint::test_endpoint` 中执行：

- 根据 provider API kind 生成 test payload
- 发 blocking `reqwest` 请求
- UI 只展示 provider、endpoint、status、latency、stream 状态和 `error`

失败时 `EndpointTestResult.error` 主要是 `HTTP <status>` 或 reqwest error。对下面这些情况很难继续判断：

- `HTTP 400`：payload 协议不匹配、模型名错误、字段不支持、endpoint path 错误
- `HTTP 429`：rate limit、quota、并发限制、账号限制
- `HTTP 500`：provider 内部错误、proxy/adapter 错误、上游模型不可用
- 同一个 provider 有多个 endpoint 时，不清楚每个 endpoint 实际使用了哪种 test mode 和 model

## 设计方向

### 1. 增加 endpoint test 专用诊断日志

Endpoint test 每次执行都写日志，建议写到现有应用日志路径：

```text
~/.claude-code-router/claude-code-router.log
```

如果项目后续统一日志位置，可以迁移到同一 logger，但本 phase 先保证 UI Logs tab 能读到这些诊断内容。

每次测速至少记录：

- timestamp
- provider name
- endpoint
- resolved provider API kind
- endpoint test mode
- model
- request body 摘要
- request headers 摘要
- latency
- final status
- HTTP status code
- response body 摘要
- reqwest/network error message
- stream check result

### 2. 敏感信息处理

日志不能写入 API key 或完整 auth token。

规则：

- `authorization` 只记录 `Bearer <redacted>`
- `x-api-key` 只记录 `<redacted>`
- 其他 header 正常记录
- request body 可以记录完整测试 payload，因为当前 payload 只包含 model、短 prompt 和 token 字段
- response body 只记录截断摘要，建议最多 2KB
- 网络错误只记录错误字符串，不记录环境变量

### 3. HTTP 响应摘要

非 2xx 响应需要读取 body 文本并写入日志。

日志示例：

```text
[endpoint-test] result provider=glm endpoint=https://open.bigmodel.cn/api/... kind=openai_chat mode=openai_chat model=glm-4.6 status=http_error http_status=429 latency_ms=106 response="{\"error\":{\"message\":\"rate limit\"...}}"
```

成功响应也应记录简短摘要：

```text
[endpoint-test] result provider=siliconflow endpoint=https://api.siliconflow.cn/... status=available http_status=200 latency_ms=725 stream_available=true response="{...}"
```

### 4. Stream check 也需要日志

当前成功后会调用 `check_stream_endpoint`，但 stream 失败不会解释原因。

Stream check 应记录：

- provider
- endpoint
- stream request mode
- HTTP status
- response 摘要
- reqwest error
- inferred `stream_available`

这样可以区分“普通请求可用但 streaming 不可用”和“stream request payload 不被支持”。

### 5. UI 行为

本 phase 不要求把完整响应体塞进 Endpoint Test 表格。表格仍保持短状态，避免 UI 过载。

UI 需要保留或增强：

- Endpoint Test 表格继续显示简短 status
- Logs tab 可以刷新看到诊断日志
- 如果已有 `error` 字段，可以把非 2xx 响应摘要的第一行放进去，但完整诊断仍以日志为准
- UI 触发测速、保存候选 endpoint、应用 failover、刷新日志、清空日志时也要写 UI-side log
- UI-side log 需要记录 action、selected provider、结果数量、status message 等轻量上下文
- UI-side log 不记录 API key，也不重复写完整 HTTP response body

这样可以区分：

- endpoint 请求确实失败
- endpoint 请求成功但 UI 状态没有刷新
- UI 操作没有触发测速
- Logs tab 没有刷新到最新内容

### 6. UI-side 日志事件

Endpoint Test UI 至少记录：

- `run_selected_clicked`
- `run_all_clicked`
- `add_candidate`
- `remove_candidate`
- `save_candidates`
- `apply_failover_requested`
- `apply_failover_confirmed`
- `apply_failover_cancelled`
- `apply_failover_failed`

Logs UI 至少记录：

- `logs_refresh_clicked`
- `logs_clear_clicked`
- `logs_load_failed`

### 7. 日志格式

短期可以使用单行文本日志，方便直接在 Logs tab 搜索：

```text
2026-05-01T12:00:00+08:00 [endpoint-test] start provider=... endpoint=... kind=... mode=... model=...
2026-05-01T12:00:01+08:00 [endpoint-test] result provider=... endpoint=... status=... http_status=... latency_ms=... response=...
```

如果实现上更容易，也可以写 JSONL：

```json
{"target":"endpoint-test","event":"result","provider":"glm","status":"http_error","http_status":429}
```

但必须保证 Logs tab 里可读。

## 非目标

- 不改变 endpoint test 的可用性判断规则
- 不新增 provider 自动修复逻辑
- 不把完整响应体展示到 UI 表格
- 不记录 API key
- 不实现独立日志查询/过滤 UI
- 不修改 server request/failover logging

## 预计修改文件

- `ccr-rust/crates/ccr-app-core/src/endpoint.rs`
- `ccr-rust/crates/ccr-app-core/Cargo.toml`
- `ccr-rust/crates/ccr-app-core/src/logging.rs`
- `ccr-rust/crates/ccr-ui/src/endpoint_test_tab.rs`
- `ccr-rust/crates/ccr-ui/src/logs_tab.rs`

## 规划的测试用例

- ✅ HTTP 400/429/500 时，`EndpointTestResult.error` 包含 HTTP status 和截断 response 摘要。
- ✅ 非 2xx response body 会被截断，不超过设定长度。
- ✅ `authorization` 和 `x-api-key` 在日志摘要中被 redacted。
- ✅ invalid URL 不发网络请求，但写入 endpoint-test 失败日志。
- ✅ network error / timeout 写入 endpoint-test 失败日志。
- ✅ stream check 成功时写入 stream result 日志。
- ✅ stream check 非 2xx 或网络失败时写入失败原因。
- ✅ 日志 helper 在日志目录不存在时自动创建目录。
- ✅ UI 触发 Run selected / Run all 时写入 UI-side log。
- ✅ Logs tab Refresh / Clear 写入 UI-side log。
- ✅ Clear logs 后保留一条 clear action log，避免“清空后完全无迹可查”。
- ✅ `cargo test --package ccr-app-core --lib` 通过。
- ✅ `cargo test --workspace` 通过。
- ✅ `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- 用户在 Endpoint Test 后打开 Logs tab，可以看到每个 provider endpoint 的诊断日志。
- 对 `HTTP 400` 可以看到 provider 返回的错误摘要。
- 对 `HTTP 429` 可以看到 provider 返回的限流/额度类摘要，如果 provider 有返回。
- 对 `HTTP 500` 可以看到 provider 返回的错误摘要。
- 日志里不出现明文 API key。
- UI 表格仍保持简洁，不因为长响应体撑坏布局。

## 预留偏差

- 原计划用本地 loopback HTTP server 覆盖真实 HTTP 400/200 分支，但当前 sandbox 禁止测试进程绑定本地端口；最终改为把 response summary、result builder、stream 判断和日志格式拆成纯函数测试。
- Runtime 仍会读取真实 HTTP response body 并写入诊断日志；测试层只避免依赖本地 socket。

## 验证结果

- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --package ccr-ui`
- `cargo llvm-cov --workspace --lib --summary-only`

最终 line coverage：80.75%。
