# Completed Execution Plans

**状态：** completed index
**最后验证：** 2026-05-17

Completed execution plans live here.

Rules:

- Preserve completion evidence.
- Preserve verification commands and results.
- Preserve the final execution record: `Do / 执行记录`, `Check / 验证与偏差` and
  `Act / 处理与沉淀`.
- Keep old decisions for future agent navigation.
- Do not rewrite completed plans into current architecture docs; update long-term docs separately when behavior changes.

Historical top-level phase files were migrated here as `legacy-phase-*.md`.

## Recent Plans

- `plan-005-long-term-docs-current-state-and-direction-alignment.md` - aligns long-term docs with current code and removes the resolved centralized debt index.
- `ccr-15-claude-codex-path-overrides.md` - wires Claude/Codex client path overrides through app-core status and injection operations.
- `plan-013-logs-tab-progressive-reverse-loading.md` - makes Logs tab load latest logs first, progressively load older entries and avoid UI blocking on large log files.
- `plan-012-stream-event-cross-protocol-conversion.md` - blocks unsupported cross-protocol streaming instead of passing incompatible SSE schemas through.
- `plan-011-responses-stateful-fields-degradation.md` - preserves Responses state for Responses upstream and rejects state-only requests for non-Responses providers.
- `plan-010-multimodal-content-cross-protocol-mapping.md` - adds text/image content mapping and explicit unsupported content errors.
- `plan-009-tool-call-tool-result-cross-protocol-mapping.md` - maps Anthropic tool_use request blocks to OpenAI Chat tool_calls for non-stream request bodies.
- `plan-008-responses-reasoning-field-mapping.md` - preserves Responses reasoning for Responses upstream and drops Responses-only reasoning for non-Responses providers.
- `plan-007-provider-api-kind-protocol-matrix-completion.md` - completes the basic Anthropic Messages, OpenAI Chat and OpenAI Responses request/header conversion matrix.
- `plan-006-real-api-ttft-sliding-window.md` - computes TTFT sliding windows from real API traffic, excluding manual endpoint tests.
- `plan-001-real-request-response-metrics-alignment.md` - separates endpoint test diagnostics from real request/attempt history and adds request id correlation.
