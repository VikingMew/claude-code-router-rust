# plan-005 - Long-Term Docs Current State And Direction Alignment

**状态：** completed
**优先级：** P1
**计划编号：** plan-005
**最后更新：** 2026-05-21

## 目标

更新所有长期文档，让它们准确描述当前项目现状、产品方向、技术方向和已知偏差。

本计划要解决的问题不是单个 README 或单个 phase 文档，而是长期文档整体对齐：

- 用户和智能体能一眼判断 CCR 当前是 UI-first desktop app。
- 长期方向清楚：UI 功能闭环、Route Pool-only、through-CCR、logs/query、真实 request/response metrics。
- CLI、admin API、历史 routing 名词和 config-file 工作流不会被误读成当前主产品路径。
- 发现代码和文档不一致时，有明确记录、修正和 follow-up execution plan 流程。

## 非目标

- 不在本计划中实现新的产品功能。
- 不删除历史 phase 文档。
- 不把 README 重新扩展成 API、CLI 或配置手册。
- 不把 admin API、legacy route fallback、ProviderPool、Failover 或 Primary Route 重新描述为当前能力。
- 不实现 provider store/database。
- 不实现完整 request history。
- 不重构 CLI 或 server。
- 不重新设计 README 视觉风格。
- 不清理所有 historical phase 旧术语。

## 背景

长期文档分散在 `AGENTS.md`、`ARCHITECTURE.md`、`README.md`、`docs/*.md` 和 `docs/exec-plans/*`。最近产品方向已经收敛为 UI-first，但部分文档可能仍保留 CLI-first、API-first、config-file-first 或历史 routing 术语。

近期已经完成或确认的状态：

- README 已收敛为 UI app 介绍，不再包含 config JSON 教程、CLI 命令列表、API endpoint 清单或 crate map。
- CLI 已收缩：client config 逻辑迁到 `ccr-app-core`，UI 不再依赖 `ccr-cli` crate；CLI 主要保留 start/stop/restart/status、client activate/deactivate、preset 和 statusline。
- Admin API 默认关闭：`AppSettings.admin_api_enabled` 默认为 false，`/api/admin/reload` disabled 时返回 404，enabled 后走 auth check。
- Linux/WSL UI startup 已修复关键 fallback：tray best-effort，WSL/fallback 场景移除 `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`，设置 `GDK_BACKEND=x11` 和 `LIBGL_ALWAYS_SOFTWARE=1`。
- Route Pool 是当前唯一主 routing 模型；不应恢复 legacy default route/failover/provider pool 行为。
- plan-001 已完成：真实 request/response metrics、attempt metrics 和 endpoint test 边界已完成第一阶段对齐。

## 设计方向

长期文档应分层：

- `README.md`：只面向用户解释这是 UI app、能做什么、怎么启动和怎么从 UI setup。
- `AGENTS.md`：面向智能体的入口地图和硬规则。
- `ARCHITECTURE.md`：代码结构、crate 边界、runtime paths 和 invariants。
- `docs/index.md`：长期知识库索引。
- `docs/long-term-roadmap.md`：产品方向和优先级。
- `docs/RELIABILITY.md` / `docs/SECURITY.md`：可靠性和安全边界。
- `docs/QUALITY_SCORE.md`：当前质量状态和验证口径。
- `docs/provider-runtime-metrics.md`：真实 traffic metrics 与 endpoint test 边界。
- `docs/exec-plans/*`：计划、完成记录、执行记录、验证偏差和处理沉淀。

文档判断原则：

- 当前长期文档必须描述当前支持的产品路径。
- 历史 completed docs 可以保留旧术语，但索引和长期文档必须说明它们是历史记录。
- 如果代码和文档冲突，先信代码，然后更新文档或创建 follow-up execution plan。
- 如果某个能力只存在于 CLI/API，但 UI 未开发完成，应标注为辅助/调试/未主推，不能作为用户主路径。

## 已知代码/文档不一致候选

本计划开始时已知或需要重点核验的偏差：

- README 曾包含 config JSON 示例、CLI 命令和 API endpoint 清单，已初步修正，但需要确认长期 docs 没有继续引导 config-file-first。
- `WINIT_UNIX_BACKEND=x11` 曾被写作 WSL fallback，但 `winit 0.30` 已移除该变量；当前验证成功方案是 unset Wayland env 并走 X11。
- 历史 phase 中仍可能出现 `Router.default`、`Failover`、`ProviderPool`、Primary Route 等旧术语；当前长期 docs 不应把它们作为支持能力。
- Admin API 文档可能与当前 default-off 行为不一致。
- Runtime metrics 文档可能把 endpoint test、attempt metrics 和 request history 混写；plan-001 正在处理。
- CLI 文档或 crate 描述可能仍暗示 CLI 是主产品面。

## 修改文件

预计修改：

- `AGENTS.md`
- `ARCHITECTURE.md`
- `README.md`
- `docs/index.md`
- `docs/long-term-roadmap.md`
- `docs/RELIABILITY.md`
- `docs/SECURITY.md`
- `docs/QUALITY_SCORE.md`
- `docs/provider-runtime-metrics.md`
- `docs/client-model-mapping.md`
- `docs/release.md`
- `docs/exec-plans/active/README.md`
- `docs/exec-plans/index.md`

可能修改：

- `docs/cc-switch-feature-gap-analysis.md`
- `docs/ccr-original-feature-gap-analysis.md`
- `docs/DESIGN.md`
- `docs/FRONTEND.md`
- `docs/PRODUCT_SENSE.md`

## 验收测试

```sh
./scripts/check-docs-structure.sh
rg -n "Router\\.default|Failover|ProviderPool|Primary Route|default route fallback" AGENTS.md ARCHITECTURE.md README.md docs --glob '!docs/exec-plans/completed/legacy-phase-*.md'
rg -n "config\\.json|cargo run --bin ccr --|GET /api|POST /v1" README.md
rg -n "admin_api_enabled|/api/admin|Admin API" crates docs AGENTS.md ARCHITECTURE.md README.md
rg -n "Endpoint test|Request metrics|Attempt metrics|Response metrics|request history" docs
cargo fmt --check
cargo test --workspace
```

人工验收：

- 从新用户视角阅读 README，确认它呈现 CCR 是桌面 UI app。
- 从智能体视角阅读 `AGENTS.md` 和 `ARCHITECTURE.md`，确认它们指向当前边界。
- 对照代码核验 CLI 命令、admin API 默认行为和 UI/server startup flow。
- 确认 historical completed plans 可以保留旧术语，但当前长期文档不把旧术语当作当前能力。

完成时必须记录：

- 实际修改文件清单。
- 每条代码/文档不一致的处理结果。
- 运行过的验证命令和结果。
- 未解决偏差对应的 follow-up plan。

## Do / 执行记录

- 实际审计的长期文档清单：
  - `AGENTS.md`
  - `ARCHITECTURE.md`
  - `README.md`
  - `docs/index.md`
  - `docs/long-term-roadmap.md`
  - `docs/RELIABILITY.md`
  - `docs/SECURITY.md`
  - `docs/QUALITY_SCORE.md`
  - `docs/provider-runtime-metrics.md`
  - `docs/provider-api-kinds.md`
  - `docs/exec-plans/index.md`
  - `docs/exec-plans/active/README.md`
  - `docs/exec-plans/completed/README.md`
  - former centralized debt index
- 从代码核验出的事实：
  - UI-first 和 through-CCR 仍是当前主路径。
  - CLI 保留为自动化/调试入口。
  - Admin API 默认关闭；`AppSettings.admin_api_enabled` 默认 false。
  - Route Pool 是唯一主 routing model。
  - logs query、runtime metrics、request/attempt metrics 和 TTFT summary 已实现最小闭环。
  - deleted debt index 中只有本计划仍是 active，其余条目均已 resolved 或有 completed plan 记录。
- 实际更新的文档文件：
  - `AGENTS.md`
  - `docs/index.md`
  - `docs/long-term-roadmap.md`
  - `docs/exec-plans/index.md`
  - `docs/exec-plans/active/README.md`
  - `docs/exec-plans/completed/README.md`
  - `scripts/check-docs-structure.sh`
  - 本计划文件。
- 与本计划预期不同的实现或文档处理决策：
  - 不再保留集中债务索引。所有已出现的问题已在 completed plans 中保留完成证据，后续新问题直接创建 active execution plan。

## Check / 验证与偏差

- 实际运行的验证命令和结果：
  - `rg -n "状态：.*(open|active|blocked)" deleted debt index`：仅发现本计划仍为 active。
  - `rg -n "Router\\.default|Failover|ProviderPool|Primary Route|default route fallback" AGENTS.md ARCHITECTURE.md README.md docs --glob '!docs/exec-plans/completed/legacy-phase-*.md'`：当前长期文档只在禁止/历史语境中出现旧 routing 术语。
  - `rg -n "config\\.json|cargo run --bin ccr --|GET /api|POST /v1" README.md`：无 README CLI/API/config-first 入口。
  - `rg -n "admin_api_enabled|/api/admin|Admin API" crates docs AGENTS.md ARCHITECTURE.md README.md`：当前长期文档明确 Admin API 默认关闭；代码默认值为 false。
  - `rg -n "Endpoint test|Request metrics|Attempt metrics|Response metrics|request history" docs`：长期文档已区分 endpoint test 和真实 request/attempt/response metrics。
- 每条代码/文档不一致发现：
  - 集中债务索引已不再需要；保留会让新问题继续进入集中债务文档，和当前“直接创建 active exec plan”的流程冲突。
  - `AGENTS.md`、`docs/index.md`、`docs/long-term-roadmap.md`、`docs/exec-plans/index.md` 和 `scripts/check-docs-structure.sh` 仍引用集中债务索引，需要删除或改写。
- 已修正偏差：
  - 删除当前长期文档和结构检查脚本中对集中债务索引的依赖。
  - 将 active README 改为 “No active execution plans”。
  - 将本计划补齐 Do/Check/Act、完成记录和完成偏差后移动到 completed。
- 不能直接修正、需要后续计划的偏差：
  - 无。后台日志加载、Route Pool health score、stream timing/token throughput 等已经在对应 completed plan 偏差或长期路线中表达，不需要集中 tracker 继续跟踪。

## Act / 处理与沉淀

- 已处理偏差的处理方式：
  - 修正文档入口，不再要求维护集中债务索引。
  - 删除集中债务索引前确认所有已列条目已 resolved，唯一 active 项为本计划。
  - 已完成问题的证据保留在 `docs/exec-plans/completed/` 下的对应 plan 文件中。
- 写入长期文档的新规则：
  - 新复杂工作直接进入 `docs/exec-plans/active/`。
  - completed plan 保留完成证据、验证命令和完成偏差。
  - 不再维护集中技术债文档。
- 新增到 `docs/exec-plans/active/` 的后续项：
  - 无。
- 归档条件：
  - 满足。长期文档入口已更新，已出现的问题都有完成记录或本计划闭环记录。

## 决策日志

- 2026-05-08：用户要求每个 execution plan 遵循 PDCA；确认本仓库的 Plan 是原有目标/非目标/背景/设计/修改文件/验收测试，Do/Check/Act 用于执行记录、验证偏差和闭环沉淀。
- 2026-05-08：用户要求长期文档描述清楚当前现状、发展方向，以及发现代码和文档不一致的部分。

## 完成记录

完成：2026-05-21。

- 确认已删除债务索引中除本计划外没有 open/active/blocked 条目。
- 当前长期文档已保持 UI-first、Route Pool-only、through-CCR、Admin API default-off、endpoint test 与真实 runtime metrics 分离等方向。
- 删除当前文档入口和结构检查脚本对集中债务索引的依赖。
- 删除集中债务索引文件。
- 本计划移动到 `docs/exec-plans/completed/`。

验证：

```sh
rg -n "former centralized debt index path" AGENTS.md ARCHITECTURE.md README.md docs scripts
./scripts/check-docs-structure.sh
cargo fmt --check
cargo test --workspace
```

## 完成偏差

- 本计划没有实现新功能；它只关闭文档一致性和技术债记录系统迁移。
- 历史 completed plans 中仍可能保留 `TD-*` 标题和旧术语，这是历史完成证据，不代表当前流程继续使用 TD 编号或集中 tracker。
