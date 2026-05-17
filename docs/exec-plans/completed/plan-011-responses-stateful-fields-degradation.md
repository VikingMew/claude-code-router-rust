# plan-011 - Responses Stateful Fields Degradation

**状态：** completed
**优先级：** P1
**计划编号：** plan-011
**最后更新：** 2026-05-17

## 目标

为 OpenAI Responses stateful 字段在 through-CCR 和跨 provider 转换中的行为建立明确策略。

字段包括：

- `previous_response_id`
- `conversation_id`
- `prompt`
- 其他 Responses-only 状态字段

## 非目标

- 不实现跨 provider 的长期会话存储。
- 不伪造 OpenAI Responses state semantics。
- 不把非 Responses provider 包装成真正 stateful Responses provider。
- 不实现完整 stream event 转换。

## 背景

OpenAI Responses API 允许通过 `previous_response_id`、`conversation_id` 或 `prompt` 等字段表达 stateful workflow。Anthropic Messages 和 OpenAI Chat Completions 没有完全等价的服务器端 state 语义。

当前文档已标记这些字段与非 Responses provider 的语义降级策略未完成。如果不处理，CCR 可能把 Responses-only state 字段静默丢弃或原样发给不支持的 upstream。

## 设计方向

- OpenAI Responses upstream：保留合法 stateful 字段。
- OpenAI Chat / Anthropic Messages upstream：默认拒绝仅依赖 stateful 字段且没有可展开上下文的请求。
- 如果请求同时包含完整 `input` / `messages`，可以忽略 state handle，但必须记录降级日志。
- UI/logs 中能看到 stateful 字段被保留、降级或拒绝。
- 文档明确 through-CCR 跨 provider 不承诺 OpenAI Responses state continuity。

## 修改文件

- `crates/ccr-server/src/lib.rs`
- `crates/ccr-app-core/src/logging.rs`
- `docs/provider-api-kinds.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

```sh
cargo fmt --check
cargo test --package ccr-server --lib stateful
cargo test --package ccr-app-core --lib logging
```

人工验收：

- Responses provider 收到 `previous_response_id` 时保留字段。
- Anthropic/OpenAI Chat provider 遇到仅有 state handle 且没有展开 input 的请求时返回明确错误。
- 同时有完整 input 的请求可继续转换，并在日志中标记降级。

## 预期覆盖测试用例

Server upstream builder 单元测试：

- `responses_provider_preserves_previous_response_id`
  - 入站：Responses body 含 `previous_response_id` 和 `input`。
  - 目标：OpenAI Responses provider。
  - 断言：upstream body 保留 `previous_response_id` 和 `input`。
- `responses_provider_preserves_conversation_id`
  - 入站：Responses body 含 `conversation_id` 和 `input`。
  - 目标：OpenAI Responses provider。
  - 断言：upstream body 保留 `conversation_id`。
- `responses_provider_preserves_prompt_without_input`
  - 入站：Responses body 含 `prompt`，不含 `input/messages`。
  - 目标：OpenAI Responses provider。
  - 断言：允许构建 upstream body，不强制生成 `input`。
- `chat_provider_rejects_previous_response_id_without_input`
  - 入站：仅含 `previous_response_id`。
  - 目标：OpenAI Chat provider。
  - 断言：返回明确错误，说明 non-Responses provider 不能恢复 state。
- `anthropic_provider_rejects_conversation_id_without_input`
  - 入站：仅含 `conversation_id`。
  - 目标：Anthropic provider。
  - 断言：返回明确错误。
- `chat_provider_drops_stateful_fields_when_full_input_present`
  - 入站：含 `previous_response_id` 和完整 `input`。
  - 目标：OpenAI Chat provider。
  - 断言：生成合法 `messages`；不透传 `previous_response_id`；记录降级。
- `anthropic_provider_drops_stateful_fields_when_full_messages_present`
  - 入站：含 `conversation_id` 和完整 `messages`。
  - 目标：Anthropic provider。
  - 断言：生成合法 Anthropic `messages`；不透传 `conversation_id`；记录降级。

日志/诊断测试：

- `stateful_field_degradation_is_logged_with_request_context`
  - 断言：log 包含 request id、route、provider、field、action = preserved/dropped/rejected。

Runtime 手工测试：

- Codex 发仅含 `previous_response_id` 的 follow-up，Route Pool 选中 OpenAI Responses provider 时转发成功。
- 同一请求选中 Anthropic provider 时返回明确错误，而不是向 upstream 发送空 messages。

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

- 2026-05-17：stateful Responses 语义不能被普通 body 转换静默模拟，因此拆为独立计划。

## 完成记录

2026-05-17：完成本计划的最小可运行实现和验证。详见 Do/Check/Act。
