# Long-Term Roadmap

**状态：** 长期记录文档
**范围：** 记录 `ccr-rust` 的长期产品方向、工程约束和后续 phase 拆分边界
**最后验证：** 2026-05-08

## 使用方式

本文件是长期方向入口，不替代具体执行计划。

- 新 execution plan 应从本文件、`docs/cc-switch-feature-gap-analysis.md`、`docs/ccr-original-feature-gap-analysis.md` 和 `docs/exec-plans/tech-debt-tracker.md` 中选择明确问题。
- 复杂工作必须创建独立 phase 文档，记录目标、非目标、设计方向、测试和完成状态。
- 已完成 phase 的历史文档可以保留旧术语，但长期文档不能把已删除能力写成当前支持能力。
- 如果代码行为和本文件不一致，先以代码为准，再更新文档或创建技术债条目。

## 产品定位

`ccr-rust` 是 UI 优先的本地桌面程序，用于把 Claude Code、Codex、OpenCode、OpenClaw 等 client 接入本地 CCR server，并由 server 负责上游 provider、模型、协议转换、Route Pool、日志和诊断。

默认产品路径是 through-CCR：

- client 只连接本地 CCR server。
- provider kind、transformer、Route Pool 和重试策略由 CCR server 处理。
- client 注入层不应该理解或泄漏上游 provider 协议细节。
- direct-to-provider 如果未来引入，只能作为高级可选模式，并且必须有清晰预览和回滚策略。

CLI 是辅助入口，用于调试、自动化和少量运维操作；P0/P1 用户体验应优先在桌面 UI 中完成闭环。

Admin API 不是近期主产品面。`/api/admin/*` 必须默认关闭；如果未来需要作为恢复或高级运维入口，必须显式启用、鉴权、测试并在文档中说明风险。近期管理能力优先通过桌面 UI、受控 config/logs/status API 和可查询日志完成。

## 架构方向

### 1. 以 Route Pool 作为唯一主路由模型

长期主模型是 `RoutePool`。

应继续保持：

- server runtime 只从 enabled Route Pool candidates 产生 upstream attempts。
- provider-only route 表示使用请求里的模型或 provider 的唯一模型。
- `provider,model` route 表示固定上游模型。
- route ordering、ban、retry 和健康判断都围绕 Route Pool 实现。

不应重新引入：

- `Router.default`
- `Failover`
- `ProviderPool` runtime 模型
- Primary Route / Default Route UI
- default route fallback

兼容 API 如果必须保留，需要在代码和文档中明确标记为 compatibility alias，不能作为当前业务模型继续扩展。

### 2. 建立统一 client app 抽象

当前 Claude Code、Codex、OpenCode、OpenClaw 已有 through-CCR 注入能力，但仍缺统一抽象。

长期目标：

- 每个 client 有统一的 status snapshot。
- 每个 client 有独立 config path override。
- 每个 client 的 activate/deactivate/add/remove 行为可预览、可验证、可回滚。
- OpenCode/OpenClaw 这类 additive config 不应使用 Claude/Codex 的整文件覆盖语义。
- 状态页展示 client 是否指向当前本地 CCR server，而不是只判断是否存在 CCR 片段。

非目标：

- Gemini 暂不进入当前目标。
- MCP 和 Skills 管理保留为后期能力。

### 3. Provider 管理从 JSON 配置走向结构化模型

当前 provider 仍主要存储在 `config.json` 中。短期可以继续保留这个模型，但长期需要更结构化的 provider store。

需要补齐：

- provider create/update/delete 的结构化 API。
- current provider 或 current route 的持久语义。
- provider 分类、排序、icon、metadata。
- provider API kind 的显式/推断状态在 UI、server、endpoint test 中保持一致。
- endpoint candidates 和 runtime metrics 联动。
- preset/profile 应能生成 provider、Route Pool、client mapping 和 required env。

引入数据库前，需要先明确迁移策略、备份策略和导入导出边界。

### 4. 可观测性从日志走向可查询反馈回路

当前已有 app log、server tracing log 和日志 UI，但长期需要让 Codex 和用户都能直接查询真实运行状态。

目标分层：

- 第一阶段：结构化 JSONL app log、过滤 API、按 target/event/route/provider 查询。
- 第二阶段：本地 metrics store，记录 request metrics 和 attempt metrics。
- 第三阶段：Route Pool 使用真实 runtime metrics 做 health score 和排序建议。
- 第四阶段：可选接入外部 observability sink，但本地路由不能依赖外部服务。

近期重点是 UI 可见日志写入和 logs query API。Admin API 暂不作为观测或管理功能的扩展点。

必须区分：

- Endpoint test：主动探测，用于配置验证。
- Runtime metrics：真实请求观测，用于 provider 质量和 Route Pool 健康。

### 5. UI 逻辑继续下沉到 app-core

`ccr-ui` 应主要负责渲染和派发 action。复杂状态转换、配置写入、endpoint test 结果处理、status snapshot 和 client 注入逻辑应继续迁移到 `ccr-app-core` 或对应非 UI crate。

长期目标：

- UI tab 中不直接修改复杂业务结构。
- app-core 有稳定、可单元测试的 action/reducer 或 service API。
- UI smoke tests 只覆盖渲染入口和关键状态，不承担业务正确性的主要验证。
- blocking 网络请求不应出现在 egui 渲染路径中。

### 6. 知识库作为记录系统

仓库应逐步变成 agent-readable record system。

目标结构：

```text
AGENTS.md
ARCHITECTURE.md
docs/
  index.md
  long-term-roadmap.md
  DESIGN.md
  FRONTEND.md
  PRODUCT_SENSE.md
  QUALITY_SCORE.md
  RELIABILITY.md
  SECURITY.md
  client-model-mapping.md
  provider-runtime-metrics.md
  cc-switch-feature-gap-analysis.md
  ccr-original-feature-gap-analysis.md
docs/exec-plans/
  index.md
  tech-debt-tracker.md
  active/
  completed/
```

历史 phase 文档已迁移到 `docs/exec-plans/completed/legacy-phase-*.md`。新增复杂计划应进入 `docs/exec-plans/active/`，完成后再移动到 `docs/exec-plans/completed/`。

文档需要可机械检查：

- 文档引用的代码路径必须存在，或明确标记为历史路径。
- 长期文档必须有状态和最后验证日期。
- active plan 必须有验收测试。
- completed plan 必须有完成记录和验证命令。

## 近期优先级

### P0

- 维护 `AGENTS.md` 和 `ARCHITECTURE.md`，确保它们持续作为智能体入口地图。
- 维护 `docs/exec-plans/tech-debt-tracker.md`，把已知债务集中记录。
- 维护基础 CI：`cargo fmt --check`、`cargo test --workspace` 和文档结构检查。
- 保持 Route Pool-only API 命名，不恢复 `/api/provider-pool/status`。
- 维护 logs query API，避免 UI 和智能体只能读取完整日志文件。
- 默认关闭 `/api/admin/*`，短期不把 admin API 作为功能扩展点。

### P1

- 维护 `docs/index.md` 和 `docs/exec-plans/index.md`。
- 将新的复杂工作放入 `docs/exec-plans/active/`。
- 将 UI 中仍然直接处理的业务逻辑继续迁移到 `ccr-app-core`。
- 为 Route Pool runtime state 增加清除 ban、event history 和最小 health summary。
- 为真实 request/attempt history 增加更完整的查询和 UI。

### 后期

- Provider store / database。
- Universal Provider。
- 完整 import/export。
- Runtime metrics store。
- Session 管理。
- MCP / Skills 管理。
- WebDAV 或其他配置同步。
- 自动更新。

## 决策原则

- UI 优先，CLI 辅助。
- Admin API 默认关闭，UI 和 logs/query 优先。
- through-CCR 默认，direct-to-provider 高级可选。
- Route Pool 是主路由模型。
- provider kind 影响 server upstream、endpoint test 和 transformer，不影响默认 client 注入。
- 真实请求指标优先于单次 endpoint test 结果。
- 文档必须帮助智能体导航，不应变成无法维护的长手册。
- 如果一个能力不能被测试或被文档索引，它就不能被视为完成。
