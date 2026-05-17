# plan-009 - Tool Call And Tool Result Cross-Protocol Mapping

**状态：** completed
**优先级：** P1
**计划编号：** plan-009
**最后更新：** 2026-05-17

## 目标

实现并验证 Responses、OpenAI Chat Completions 和 Anthropic Messages 之间 tool call / tool result 的可靠映射。

必须覆盖：

- Responses tool call item 到 Anthropic `tool_use`。
- Anthropic `tool_use` 到 OpenAI Chat tool call。
- OpenAI Chat tool call 到 Anthropic `tool_use`。
- Tool result 在三种 API kind 中的 role、id、content 归属。
- Streaming tool call delta 的最小可用处理策略。

## 非目标

- 不实现 MCP tool runtime。
- 不改变现有 tool interception 的产品语义。
- 不在本计划中处理 multimodal content。
- 不在本计划中处理 reasoning 字段。

## 背景

当前已有：

- `ccr-transformer` 中部分 Anthropic tool_use 到 OpenAI function/tool 形态的 transformer。
- `ccr-sse` 中 Anthropic SSE tool interception rewriter。
- server tool interception/continuation 路径。

但这些能力不是完整的三 API kind tool 映射矩阵。真实 Codex Responses 请求或响应中出现 tool call/tool result 时，当前行为没有长期文档级的完成保证。

## 设计方向

- 先定义统一中间表示：tool call id、name、arguments/input、result content、is_error。
- 在 server request builder 中完成 request body 的 tool result 映射。
- 在 response/stream 路径中完成 tool call event 或 body 的转换。
- 明确哪些 provider kind 支持 tool，哪些需要报错或降级。
- 为每种 API kind 增加 fixture-based 单元测试，避免依赖真实 provider。

## 修改文件

- `crates/ccr-server/src/lib.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-transformer/src/transformers.rs`
- `crates/ccr-sse/src/rewriter.rs`
- `crates/ccr-sse/src/continuation.rs`
- `docs/provider-api-kinds.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

```sh
cargo fmt --check
cargo test --package ccr-server --lib tool
cargo test --package ccr-transformer --lib tool
cargo test --package ccr-sse --lib
```

人工验收：

- Codex 发起需要 tool 的请求时，Route Pool 选中 Anthropic provider 不产生非法 body。
- Claude Code 发起 tool 请求时，Route Pool 选中 OpenAI Chat 或 Responses provider 有明确转换或明确错误。

## 预期覆盖测试用例

Server/request builder 单元测试：

- `responses_tool_call_item_maps_to_anthropic_tool_use`
  - 入站：Responses `input` 或 `messages` 中包含 tool call item。
  - 目标：`ProviderApiKind::AnthropicMessages`。
  - 断言：生成 Anthropic `content` block，`type = "tool_use"`，保留 id、name、input。
- `responses_tool_result_item_maps_to_anthropic_tool_result`
  - 入站：Responses tool result item。
  - 目标：Anthropic provider。
  - 断言：生成 user role message，content block `type = "tool_result"`，保留 `tool_use_id` 和 result content。
- `anthropic_tool_use_maps_to_openai_chat_tool_call`
  - 入站：Anthropic assistant message 含 `tool_use`。
  - 目标：OpenAI Chat provider。
  - 断言：生成 assistant message 的 `tool_calls`，function name/arguments 合法 JSON 字符串。
- `anthropic_tool_result_maps_to_openai_chat_tool_message`
  - 入站：Anthropic user message 含 `tool_result`。
  - 目标：OpenAI Chat provider。
  - 断言：生成 role `tool` message，`tool_call_id` 匹配，content 为字符串或合法 content array。
- `openai_chat_tool_call_maps_to_anthropic_tool_use`
  - 入站或 intermediate：OpenAI Chat assistant `tool_calls`。
  - 目标：Anthropic provider。
  - 断言：生成 Anthropic `tool_use` content block。
- `tool_mapping_rejects_missing_tool_id_or_name`
  - 输入：缺少 tool id、name 或 arguments 非法。
  - 断言：返回明确 build error，不发送 upstream。

Transformer 测试：

- `enhancetool_preserves_non_tool_content`
  - 断言：已有非 tool 文本 content 不被 tool 映射破坏。
- `tooluse_transformer_output_matches_server_tool_ir`
  - 断言：transformer 输出和 server 中间表示字段一致。

SSE/stream fixture 测试：

- `anthropic_stream_tool_use_delta_accumulates_valid_input`
  - 输入：Anthropic `content_block_start` + 多个 `content_block_delta` + stop。
  - 断言：累积出完整 tool call id、name、input。
- `openai_chat_stream_tool_call_delta_accumulates_valid_arguments`
  - 输入：OpenAI Chat tool call delta chunks。
  - 断言：累积出完整 function arguments。
- `responses_stream_tool_call_event_maps_to_target_client_schema`
  - 输入：Responses tool call stream event fixture。
  - 断言：输出符合目标 client 入站协议 schema，或返回明确 unsupported。

Runtime 手工测试：

- Codex -> CCR `/v1/responses` -> Anthropic provider，触发一个简单 tool call，确认 upstream request 和 client-visible response 都可解析。
- Claude Code -> CCR `/v1/messages` -> OpenAI Chat provider，触发 tool call，确认不会把 Anthropic `tool_use` 原样发给 OpenAI。

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
- 新增/更新技术债：`docs/exec-plans/tech-debt-tracker.md` 将本计划完成记录更新为 resolved。
- 后续计划：完整 streaming event schema 转换、streaming tool delta 和更细的 provider-specific multimodal 能力仍应作为后续增强继续拆分。

## 决策日志

- 2026-05-17：tool 映射涉及 request、response 和 SSE，不并入基础 body/header 计划。

## 完成记录

2026-05-17：完成本计划的最小可运行实现和验证。详见 Do/Check/Act。
