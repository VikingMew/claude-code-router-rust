# plan-008 - Responses Reasoning Field Mapping

**状态：** completed
**优先级：** P1
**计划编号：** plan-008
**最后更新：** 2026-05-17

## 目标

定义并实现 OpenAI Responses reasoning 字段在 CCR 三种主 API kind 之间的最小可靠映射策略。

必须回答：

- Responses 入站的 `reasoning` 字段发往 OpenAI Responses upstream 时如何保留。
- Responses 入站发往 OpenAI Chat 或 Anthropic Messages upstream 时如何降级或转换。
- Claude/Anthropic Messages 入站发往 OpenAI Responses upstream 时是否生成 `reasoning`。
- 当前 transformer 中 `reasoning` / `forcereasoning` 与 provider API kind 的关系。

## 非目标

- 不实现完整 chain-of-thought 或私有推理内容的跨协议暴露。
- 不改变模型选择和 Route Pool 语义。
- 不实现 tool 或 multimodal 映射。

## 背景

`docs/provider-api-kinds.md` 已明确 Responses reasoning 字段尚未完整支持。当前 `ccr-transformer` 有 `reasoning` 和 `forcereasoning` transformer，但这不是 Responses API reasoning 字段的完整协议策略。

如果不明确处理，真实 Codex 请求中的 reasoning 参数可能被错误透传、错误删除，或在非 Responses provider 上产生非法 body。

## 设计方向

- 先在 server upstream builder 层定义 allow/pass/drop/degrade 规则。
- 对 OpenAI Responses upstream，保留官方兼容的 `reasoning` shape。
- 对 OpenAI Chat / Anthropic Messages upstream，默认不透传未知 Responses-only reasoning 字段；如果有可验证等价字段，再显式映射。
- 在日志中记录被降级或丢弃的 protocol-only 字段，避免静默行为。
- 测试应覆盖保留、丢弃和错误 shape。

## 修改文件

- `crates/ccr-server/src/lib.rs`
- `crates/ccr-transformer/src/transformers.rs`
- `docs/provider-api-kinds.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

```sh
cargo fmt --check
cargo test --package ccr-server --lib reasoning
cargo test --package ccr-transformer --lib reasoning
```

人工验收：

- 用一个带 Responses `reasoning` 字段的 Codex 请求检查 upstream log。
- 确认 OpenAI Responses provider 保留合法 reasoning 字段。
- 确认 Anthropic/OpenAI Chat provider 不收到非法 Responses-only reasoning body。

## 预期覆盖测试用例

Server upstream builder 单元测试：

- `upstream_builder_preserves_responses_reasoning_for_responses_provider`
  - 入站：`InboundProtocol::OpenAiResponses`，body 含 `reasoning` 和 `input`。
  - 目标：`ProviderApiKind::OpenAiResponses`。
  - 断言：upstream body 保留合法 `reasoning` 字段和值。
- `upstream_builder_drops_or_degrades_responses_reasoning_for_openai_chat`
  - 入站：Responses body 含 `reasoning`。
  - 目标：`ProviderApiKind::OpenAiChat`。
  - 断言：Chat body 不含 Responses-only `reasoning` 字段；如果实现降级字段，断言降级字段名称和值。
- `upstream_builder_drops_or_degrades_responses_reasoning_for_anthropic_messages`
  - 入站：Responses body 含 `reasoning`。
  - 目标：`ProviderApiKind::AnthropicMessages`。
  - 断言：Anthropic body 不含非法 Responses-only `reasoning` 字段；如使用 Anthropic thinking 字段，必须断言字段 shape 合法。
- `upstream_builder_rejects_invalid_responses_reasoning_shape`
  - 入站：Responses body 含非法 `reasoning` 类型或结构。
  - 目标：`ProviderApiKind::OpenAiResponses`。
  - 断言：返回明确 build error 或按官方兼容策略丢弃，并记录原因。
- `upstream_builder_does_not_generate_reasoning_for_plain_anthropic_messages`
  - 入站：`InboundProtocol::AnthropicMessages`，body 无 reasoning/thinking。
  - 目标：`ProviderApiKind::OpenAiResponses`。
  - 断言：upstream body 不凭空生成 `reasoning`。

Transformer 单元测试：

- `reasoning_transformer_does_not_conflict_with_responses_reasoning_field`
  - 输入：含 Responses `reasoning` 字段和普通 messages/input。
  - 断言：transformer 不把 Responses `reasoning` 误当成 Anthropic thinking 内容删除或改写。
- `forcereasoning_transformer_only_applies_to_supported_provider_shape`
  - 输入：目标 provider kind 明确不支持时。
  - 断言：不产生非法 upstream body。

日志/诊断测试：

- 当 reasoning 字段被丢弃或降级时，app/server log 包含 request id、route、provider、字段名和处理结果。

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

- 2026-05-17：将 reasoning 从基础协议矩阵中拆出，避免 P0 修复被复杂语义阻塞。

## 完成记录

2026-05-17：完成本计划的最小可运行实现和验证。详见 Do/Check/Act。
