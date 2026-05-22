# CCR-24 - Server Boundaries

**状态：** completed
**优先级：** P1
**计划编号：** CCR-24
**最后更新：** 2026-05-22

## 目标

拆分 `ccr-server` 的主文件和库文件边界，让 `main.rs` 主要保留二进制启动、server wiring、route registration 和薄分发，让 `lib.rs` 通过清晰模块暴露公共 API。

## 非目标

- 不改变 Route Pool 是唯一 primary routing model 的行为。
- 不恢复 `Router.default`、Failover、ProviderPool runtime behavior、Primary Route UI 或 default route fallback。
- 不改变 Claude/Codex/OpenCode/OpenClaw 默认 client injection 语义。
- 不改变 provider API kind 的外部配置语义。
- 不做与拆分无关的 UI、CLI、配置迁移或功能重写。

## 背景

Linear CCR-24 描述指出 `crates/ccr-server/src/main.rs` 和 `src/lib.rs` 承载了 HTTP handlers、Route Pool retry/ban 状态、runtime metrics、OpenAI Responses SSE 转换、tool interception、request body protocol conversion 等逻辑，局部性差。2026-05-21 的唯一 Linear 评论是范围细化说明，未覆盖或改变描述中的实现要求。

## 设计方向

- 将协议 request body 映射从 `lib.rs` 移到 `protocol/body_mapping.rs`，由 `lib.rs` re-export 保持外部 API。
- 将 OpenAI Responses SSE 事件映射从 `main.rs` 移到 `protocol/responses_stream.rs`。
- 将 runtime metrics helper 从 `main.rs` 移到 `runtime/metrics.rs`。
- 将 Route Pool attempt loop、retryable status、ban/recovery 状态更新和日志从 `main.rs` 移到 `runtime/route_pool.rs`。
- 将 runtime metrics HTTP endpoints 从 `main.rs` 移到 `handlers/runtime_metrics.rs`。
- 如依赖关系要求，增加小型状态或 upstream/tool 模块，但避免重新集中到新的单一大文件。

## 修改文件

- `crates/ccr-server/src/lib.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-server/src/protocol/*`
- `crates/ccr-server/src/runtime/*`
- `crates/ccr-server/src/handlers/*`
- `docs/exec-plans/active/CCR-24-server-boundaries.md`

## 验收测试

- `cargo fmt --check`
- `cargo test --package ccr-server`
- `cargo test --workspace`

## Do / 执行记录

- 实际修改:
  - 将 `lib.rs` 缩小为 auth/redaction 与模块 re-export，协议 request body/upstream request 构建移动到 `protocol/body_mapping.rs`。
  - 将 Route Pool 配置候选与 provider display helper 移动到 `route_pool_config.rs`。
  - 将 Route Pool attempt loop、retry status、failure/ban/recovery 状态与 route-pool logging 移动到 `runtime/route_pool.rs`。
  - 将 request id、attempt/request metric 构建、TTFT 记录 helper 移动到 `runtime/metrics.rs`。
  - 将 Anthropic streaming event 到 OpenAI Responses SSE event 映射移动到 `runtime/responses_stream.rs`。
  - 将 runtime metrics HTTP endpoints 移动到 `handlers/runtime_metrics.rs`。
  - 将 Route Pool ban/recovery/retry tests 移到 runtime 模块，并新增 Responses SSE text delta mapping test。
- 实际偏离计划:
  - Responses SSE module 放在 binary runtime 下，而不是 library `protocol/` 下，因为它直接依赖 streaming TTFT metric finalization。
  - Route Pool candidate/provider display helper 放在 `route_pool_config.rs`，避免和 request body protocol conversion 混在同一模块。
- 中途决策:
  - 保持现有 public re-export 名称，避免改变外部调用点。
  - 保持 HTTP route registration、status/content type、logging field construction 和 Route Pool retry/ban constants 的原有路径与语义。

## Check / 验证与偏差

- 验证命令:
  - `cargo fmt --check` passed.
  - `cargo test --package ccr-server` passed: lib 38 tests, bin 9 tests, doctests 0.
  - `cargo test --workspace` passed across workspace crates and doctests.
  - 2026-05-22 merge resolution: `cargo fmt --check` passed.
  - 2026-05-22 merge resolution: `cargo test --package ccr-server` passed: lib 38 tests, bin 10 tests, doctests 0.
  - 2026-05-22 merge resolution: `cargo test --package ccr-router tokenizer_count_tokens_async_api_uses_http_and_caches_result` passed.
  - 2026-05-22 merge resolution: `cargo test --package ccr-app-core runtime_status` passed.
  - 2026-05-22 merge resolution: `cargo test --workspace` passed across workspace crates and doctests.
- 手工 QA:
  - Inspected module placement and line counts after split: `main.rs` 867 lines, `lib.rs` 66 lines, Route Pool runtime 632 lines, metrics 184 lines, Responses stream 344 lines, runtime metrics handler 88 lines.
- 发现的偏差:
  - None beyond the placement deviations recorded in Do.
- 代码和文档不一致:
  - None found.

## Act / 处理与沉淀

- 已处理偏差:
  - Recorded module placement differences in this plan; behavior remains covered by existing and moved tests.
- 长期文档更新:
  - No long-term docs required; architecture boundaries remain unchanged.
- 新增/更新技术债:
  - None.
- 后续计划:
  - None.

## 决策日志

- 2026-05-22: 以 Linear 描述和唯一范围细化评论为准；本任务只做 `ccr-server` 内部行为保持拆分。
- 2026-05-22: 最新 Linear 人工评论为“有代码合并冲突”。解释为已完成拆分需与当前 `origin/master` 解决合并冲突，保持 CCR-24 原行为保持重构范围，不新增产品行为。
- 2026-05-22: 合并 `origin/master` 时保留 CCR-24 server 模块拆分，并将 master 新增的 official provider API key resolution 接入 `protocol/body_mapping.rs`；本地 tokenizer/status 测试暴露无 `NO_PROXY` 环境下 loopback 请求被代理的问题，修正为 loopback-only runtime calls 绕过代理。

## 完成记录

- 2026-05-22: Completed behavior-preserving `ccr-server` module boundary split and validation.
