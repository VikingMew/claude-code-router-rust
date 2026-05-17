# plan-012 - Stream Event Cross-Protocol Conversion

**状态：** completed
**优先级：** P1
**计划编号：** plan-012
**最后更新：** 2026-05-17

## 目标

实现或明确限制 Anthropic Messages、OpenAI Chat Completions 和 OpenAI Responses 之间的 streaming response event 转换。

必须覆盖：

- OpenAI Responses SSE event 到 Codex 可读 event 的透传或转换。
- Anthropic Messages SSE event 到 Codex Responses client 的转换。
- OpenAI Chat streaming delta 到 Claude/Codex client 的转换策略。
- TTFT、first byte 和 stream chunk metrics 在 event 转换后的采集边界。

## 非目标

- 不实现完整 tool call 映射；该工作由 `plan-009` 处理。
- 不实现完整 multimodal 映射；该工作由 `plan-010` 处理。
- 不改变 Route Pool retry/ban 模型。

## 背景

当前 server 对 streaming upstream 多数是 passthrough，并在部分 tool interception 路径中处理 Anthropic SSE。真实 through-CCR 场景中，client 入站协议和 upstream provider API kind 可能不同；只透传 upstream SSE 可能让 client 收到不认识的 event schema。

这会影响：

- Codex 通过 `/v1/responses` 使用 Anthropic upstream。
- Claude Code 通过 `/v1/messages` 使用 OpenAI upstream。
- Tool call、finish reason、usage、reasoning summary 和 error event 的呈现。
- TTFT 和 stream metrics 的一致性。

## 设计方向

- 定义 stream event support matrix：passthrough、convert、unsupported。
- 先覆盖 text delta、message start/stop、finish reason、usage 和 error event。
- Tool、reasoning、multimodal event 只接入对应计划已完成的字段。
- 转换后仍保留 TTFT 和 chunk timing 采集。
- 不支持的跨协议 streaming 组合应返回明确错误或自动降级为 non-stream（需要明确产品决策）。

## 修改文件

- `crates/ccr-server/src/main.rs`
- `crates/ccr-sse/src/rewriter.rs`
- `crates/ccr-sse/src/continuation.rs`
- `crates/ccr-app-core/src/metrics.rs`
- `docs/provider-api-kinds.md`
- `docs/provider-runtime-metrics.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

```sh
cargo fmt --check
cargo test --package ccr-server stream
cargo test --package ccr-sse --lib
cargo test --package ccr-app-core --lib metrics
```

人工验收：

- Codex streaming 请求走 Anthropic upstream 时，client 收到可解析的 Responses-compatible stream 或明确错误。
- Claude Code streaming 请求走 OpenAI upstream 时，client 收到可解析的 Messages-compatible stream 或明确错误。
- TTFT 和 chunk metrics 仍来自真实 upstream response path。

## 预期覆盖测试用例

SSE/stream fixture 测试：

- `anthropic_text_stream_maps_to_responses_text_delta`
  - 输入：Anthropic `message_start`、`content_block_start`、`content_block_delta`、`message_stop` fixture。
  - 输出：Responses-compatible text delta event。
  - 断言：文本 delta 顺序一致，finish event 可被 Codex 解析。
- `responses_text_stream_maps_to_anthropic_content_delta`
  - 输入：Responses text delta fixture。
  - 输出：Anthropic-compatible `content_block_delta`。
  - 断言：Claude Code 可按 Messages stream schema 解析。
- `openai_chat_delta_stream_maps_to_responses_text_delta`
  - 输入：Chat Completions `choices[].delta.content` fixture。
  - 输出：Responses-compatible text delta。
  - 断言：delta text 不丢失，finish reason 映射。
- `openai_chat_delta_stream_maps_to_anthropic_content_delta`
  - 输入：Chat Completions streaming fixture。
  - 输出：Anthropic-compatible content delta。
  - 断言：message start/stop 和 content block index 合法。
- `upstream_error_event_maps_to_target_client_error`
  - 输入：Anthropic/OpenAI/Responses upstream error event。
  - 输出：目标 client schema 的 error event 或 HTTP error。
  - 断言：错误 code/message 不丢失。
- `usage_event_maps_when_supported`
  - 输入：包含 usage 的 stream 完成事件。
  - 输出：目标 client 支持的 usage/final metadata。
  - 断言：input/output token 字段不被错误交换。
- `unsupported_cross_protocol_stream_returns_explicit_error`
  - 输入：包含暂不支持 event type 的 stream。
  - 断言：返回明确 unsupported，而不是原样透传不兼容 schema。

Runtime/metrics 测试：

- `stream_conversion_records_ttft_on_first_converted_event`
  - 断言：TTFT sample 来自第一个有效 upstream chunk 或转换后的第一个有效 event，且 request id 关联正确。
- `stream_conversion_records_chunk_count_and_total_duration`
  - 断言：chunk count、first byte、total latency 不因转换丢失。
- `non_stream_fallback_records_non_stream_first_byte`
  - 如果实现 non-stream fallback，断言 first byte/total latency 仍记录，TTFT 标记为 non-stream 或等价字段。

手工测试：

- Codex `/v1/responses` streaming -> Anthropic provider，验证 Codex UI/CLI 不因 Anthropic event schema 报错。
- Claude Code `/v1/messages` streaming -> OpenAI Chat provider，验证 Claude Code 不收到 OpenAI Chat 原始 delta。

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

- 2026-05-17：stream event schema 与 request body 转换不同，单独拆 plan，避免误把 HTTP streaming passthrough 当作跨协议支持。

## 完成记录

2026-05-17：完成本计划的最小可运行实现和验证。详见 Do/Check/Act。
