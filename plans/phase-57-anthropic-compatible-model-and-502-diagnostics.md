# Phase 57 - Anthropic-Compatible Model Resolution and 502 Diagnostics

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

修正 Anthropic-compatible provider 在 router 模式下的模型解析和 502 诊断。

当前 `zenmux` direct-to-Claude-Code 写法可以工作：

```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-...",
    "ANTHROPIC_BASE_URL": "https://zenmux.ai/api/anthropic",
    "ANTHROPIC_API_KEY": "",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1"
  }
}
```

但 router 模式存在两个问题：

1. Endpoint test 在 provider 没有配置 model 时使用 fallback `model="test"`，这对 Anthropic-compatible endpoint 不成立；参考 `cc-switch`，Claude direct provider 的模型字段留空时应该不写 model env，让 Claude Code/provider 默认或请求模型继续生效。
2. Claude Code 接入 CCR 后返回 502，但 UI Logs 看不到 upstream response body 或 502 根因。

本 phase 要让 endpoint test、router request 和日志诊断都围绕真实模型和真实 upstream 错误工作。

## 当前排查结论

当前本机配置中 `zenmux` 是：

```json
{
  "name": "zenmux",
  "api_kind": "anthropic_messages",
  "api_kind_source": "inferred",
  "api_base_url": "https://zenmux.ai/api/anthropic",
  "models": [],
  "router": {
    "default": "zenmux"
  }
}
```

这会导致：

- `endpoint_test_request()` 找不到 `provider.models.first()`，于是使用 `"test"`。
- `Router.default = "zenmux"` 不是 `provider,model`。原版 CCR router 默认推荐并生成 `provider,model`，但 direct Claude provider 可以不写 model env，语义是透传 Claude Code/provider 默认模型。
- `prepare_messages_body()` 会用 `ccr_router::model_name(route)` 覆盖 Claude Code 原请求里的 model；如果 route 是 `"zenmux"`，upstream body 可能变成 `model="zenmux"`。
- Direct-to-Claude-Code 模式之所以能工作，是因为 cc-switch 不强写空模型；Claude Code 自己会带它理解的真实 model，或者 provider 使用自己的默认模型。router 模式当前把这个 model 覆盖掉了。
- Server 的 tracing log 写到 `~/.claude-code-router/logs/ccr-server.log.<date>`，而 UI Logs 读的是 `~/.claude-code-router/claude-code-router.log`。
- `send_with_failover()` 对 5xx/429 只记录 status，不读取 upstream response body；最终 502 只有简短错误，缺少 provider 返回的 body。

## 设计方向

### 1. Provider/model 配置校验

UI 和 core 需要明确要求 router route 必须能解析成真实模型。

规则：

- 推荐 route 格式继续使用 `provider,model`。
- 如果 route 只有 provider 名，例如 `"zenmux"`：
  - 不能静默把 provider name 当 model 发给 upstream。
  - 对 Claude/Anthropic Messages 入站请求，默认应该保留 inbound `body.model`，实现“空 router model 透传”。
  - 如果 provider 只有一个 configured model，可以作为 UI 建议或显式补全，但不能强制覆盖用户的 direct/default 语义。
  - 如果 provider 没有 models 且 inbound request 也没有 model，才报 `missing upstream model`。
- Provider models 为空时，Endpoint Test 不能 fallback 到 `"test"`；应允许使用配置的 Claude Code model mapping 或让用户显式输入测试模型。

### 2. Endpoint test model resolution

新增可测试 helper：

- 优先使用 `provider.models.first()`。
- 如果 provider models 为空，使用 provider kind 对应的保守测试模型：
  - Anthropic Messages / Anthropic-compatible: `claude-sonnet-4-20250514`
  - DeepSeek: `deepseek-chat`
  - OpenAI-like / Custom: `gpt-4o-mini`
- 不再使用 `"test"` 作为真实请求模型。

后续可扩展：

- 从 Router.default / scenario route 反查当前 provider 的 route model。
- 在 Endpoint Test UI 增加可选测试模型输入。

本 phase 先不要再使用 `"test"` 作为真实请求模型。

### 3. Router request model preservation / override 策略

当前 `prepare_messages_body()` 总是用 route model 覆盖入站 Claude Code body 的 `model`：

```rust
body["model"] = model_name(model_route)
```

需要改成更明确的规则：

- 如果 route 是 `provider,model`，用 route model 覆盖 inbound model。
- 如果 route 只有 provider：
  - 保留 inbound Claude Code model，实现“只指定 provider，不指定 model 时透传 model”。
  - 如果 inbound model 缺失，再考虑 provider 唯一 configured model。
  - 如果两者都没有，返回明确错误。

本 phase 采用的策略：

- `provider,model` 表示强制路由到指定 upstream model。
- `provider` 表示只切 provider，model 透传入站请求。
- 对 legacy `provider` route，如果 inbound Claude Code body 已经有 `model`，保留 inbound model，并写 info/warning log。
- 如果 inbound body 也没有 model，返回 400/502 前写清楚 `missing upstream model`。

这样能兼容 direct-to-Claude-Code 可用的 Anthropic-compatible endpoint，不会把 `zenmux` 或 `test` 发成模型。

### 4. Server upstream diagnostics log

把 server upstream attempt 的关键诊断写入 UI 能看到的应用日志：

```text
~/.claude-code-router/claude-code-router.log
```

每个 attempt 记录：

- route
- provider
- endpoint
- provider API kind
- inbound protocol
- upstream model
- request body 摘要
- redacted headers
- HTTP status
- latency
- upstream response body 摘要
- failover decision
- final 502 reason

需要覆盖：

- build upstream request 失败
- provider not found
- network error
- HTTP 429
- HTTP 5xx
- exhausted failover
- non-stream response body read error
- stream body error

### 5. 读取 retryable response body

`send_with_failover()` 当前在 5xx/429 时直接继续 failover，没有读取 body。

需要改成：

- 对 retryable status 读取 response text 摘要。
- 写入应用日志。
- last_error 包含 status + response 摘要的短版本。
- 如果没有 failover，最终 502 body 至少包含短错误。
- 不把 API key 写入日志。

注意：读取 body 后该 `response` 不能再返回给 caller，但 retryable 分支本来就不会返回 response，所以安全。

### 6. Logs UI 对齐

当前存在两类日志：

- UI app log：`~/.claude-code-router/claude-code-router.log`
- Server tracing log：`~/.claude-code-router/logs/ccr-server.log.<date>`

本 phase 先把 server upstream diagnostics 也写入 app log，保证 Logs tab 能看到。

后续可以扩展 Logs tab 显示：

- App log
- Server log
- Endpoint test log
- All merged

但本 phase 不要求完整日志浏览器。

## 非目标

- 不实现 direct-to-provider 模式。
- 不改变 through-CCR 的默认产品边界。
- 不要求完全兼容所有 Anthropic-compatible provider 的私有模型命名。
- 不实现完整 log query/filter UI。
- 不把 API key 或完整 auth token 写入日志。
- 不把无限长 response body 写进 UI。

## 实际修改文件

- `ccr-rust/crates/ccr-app-core/src/endpoint.rs`
- `ccr-rust/crates/ccr-server/src/lib.rs`
- `ccr-rust/crates/ccr-server/src/main.rs`
- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-cli/src/claude_config.rs`

## 规划的测试用例

- Provider models 为空时，endpoint test 不使用 `"test"`。
- Provider models 为空时，endpoint test 使用 provider kind 对应的保守测试模型。
- Provider models 非空时，endpoint test 使用第一个 configured model。
- `Router.default = "provider,model"` 时，server upstream body 使用 `model`。
- `Router.default = "provider"` 且 inbound Claude body 有 model 时，透传 inbound model，不把 provider name 写入 upstream model。
- `Router.default = "provider"` 且 provider models 为空、inbound model 为空时，返回明确错误。
- Claude Code model mapping 默认空值不写 env，已有旧 model env 会在激活时被删除。
- Retryable upstream 5xx/429 会读取 response body 摘要并写入 app log。
- Exhausted failover 的 502 body 包含短诊断。
- Server upstream logs redact `authorization` 和 `x-api-key`。
- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-server --lib`
- `cargo test --workspace`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- `zenmux` 这类 Anthropic-compatible endpoint test 不再发送 `model="test"`。
- 如果 provider 没有配置模型，Endpoint Test 使用 provider kind 对应的保守测试模型。
- Claude Code 经 CCR 返回 502 时，Logs tab 能看到 upstream route、endpoint、HTTP status 和 response 摘要。
- 5xx/429 不再只显示 `HTTP 500` / `HTTP 429`，而是能看到 provider 返回的错误摘要。
- 日志不包含明文 API key。
- `Router.default = "zenmux"` 这类 provider-only route 表示切 provider 并透传 inbound model，不再被静默当作 model 使用。

## 预留偏差

- 实现时没有让 `endpoint_test_request()` 直接读取 `AppSettings.claude_code_models`，因为该 helper 当前只接收 `Provider`，UI/config 两个入口都已经围绕这个签名复用。为避免扩大调用面，本 phase 使用 provider kind 的保守默认测试模型，后续如果增加“测试模型输入框”或把 config 传入 endpoint helper，再接入用户配置。
- Claude Code model mapping 的默认值改为空字符串，并在写入 Claude settings 时删除空 model env。这样更接近 `cc-switch` 的行为：用户没选模型就是没选，不强写默认模型。
- 本 phase 没有改 Logs UI 的布局；server upstream diagnostics 已写入同一个 app log 文件，所以现有 Logs tab 刷新即可看到。

## 验证结果

- `cargo test --workspace`：通过。
- `cargo llvm-cov --workspace --lib --summary-only`：通过，line coverage `81.23%`。
