# Provider API Kinds

**状态：** 长期设计文档  
**范围：** 记录 CCR 支持的 provider API kind、client 入站协议、upstream body/header 转换和 client injection 边界  
**最后验证：** 2026-05-17

## 产品边界

CCR 默认工作在 through-CCR 模式。

- Client 只连接本地 CCR server。
- Route Pool 选择真实 upstream provider 和 model。
- Provider API kind 只影响 server 发往 upstream 的 body、headers、endpoint test 和 transformer recommendation。
- Provider API kind 不应该泄漏到默认 client injection 语义中。

这意味着 Claude Code、Codex、OpenCode、OpenClaw 的配置注入只负责把 client 指向本地 CCR。它不应该因为当前 Route Pool 选择了 Anthropic、OpenAI Chat 或 OpenAI Responses provider，就改变默认 client 注入策略。

## 三种主 API 形态

当前长期模型只把以下三类作为一等 provider API kind。

| API kind | 配置值 | 常见 endpoint | 顶层 body | 认证/header |
| --- | --- | --- | --- | --- |
| Anthropic Messages | `anthropic_messages` | `/v1/messages` | `messages` | `x-api-key` + `anthropic-version` |
| OpenAI Chat Completions | `openai_chat` | `/v1/chat/completions` | `messages` | `Authorization: Bearer ...` |
| OpenAI Responses | `openai_responses` | `/v1/responses` | `input` | `Authorization: Bearer ...` |

不要把 OpenAI Chat Completions 简写为 OpenAI Messages。`messages` 是 Chat Completions 的请求字段，不是独立 API 名称。

## Client 入站协议

### Claude Code

Claude Code 通过本地 CCR `/v1/messages` 进入。

```text
Claude Code
  -> http://127.0.0.1:<port>/v1/messages
  -> InboundProtocol::AnthropicMessages
```

入站 body 是 Anthropic Messages 形态。CCR 在 Route Pool 选中 provider 后，再按目标 provider API kind 构建 upstream 请求。

### Codex

Codex 通过本地 CCR `/v1/responses` 进入。

```text
Codex
  -> http://127.0.0.1:<port>/v1/responses
  -> InboundProtocol::OpenAiResponses
```

Codex provider config 使用：

```toml
model_provider = "ccr"

[model_providers.ccr]
base_url = "http://127.0.0.1:<port>/v1"
wire_api = "responses"
requires_openai_auth = true
```

Codex 的真实请求可能带 Responses endpoint，但 body 中仍出现 `messages`。这是 client 入站兼容问题，不代表 upstream OpenAI Responses 可以接受顶层 `messages`。

## Upstream 转换规则

CCR 的转换方向由两个变量决定：

- 入站协议：`AnthropicMessages` 或 `OpenAiResponses`
- 目标 provider API kind：`anthropic_messages`、`openai_chat`、`openai_responses`

### Codex / Responses 入站

```text
Codex /v1/responses
  -> Route Pool selected provider
  -> build upstream request by provider API kind
```

如果目标 provider 是 `openai_responses`：

- 顶层 `input` 保留。
- 如果入站只有顶层 `messages`，CCR 必须转换为顶层 `input`。
- 不应把顶层 `messages` 原样发给 OpenAI Responses upstream。

如果目标 provider 是 `openai_chat`：

- `input` 或入站 `messages` 转成 Chat Completions `messages`。
- 使用 Bearer auth。
- 应用 OpenAI/chat 相关 transformer。

如果目标 provider 是 `anthropic_messages`：

- `input` 或入站 `messages` 转成 Anthropic Messages `messages`。
- Codex content item 类型 `input_text` / `output_text` 需要规范化为 Anthropic 兼容的 `text`。
- 使用 `x-api-key` 和 `anthropic-version`。

### Claude Code / Messages 入站

如果目标 provider 是 `anthropic_messages`：

- 保留 Anthropic Messages 语义。
- 根据 route 决定 upstream model。

如果目标 provider 是 `openai_chat`：

- 转成 Chat Completions `messages`。
- 使用 Bearer auth。

如果目标 provider 是 `openai_responses`：

- 转成 Responses `input`。
- 使用 Bearer auth。

## Endpoint Test 与真实请求

Endpoint test 是主动测速和配置验证。它应该按 provider API kind 生成最小合法请求：

- `anthropic_messages` 生成 Messages body。
- `openai_chat` 生成 Chat Completions body。
- `openai_responses` 生成 Responses body，顶层字段是 `input`。

真实请求转换不能从 endpoint test 推断完成。Codex、Claude Code 和其他 client 的真实请求可能包含更复杂的 tool、reasoning、multi-modal、stream 和历史上下文字段。真实请求路径必须有独立单元测试和运行时日志。

## 当前实现边界

当前实现支持最小 body/header 转换：

- Responses `input: string` 转为目标 provider messages。
- Responses `input: array` 转为目标 provider messages。
- Codex Responses 入站的顶层 `messages` 转为 OpenAI Responses 顶层 `input`。
- Codex `input_text` / `output_text` content item 转为 Anthropic 兼容 `text`。
- Claude/Anthropic Messages 入站可以转为 OpenAI Chat `messages` 或 OpenAI Responses `input`。
- Anthropic image source 与 OpenAI Chat/Responses image content 有最小映射。
- Responses stateful 字段在 OpenAI Responses upstream 中保留；对非 Responses provider，缺少完整 input/messages 的 state-only 请求会被拒绝。
- 跨协议 streaming 当前明确拒绝，避免把 upstream 不兼容 SSE schema 原样透传给 client。
- `max_output_tokens` 转为 Chat/Messages provider 的 `max_tokens`。
- Route 中的 `provider,model` 会覆盖 inbound model。
- Provider-only route 使用 inbound model 或 provider 唯一 model。

尚未视为完整完成：

- Responses reasoning 字段的完整映射。
- Responses tool call / tool result 的完整双向映射。
- 多模态 content 的完整跨协议映射，尤其 file/audio/video 和 provider-specific 文件能力。
- OpenAI Responses stateful 字段如 `previous_response_id`、`conversation_id` 与非 Responses provider 的语义连续性；当前只支持保留或明确拒绝。
- stream event 的完整跨协议转换。

这些缺口应进入 execution plan 或 tech debt，而不是在长期文档里宣称已支持。

## 代码入口

- `crates/ccr-types/src/lib.rs` - `ProviderApiKind` 配置枚举。
- `crates/ccr-app-core/src/provider_kind.rs` - provider API kind 推断、默认 endpoint test mode 和 transformer recommendation。
- `crates/ccr-server/src/lib.rs` - inbound protocol 到 upstream request 的 body/header 构建。
- `crates/ccr-server/src/main.rs` - `/v1/messages` 和 `/v1/responses` runtime 入口。
- `crates/ccr-app-core/src/client_config/codex.rs` - Codex through-CCR provider injection。
