# Technical Debt Tracker

**状态：** 长期跟踪文档
**范围：** 集中记录当前已知技术债、文档债和验证债
**最后验证：** 2026-05-21

## 使用方式

本文件记录仍然需要修复或拆 execution plan 的债务。它不是完成记录。

每个条目应包含：

- ID：稳定编号。新条目使用 `plan-001`、`plan-002` 这套编号，便于 execution plan 和 PR 引用。
- 优先级：P0 / P1 / P2 / 后期。
- 状态：open / active / blocked / resolved。
- 影响：为什么它会影响用户、维护者或智能体。
- 修正方向：下一步应该怎么处理。
- 验证方式：完成后如何证明问题被修复。

状态变更规则：

- 新工作统一使用 `plan-001`、`plan-002` 这套编号，不再新增 TD 编号。
- 开始实现时改为 `active`，并链接 `docs/exec-plans/active/` 下的执行计划。
- 完成后改为 `resolved`，保留验证命令或证据。
- 如果只是历史 phase 中出现旧术语，但不代表当前 runtime 行为，不需要为每个历史文件开债务；应在索引或长期文档中统一说明。

## Tracked Items

### plan-007 - Windows Claude config ACL 加固

**优先级：** P2（如果 Windows 发布面扩大则升 P1）
**状态：** resolved
**执行计划：** 无，直接修复

影响：

- Claude settings/plugin config 与 CCR 备份可能包含本地 auth/config 信息。
- Windows 过去缺少等价于 Unix `0600` 的 ACL 加固，可能继承目录 ACL 并向 `Everyone` 或普通 `Users` 组开放读取。

修正方向：

- `crates/ccr-app-core/src/client_config/claude.rs` 的 Windows `set_secure_permissions` 使用 Windows API 设置 protected DACL，只授予当前用户读写权限。
- 加固覆盖 Claude `settings.json`、`config.json`、固定备份、时间戳备份、plugin 备份，以及 restore 时复制回目标文件的临时文件。
- `claude-config.backup.missing` 和 `claude-plugin-config.backup.missing` marker 只包含固定字符串 `missing`，但也通过同一 helper 加固，避免 marker 路径形成宽权限例外。
- 不保留 `Everyone` / 普通 `Users` 读取权限；也不显式保留 `Administrators` / `SYSTEM` ACE。受保护 DACL 避免敏感 JSON 文件继续继承宽权限。

验证方式：

- `cfg(windows)` 测试检查 Windows DACL 只有一个 SID trustee ACE，权限为 `FILE_GENERIC_READ | FILE_GENERIC_WRITE`。
- Unix 测试检查现有 owner-only 语义仍为 `0600`。
- 实际 Windows 发布前仍应保留一次 `icacls` 或 PowerShell `Get-Acl` 抽查记录，用于确认打包运行环境与自动化测试一致。

完成记录：

- 2026-05-21：替换 Claude Windows ACL TODO，新增 Windows API helper 和 Unix/Windows 权限测试。

### plan-005 - 长期文档现状、方向和代码/文档一致性审计

**优先级：** P1
**状态：** active
**执行计划：** `docs/exec-plans/active/plan-005-long-term-docs-current-state-and-direction-alignment.md`

影响：

- 长期文档如果继续混入 CLI-first、API-first、config-file-first 或历史 routing 术语，会误导用户和智能体。
- 已完成历史计划可保留旧术语，但当前长期文档必须准确描述现状和方向。
- 代码和文档不一致需要集中记录处理，否则后续计划会基于错误事实推进。

修正方向：

- 审计长期文档并按代码事实更新当前状态。
- 记录发展方向：UI-first、Route Pool-only、through-CCR、logs/query、真实 request/response metrics。
- 对每个代码/文档不一致项选择修正文档、创建技术债或标注历史记录。

验证方式：

- `./scripts/check-docs-structure.sh` 通过。
- README 保持 UI app 聚焦。
- 当前长期文档不把 removed/legacy routing 行为描述为支持能力。
- 代码/文档不一致清单有处理结果。

### plan-006 - 真实 API 调用 TTFT 滑动窗口

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/plan-006-real-api-ttft-sliding-window.md`

影响：

- 真实 provider 响应速度不能只看 endpoint test 点击测速。
- 当前 request/attempt history 还缺 TTFT，无法判断真实流式首 token 体验。
- Route Pool health 和用户诊断后续需要基于真实 API 调用的窗口指标。

修正方向：

- 在真实 `/v1/messages` 和 `/v1/responses` upstream response 路径采集 TTFT。
- 将 TTFT sample 写入 attempt metrics，并通过 request id 关联 request history。
- 增加真实流量 TTFT sliding window summary API。
- 明确排除 endpoint test latency。

验证方式：

- endpoint test 不产生 TTFT sample。
- 真实 streaming API 调用产生 TTFT sample。
- `/api/runtime-metrics/ttft-summary?window=300` 返回真实流量窗口聚合。

完成记录：

- 2026-05-13：新增 attempt TTFT 字段、TTFT sliding window summary、`/api/runtime-metrics/ttft-summary` 和 Status UI TTFT 展示。
- 验证：`cargo fmt --check`、`cargo test --package ccr-app-core --lib`、`cargo test --package ccr-server`、`cargo test --package ccr-ui` 通过。

### plan-004 - Ubuntu/Linux UI 启动时 GTK tray/EGL 初始化崩溃

**优先级：** P0
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/plan-004-linux-ui-startup-gtk-egl-resilience.md`

影响：

- `cargo run --bin ccr-ui` 在部分 Ubuntu/Linux 图形环境下会因为 GTK tray 初始化 panic 直接退出。
- system tray 是增强能力，不应阻止主桌面 UI 启动。
- EGL/Mesa/ZINK warning 需要有 documented fallback，否则用户无法判断是 GPU、GTK 还是 CCR 自身问题。

修正方向：

- 让 tray 初始化 best-effort，失败或 panic 时禁用 tray 并继续启动主 UI。
- 支持 `CCR_DISABLE_TRAY=1` 这类调试/降级入口。
- 文档记录 Linux UI 启动依赖和软件渲染/tray 禁用排查命令。

验证方式：

- tray 初始化失败不会 panic。
- `CCR_DISABLE_TRAY=1 cargo run --bin ccr-ui` 可跳过 tray。
- `cargo test --package ccr-ui` 和 `cargo check --package ccr-ui` 通过。

完成记录：

- 2026-05-08：tray 初始化改为 best-effort，Linux 下先 `gtk::init()`，并用 `catch_unwind` 隔离 GTK/tray panic。
- 2026-05-08：新增 `CCR_DISABLE_TRAY=1`、移除 `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`、`GDK_BACKEND=x11`、WSL/software rendering fallback，并在 README/release docs 中记录。
- 2026-05-08：捕获 eframe/glutin 启动 panic，避免 WSL Wayland config selection 失败时只输出裸栈。
- 验证：`cargo fmt --check`、`cargo test --package ccr-ui`、`cargo check --package ccr-ui`、`cargo test --workspace`、`./scripts/check-docs-structure.sh`。

### plan-003 - Admin API 默认开放且分散 UI/logs 重点

**优先级：** P0
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/plan-003-admin-api-default-off-ui-logs-focus.md`

影响：

- `POST /api/admin/reload` 当前已注册，且 handler 中仍有 auth TODO。
- Admin API 容易形成 UI 之外的第二套管理产品面。
- 近期更需要把工程注意力放在桌面 UI 功能闭环、日志写入和 logs query 上。

修正方向：

- 默认关闭 `/api/admin/*`。
- 如果未来保留 admin API，必须显式启用、鉴权、测试和文档化。
- 不再把 admin API 作为短期扩展点；UI 和 logs/query 是近期主路径。

验证方式：

- 默认配置下 `/api/admin/reload` 不可用。
- 显式启用后必须通过 auth check。
- UI 使用的 config/logs/route-pool/runtime-metrics API 不受影响。

完成记录：

- 2026-05-08：新增 `AppSettings.admin_api_enabled`，默认 false。
- 2026-05-08：`/api/admin/reload` disabled 返回 404，enabled 后走 auth check。
- 验证：`cargo test --package ccr-server`、`cargo test --package ccr-types`。

### plan-002 - CLI surface 与 UI 核心功能边界不清

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/plan-002-cli-surface-boundary-and-core-extraction.md`

影响：

- 产品定位是桌面 UI，但 `ccr-cli` 当前仍同时承载 CLI 命令和 UI 复用的 client config 业务逻辑。
- 新功能如果继续落在 CLI，会让 UI-first 路线偏移，并让 README/docs 把项目误读成命令行工具。
- `env`、`code`、`model` 等命令会绕开 UI 状态、配置预览和 Route Pool 管理闭环。

修正方向：

- 用 `plan-002` 定义 CLI 边界：CLI 只做自动化/调试/兼容包装，不拥有 UI 核心业务实现。
- 将 Claude/Codex/OpenCode/OpenClaw client config 逻辑迁到 `ccr-app-core` 或新专用 crate。
- 删除或降级与 UI 主工作流冲突的 CLI 命令。

验证方式：

- `ccr-ui` 不再依赖 `ccr-cli` crate 获取核心 client config 业务。
- `ccr-cli` binary 只调用非 CLI crate 的 service API。
- README 快速开始以桌面 UI 为主，不展示 CLI-first client setup。

完成记录：

- 2026-05-08：client config 逻辑迁移到 `ccr-app-core::client_config`。
- 2026-05-08：`ccr-ui` 移除 `ccr-cli` crate 依赖，tray 不再 shell 到 `ccr` 命令。
- 2026-05-08：删除 `env`、`code`、`ui`、`model` CLI 命令。
- 验证：`cargo test --package ccr-app-core --lib`、`cargo test --package ccr-cli`、`cargo check --package ccr-ui`。

### plan-001 - 真实响应数据采集和 endpoint test 边界混淆

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/plan-001-real-request-response-metrics-alignment.md`

影响：

- Endpoint test 是点击按钮触发的主动测速和配置验证，不能代表实际使用中的 provider 响应质量。
- 当前完成口径容易把最小 attempt store、endpoint test 历史和真实 request history 混在一起。
- 后续 Route Pool health、Request History UI 和 metrics API 可能继续基于错误概念扩展。

修正方向：

- 用 `plan-001` 统一纠正长期文档、完成偏差、metrics 模型和 UI/API 边界。
- 明确 request metrics、attempt metrics、response metrics 与 endpoint test 的关系。
- 后续真实响应数据必须来自实际 client traffic。

验证方式：

- endpoint test 不写入 request history。
- 真实 client request 产生 request-level record。
- upstream retry 产生可关联的 attempt-level record。
- Runtime summary 不使用 endpoint test 结果。

完成记录：

- 2026-05-13：新增 request-level metrics、attempt request id correlation 和 `/api/runtime-metrics/requests`。
- 验证：`cargo fmt --check`、`cargo test --package ccr-app-core --lib`、`cargo test --package ccr-server`、`cargo test --package ccr-ui`、`cargo test --workspace` 通过。

### TD-001 - 缺少 AGENTS.md 智能体入口地图

**优先级：** P0
**状态：** resolved

影响：

- 新智能体进入仓库时没有稳定导航入口。
- 任务约束散落在历史 phase 和少量 docs 中，容易读取过多或漏读关键规则。

修正方向：

- 新增顶层 `AGENTS.md`。
- 保持短文档，只写工作流、关键命令、目录地图和必读文档链接。
- 不把 `AGENTS.md` 写成完整手册。

验证方式：

- `AGENTS.md` 存在。
- 文件长度保持在可快速阅读范围内。
- 文档链接到 `ARCHITECTURE.md`、`docs/long-term-roadmap.md`、`docs/exec-plans/tech-debt-tracker.md` 和测试命令。

完成记录：

- 2026-05-07：新增顶层 `AGENTS.md`。

### TD-002 - 缺少 ARCHITECTURE.md 顶层架构地图

**优先级：** P0
**状态：** resolved

影响：

- workspace crate 边界只能通过 Cargo.toml 和源码推断。
- 智能体容易把 UI、CLI、server、app-core 的职责混在一起。

修正方向：

- 新增 `ARCHITECTURE.md`。
- 记录每个 crate 的职责、主要依赖方向、runtime 请求路径和配置读写路径。
- 明确 UI 优先、server through-CCR、Route Pool 主模型等架构原则。

验证方式：

- `ARCHITECTURE.md` 存在。
- 每个 workspace crate 都有一句职责说明。
- 至少包含 Claude/Codex/OpenCode/OpenClaw 注入路径和 `/v1/messages`、`/v1/responses` upstream 路径。

完成记录：

- 2026-05-07：新增顶层 `ARCHITECTURE.md`。

### TD-003 - 缺少 CI 和机械校验入口

**优先级：** P0
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-003-ci-and-mechanical-checks.md`

影响：

- 当前仓库没有 `.github` workflow。
- 格式化、测试、文档链接和计划状态只能靠人工或本地运行。
- 不符合“知识库可机械检查”的目标。

修正方向：

- 新增基础 GitHub Actions workflow。
- 先覆盖 `cargo fmt --check` 和 `cargo test --workspace`。
- 后续接入 clippy、文档链接校验、active plan 校验。

验证方式：

- `.github/workflows/ci.yml` 存在。
- CI 至少运行 fmt 和 workspace tests。
- PR 能显示 CI 结果。

### TD-004 - docs 缺少 index 和结构化知识库入口

**优先级：** P1
**状态：** resolved

影响：

- `docs/` 里已有长期缺口和设计文档，但没有索引。
- 智能体无法快速判断应该先读哪个文档。

修正方向：

- 新增 `docs/index.md`。
- 按产品边界、架构、客户端注入、provider metrics、缺口分析分类。
- 每个文档记录状态、用途和主要相关代码区域。

验证方式：

- `docs/index.md` 存在。
- `docs/` 下每个长期文档都被索引。
- 索引中的本地路径可打开。

完成记录：

- 2026-05-07：新增 `docs/index.md`，并补齐长期文档入口。

### TD-005 - execution plans 缺少 active/completed 分层

**优先级：** P1
**状态：** resolved

影响：

- 历史 phase 文档平铺在 `plans/` 下。
- 已完成历史计划和未来计划混在一起，智能体难以判断是否仍适用。

修正方向：

- 新增 `docs/exec-plans/index.md`。
- 新增 `docs/exec-plans/completed/`。
- 新增 `docs/exec-plans/completed/`。
- 旧 top-level phase 文档已迁移到 `docs/exec-plans/completed/legacy-phase-*.md`。

验证方式：

- `docs/exec-plans/index.md` 存在。
- `docs/exec-plans/completed/README.md` 存在。
- `docs/exec-plans/completed/README.md` 存在。
- 新增 execution plan 使用 active/completed 流程。

完成记录：

- 2026-05-07：新增 `docs/exec-plans/` canonical execution plan 目录。

### TD-006 - `/api/provider-pool/status` 与 Route Pool-only 方向存在命名冲突

**优先级：** P0
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-006-provider-pool-status-alias.md`

影响：

- Phase 68 已要求删除 `ProviderPool` runtime 模型。
- server 曾保留 `/api/provider-pool/status` route，并映射到 Route Pool 状态。
- 该 alias 会让后续智能体误以为 ProviderPool 仍是当前业务模型。

修正方向：

- 删除 `/api/provider-pool/status` route。

验证方式：

- 源码和文档对该 route 的语义一致。
- 搜索 `ProviderPool` / `provider-pool` 时不会显示它是当前 runtime 模型。

完成记录：

- 2026-05-07：删除 server `/api/provider-pool/status` route，保留 `/api/route-pool/status`。

### TD-007 - app log 是文本文件，缺少结构化查询能力

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-007-structured-log-query.md`

影响：

- 当前日志可读，但查询能力弱。
- 智能体和 UI 只能读取完整日志或做字符串过滤。
- 长期无法支撑 provider health、Route Pool event history 和可观测性反馈回路。

修正方向：

- 将 app log 输出升级为 JSONL，或新增并行 JSONL sink。
- 增加 `/api/logs/query`，支持 target、event、provider、route、time range、limit。
- 保留现有文本 UI 显示兼容。

验证方式：

- 单元测试覆盖日志字段转义、敏感 header 脱敏和查询过滤。
- UI Logs tab 可按 target/event 过滤。
- Codex 可通过 API 查询 endpoint-test、upstream、route-pool 事件。

### TD-008 - Runtime metrics 仍停留在设计文档

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-008-runtime-metrics-store.md`

影响：

- `docs/provider-runtime-metrics.md` 已定义 attempt/request metrics，但代码还没有完整 metrics store。
- Route Pool 健康仍主要依赖连续失败状态，缺少真实质量趋势。

修正方向：

- 拆一个 active execution plan 实现最小 runtime metrics。
- 先记录 upstream attempt 的 provider、route、model、status/error、latency、retry index、outcome。
- 再提供 status API 给 UI 和 Route Pool 使用。

验证方式：

- 有 attempt metrics 单元测试。
- upstream 成功、HTTP 429、HTTP 5xx、network error 都会记录。
- Route Pool status API 能返回最近窗口的 attempt summary。

### TD-009 - UI 仍有业务逻辑直接留在 egui 层

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-009-ui-core-decoupling.md`

影响：

- Phase 52 已完成第一轮 UI/core 解耦，但状态页、设置页和部分操作仍直接调用 CLI helper 或本地进程逻辑。
- UI 行为难以用纯单元测试覆盖。

修正方向：

- 继续把 server lifecycle、client injection snapshot、activation/deactivation 和 status aggregation 下沉到 `ccr-app-core`。
- UI 只消费 view model 和派发 action。

验证方式：

- app-core 覆盖 server snapshot、client snapshot 和 action reducer。
- UI tab 测试只做 smoke，不承担业务断言主力。

### TD-010 - 缺少历史请求记录

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-010-request-history.md`

影响：

- 当前用户和 agent 需要回看真实请求和 upstream attempts，而不是只看即时状态。
- Endpoint test 只能说明配置探测结果，不能代表真实历史请求。

修正方向：

- 持久化真实 upstream attempt history。
- 记录 route、provider、endpoint、model、latency、status/error、retry index、outcome。
- 不保存完整 prompt/response body。

验证方式：

- 多次真实 upstream attempt 会产生可查询历史。
- `/api/runtime-metrics/attempts` 能返回最近 attempts。
- `/api/runtime-metrics/summary` 能返回 route/provider 聚合。

### TD-011 - 缺少 agent-friendly 本地运行脚本

**优先级：** P2
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-011-agent-local-run-script.md`

影响：

- 智能体要验证 server/UI 行为时，需要手动拼接配置、端口、日志路径和进程管理。
- 不利于 worktree 隔离和长时间自动验证。

修正方向：

- 增加 `scripts/agent-run-local` 或等价入口。
- 使用临时 HOME/config、随机端口、独立日志路径。
- 输出 health URL、config path、log path 和 stop 命令。

验证方式：

- 脚本能启动 server 并通过 `/health`。
- 不污染用户真实 `~/.claude-code-router/config.json`。
- 脚本退出后可清理临时目录。

### TD-012 - 打包产物和源码缺少发布流程文档

**优先级：** P2
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-012-release-checklist.md`

影响：

- 仓库已有 macOS DMG、pkg 和 Windows MSI 脚本，但没有统一 release checklist。
- 后续修改打包逻辑时难以判断验收边界。

修正方向：

- 新增 `docs/release.md` 或 `packaging/README.md`。
- 记录 macOS app bundle、DMG、pkg、Windows MSI 的构建命令和验证步骤。

验证方式：

- 文档覆盖当前 `packaging/` 下脚本。
- 至少包含构建、安装、卸载和 smoke test 步骤。

### TD-013 - 顶层 plans 目录缺少迁移清单

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-013-legacy-plans-inventory.md`

影响：

- 顶层 `plans/` 曾有大量历史 phase 文档。
- 在没有清单的情况下直接删除会丢失验证命令和历史决策。

修正方向：

- 建立 `docs/exec-plans/legacy-plans-inventory.md`。
- 为每个 `plans/phase-*.md` 标记 migrate / summarize / delete。

验证方式：

- 每个历史 phase 文件都出现在 inventory 中。
- 每个条目都有推荐动作。

### TD-014 - 有价值历史 phase 尚未迁移到 completed exec plans

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-014-migrate-legacy-plans-to-completed.md`

影响：

- canonical execution plan 系统已经建立，但历史完成记录仍散落在顶层 `plans/`。
- 后续删除 `plans/` 前需要保留有价值的完成证据。

修正方向：

- 根据 inventory 迁移有价值文档到 `docs/exec-plans/completed/`。
- 对不值得全文迁移的文档做摘要。

验证方式：

- inventory 中 migrate/summarize 条目都有目标位置。
- completed 文档包含原始路径和验证证据。

### TD-015 - 顶层 plans 目录仍存在

**优先级：** P1
**状态：** resolved
**执行计划：** `docs/exec-plans/completed/td-015-remove-top-level-plans-directory.md`

影响：

- 仓库曾同时存在 `plans/` 和 `docs/exec-plans/`，会形成双计划系统。
- 智能体可能继续把新计划写入旧目录。

修正方向：

- 在 TD-013 和 TD-014 完成后删除顶层 `plans/`。
- 清理所有把顶层 `plans/` 当作当前计划系统的引用。

验证方式：

- `test ! -d plans` 通过。
- 当前文档只把 `docs/exec-plans/` 作为 execution plan 系统。

## Resolved Items

Resolved entries currently remain in `Tracked Items` to preserve ID order. If this file grows too large, resolved entries can move here while keeping their IDs and completion evidence.
