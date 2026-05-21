# plan-007 - Provider API Kind Protocol Matrix Completion

**状态：** completed
**优先级：** P0
**计划编号：** plan-007
**最后更新：** 2026-05-17

## 目标

补齐 `docs/provider-api-kinds.md` 中三种主 API kind 的基础协议矩阵，使 Anthropic Messages、OpenAI Chat Completions 和 OpenAI Responses 在真实 runtime upstream 构建中都有明确、可测试的 body/header 转换。

必须覆盖：

- `AnthropicMessages` 入站 -> `anthropic_messages` upstream。
- `AnthropicMessages` 入站 -> `openai_chat` upstream。
- `AnthropicMessages` 入站 -> `openai_responses` upstream。
- `OpenAiResponses` 入站 -> `anthropic_messages` upstream。
- `OpenAiResponses` 入站 -> `openai_chat` upstream。
- `OpenAiResponses` 入站 -> `openai_responses` upstream。

## 非目标

- 不在本计划中实现完整 tool call/tool result 双向映射。
- 不在本计划中实现 reasoning 字段完整映射。
- 不在本计划中实现多模态完整映射。
- 不在本计划中实现 stream event 跨协议重写。
- 不改变 Route Pool-only runtime 模型。

## 背景

当前代码已覆盖最小 Responses 入站路径：

- Codex `/v1/responses` 入站顶层 `messages` 可以转为 OpenAI Responses upstream 顶层 `input`。
- Responses `input` 可以转为目标 Chat/Messages provider 的 `messages`。
- Codex `input_text` / `output_text` 可以转为 Anthropic 兼容 `text`。

但 `AnthropicMessages` 入站到 `openai_responses` upstream 仍缺明确转换，容易再次出现 Responses upstream 收到顶层 `messages` 的错误。

## 设计方向

- 在 `crates/ccr-server/src/lib.rs` 中把 inbound protocol 到 target API kind 的转换矩阵显式化。
- OpenAI Responses upstream 必须收到顶层 `input`、`previous_response_id`、`conversation_id` 或 `prompt` 之一；常规 Messages 入站应转换为 `input`。
- OpenAI Chat upstream 必须收到顶层 `messages`。
- Anthropic Messages upstream 必须收到顶层 `messages`，并使用 Anthropic headers。
- 对不支持的 body shape 返回明确 build error，而不是把非法 body 发给 upstream。

## 修改文件

- `crates/ccr-server/src/lib.rs`
- `crates/ccr-server/src/main.rs`
- `docs/provider-api-kinds.md`
- `docs/exec-plans/completed/`

## 验收测试

```sh
cargo fmt --check
cargo test --package ccr-server --lib upstream_builder
cargo test --package ccr-server --lib
```

必须新增或确认以下测试：

- Claude/Anthropic Messages 入站到 OpenAI Responses upstream 生成顶层 `input`。
- Codex/OpenAI Responses 入站到 OpenAI Responses upstream 不保留顶层 `messages`。
- OpenAI Chat upstream 使用 Bearer header 和顶层 `messages`。
- Anthropic Messages upstream 使用 `x-api-key` / `anthropic-version` 和顶层 `messages`。

## 预期覆盖测试用例

Server upstream builder 单元测试：

- `upstream_builder_keeps_anthropic_messages_for_anthropic_provider`
  - 入站：`InboundProtocol::AnthropicMessages`，body 含顶层 `messages`。
  - 目标：`ProviderApiKind::AnthropicMessages`。
  - 断言：upstream body 保留顶层 `messages`，不生成 `input`；headers 含 `x-api-key` 和 `anthropic-version`。
- `upstream_builder_converts_anthropic_messages_to_openai_chat`
  - 入站：`InboundProtocol::AnthropicMessages`。
  - 目标：`ProviderApiKind::OpenAiChat`。
  - 断言：upstream body 为 Chat Completions 顶层 `messages`；headers 使用 `Authorization: Bearer ...`；不保留 Anthropic-only top-level 字段。
- `upstream_builder_converts_anthropic_messages_to_openai_responses_input`
  - 入站：`InboundProtocol::AnthropicMessages`。
  - 目标：`ProviderApiKind::OpenAiResponses`。
  - 断言：upstream body 使用顶层 `input`，不保留顶层 `messages`；headers 使用 Bearer；model 去掉 `provider,` 前缀。
- `upstream_builder_converts_responses_input_to_anthropic_messages`
  - 入站：`InboundProtocol::OpenAiResponses`，body 含 `input: "hello"`。
  - 目标：`ProviderApiKind::AnthropicMessages`。
  - 断言：生成 `messages[0].role = "user"` 和 `messages[0].content = "hello"`；`max_output_tokens` 映射到 `max_tokens`。
- `upstream_builder_converts_responses_messages_to_openai_chat`
  - 入站：`InboundProtocol::OpenAiResponses`，body 含 Codex 顶层 `messages`。
  - 目标：`ProviderApiKind::OpenAiChat`。
  - 断言：生成 Chat Completions 顶层 `messages`；Codex `input_text` content type 不原样泄漏到不支持的 provider。
- `upstream_builder_maps_codex_messages_to_responses_input`
  - 入站：`InboundProtocol::OpenAiResponses`，body 含顶层 `messages`。
  - 目标：`ProviderApiKind::OpenAiResponses`。
  - 断言：生成顶层 `input`，删除顶层 `messages`，保留 Responses-compatible content item。
- `upstream_builder_rejects_responses_without_input_messages_or_state`
  - 入站：`InboundProtocol::OpenAiResponses`，body 不含 `input`、`messages`、`previous_response_id`、`conversation_id`、`prompt`。
  - 目标：任意 provider kind。
  - 断言：返回明确 build error，不发送 upstream。

Runtime handler 测试：

- `/v1/responses` 收到 Codex 顶层 `messages` 时，log 中 `inbound = OpenAiResponses`，attempt body 对 OpenAI Responses provider 不含顶层 `messages`。
- `/v1/messages` 收到 Claude Code 请求并选中 OpenAI Responses provider 时，attempt body 不含顶层 `messages`。

## Do / 执行记录

- 实际修改：在 `crates/ccr-server/src/lib.rs` 中实现 provider API kind aware upstream request building。
- 实际修改：新增基础协议矩阵、Responses stateful 字段、reasoning 字段降级、tool request 映射、multimodal content 映射和跨协议 streaming gate。
- 实际修改：新增 server upstream builder 单元测试覆盖本计划相关输入 shape。
- 实际偏离计划：streaming 跨协议转换未做 schema 转换，当前实现为明确拒绝 unsupported cross-protocol streaming，避免错误 passthrough。
- 中途决策：优先保证不会向 upstream 发送非法 body；复杂 streaming event 转换后续必须基于 fixture 继续扩展。

## Check / 验证与偏差

- 验证命令：`cargo fmt`。
- 验证命令：`cargo test --package ccr-server --lib`，34 个 lib tests 通过。
- 验证命令：`cargo test --package ccr-server`，server lib、main tests 和 doctests 通过。
- 验证命令：`cargo test --package ccr-transformer --lib`，29 个 tests 通过。
- 验证命令：`cargo test --package ccr-sse --lib`，9 个 tests 通过。
- 发现的偏差：完整 stream event schema 转换没有实现；当前以 build error 明确阻止跨协议 stream passthrough。
- 发现的偏差：tool 映射覆盖 request body 和 Anthropic tool_use 到 OpenAI Chat tool_calls，未实现完整 streaming tool delta 跨协议转换。

## Act / 处理与沉淀

- 已处理偏差：通过 `Cross-protocol streaming conversion is not implemented` 错误阻止不兼容 SSE schema 透传。
- 长期文档更新：`docs/provider-api-kinds.md` 记录当前已支持和未完整支持边界。
- 新增/更新技术债：`docs/exec-plans/completed/` 将本计划完成记录更新为 resolved。
- 后续计划：完整 streaming event schema 转换、streaming tool delta 和更细的 provider-specific multimodal 能力仍应作为后续增强继续拆分。

## 决策日志

- 2026-05-17：根据 Codex 真实调用缺少 `input` 的故障，将基础协议矩阵拆为 P0 独立计划。

## 完成记录

2026-05-17：完成本计划的最小可运行实现和验证。详见 Do/Check/Act。
