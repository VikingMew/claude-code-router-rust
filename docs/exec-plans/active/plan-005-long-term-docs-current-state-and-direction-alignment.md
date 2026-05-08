# plan-005 - Long-Term Docs Current State And Direction Alignment

**状态：** active
**优先级：** P1
**计划编号：** plan-005
**最后更新：** 2026-05-08

## 目标

更新所有长期文档，让它们准确描述当前项目现状、产品方向、技术方向和已知偏差。

本计划要解决的问题不是单个 README 或单个 phase 文档，而是长期文档整体对齐：

- 用户和智能体能一眼判断 CCR 当前是 UI-first desktop app。
- 长期方向清楚：UI 功能闭环、Route Pool-only、through-CCR、logs/query、真实 request/response metrics。
- CLI、admin API、历史 routing 名词和 config-file 工作流不会被误读成当前主产品路径。
- 发现代码和文档不一致时，有明确记录、修正和技术债归档流程。

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
- plan-001 仍 active：真实 request/response metrics、attempt metrics 和 endpoint test 边界仍需继续对齐。

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
- `docs/exec-plans/*`：计划、技术债、完成记录、执行记录、验证偏差和处理沉淀。

文档判断原则：

- 当前长期文档必须描述当前支持的产品路径。
- 历史 completed docs 可以保留旧术语，但索引和长期文档必须说明它们是历史记录。
- 如果代码和文档冲突，先信代码，然后更新文档或创建技术债。
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
- `docs/exec-plans/tech-debt-tracker.md`

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
- 未解决偏差对应的 follow-up plan 或 tech-debt tracker 条目。

## Do / 执行记录

未开始。

计划执行时记录：

- 实际审计的长期文档清单。
- 从代码核验出的事实，包括 UI 启动、CLI 命令、admin API 默认值、Route Pool runtime、logs/query API、metrics API 和 WSL fallback。
- 实际更新的文档文件。
- 与本计划预期不同的实现或文档处理决策。

## Check / 验证与偏差

未完成。

计划完成前记录：

- 实际运行的验证命令和结果。
- 每条代码/文档不一致发现。
- 哪些偏差已经在文档中修正。
- 哪些偏差不能直接修正，需要保留为技术债或后续计划。

## Act / 处理与沉淀

未完成。

计划完成前记录：

- 已处理偏差的处理方式：修正文档、创建技术债或标注为历史记录。
- 写入长期文档的新规则。
- 更新到 `docs/exec-plans/tech-debt-tracker.md` 的后续项。
- 是否满足归档到 `docs/exec-plans/completed/` 的条件。

## 决策日志

- 2026-05-08：用户要求每个 execution plan 遵循 PDCA；确认本仓库的 Plan 是原有目标/非目标/背景/设计/修改文件/验收测试，Do/Check/Act 用于执行记录、验证偏差和闭环沉淀。
- 2026-05-08：用户要求长期文档描述清楚当前现状、发展方向，以及发现代码和文档不一致的部分。

## 完成记录

未完成。
