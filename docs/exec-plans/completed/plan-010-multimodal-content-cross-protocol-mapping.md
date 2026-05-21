# plan-010 - Multimodal Content Cross-Protocol Mapping

**状态：** completed
**优先级：** P1
**计划编号：** plan-010
**最后更新：** 2026-05-17

## 目标

定义并实现 text/image/file/audio 等多模态 content 在 Anthropic Messages、OpenAI Chat Completions 和 OpenAI Responses 之间的最小可靠映射。

必须覆盖：

- Text content 的 array/string 规范化。
- Image URL 和 base64 image 的跨协议映射。
- 不支持的 file/audio/video content 的明确错误或降级策略。
- Endpoint test 与真实 multimodal 请求的边界。

## 非目标

- 不实现图像生成或图像处理能力。
- 不实现 provider-specific 文件上传 API。
- 不在本计划中实现 tool 或 reasoning 映射。
- 不改变 Route Pool 模型。

## 背景

当前实现主要面向 text content。`normalize_message_content` 只处理 Codex `input_text` / `output_text` 到 Anthropic 兼容 `text` 的最小转换。

如果真实请求包含 image/file/audio content，CCR 可能把目标 provider 不认识的 content block 原样发出，或把信息丢失为 text-only 请求。

## 设计方向

- 建立 content block 支持矩阵，明确每个 API kind 的合法 type。
- 对 text 做无损转换。
- 对 image 做有限支持：OpenAI image_url / Responses image input 与 Anthropic image source 之间转换。
- 对 file/audio/video 先返回明确 unsupported error，除非找到可验证的等价映射。
- 日志记录被降级或拒绝的 content type。

## 修改文件

- `crates/ccr-server/src/lib.rs`
- `crates/ccr-transformer/src/transformers.rs`
- `docs/provider-api-kinds.md`
- `docs/exec-plans/completed/`

## 验收测试

```sh
cargo fmt --check
cargo test --package ccr-server --lib multimodal
cargo test --package ccr-transformer --lib multimodal
```

人工验收：

- 文本请求仍不受影响。
- 图片请求在支持矩阵内转换成功。
- 不支持 content type 返回明确错误，不发非法 upstream 请求。

## 预期覆盖测试用例

Server/request builder 单元测试：

- `text_content_string_and_array_round_trip_for_anthropic_provider`
  - 输入：string content 和 text block content。
  - 目标：Anthropic provider。
  - 断言：文本内容不丢失，role 顺序不变。
- `codex_input_text_maps_to_anthropic_text`
  - 输入：Codex content block `type = "input_text"`。
  - 目标：Anthropic provider。
  - 断言：输出 block `type = "text"`，text 值不变。
- `anthropic_image_source_maps_to_openai_chat_image_url_or_supported_shape`
  - 输入：Anthropic image source block。
  - 目标：OpenAI Chat provider。
  - 断言：生成 OpenAI 支持的 image content shape；media type 和 base64/url 不丢失。
- `openai_chat_image_url_maps_to_anthropic_image_source`
  - 输入：OpenAI Chat image_url block。
  - 目标：Anthropic provider。
  - 断言：生成 Anthropic image block；URL/base64 和 media type 合法。
- `responses_image_input_maps_to_anthropic_image_source`
  - 输入：Responses image input item。
  - 目标：Anthropic provider。
  - 断言：生成 Anthropic image source block。
- `anthropic_image_source_maps_to_responses_image_input`
  - 输入：Anthropic image source block。
  - 目标：OpenAI Responses provider。
  - 断言：生成 Responses-compatible image input item。
- `unsupported_audio_content_returns_build_error`
  - 输入：audio content。
  - 目标：不支持 audio 的 provider kind。
  - 断言：返回明确 unsupported content type 错误。
- `unsupported_file_content_returns_build_error`
  - 输入：file content。
  - 目标：无 file upload 等价能力的 provider kind。
  - 断言：返回明确 unsupported content type 错误。

Transformer 测试：

- `openai_transformer_preserves_image_content_blocks`
  - 断言：OpenAI transformer 不把 image content 丢成空字符串。
- `anthropic_transformer_preserves_image_source_blocks`
  - 断言：Anthropic transformer 不破坏 image source。

日志/诊断测试：

- 不支持的 multimodal content 被拒绝时，log 包含 request id、route、provider、content type 和错误原因。

回归测试：

- 现有纯文本 Claude Code 和 Codex 请求的 body 与 plan 执行前保持等价。

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

- 2026-05-17：多模态 content 需要单独支持矩阵，不能只靠 text normalization 视为完成。

## 完成记录

2026-05-17：完成本计划的最小可运行实现和验证。详见 Do/Check/Act。
