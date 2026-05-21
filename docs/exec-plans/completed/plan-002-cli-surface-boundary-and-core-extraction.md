# plan-002 - CLI Surface Boundary And Core Extraction

**状态：** completed
**优先级：** P1
**计划编号：** plan-002
**最后更新：** 2026-05-08

## 目标

确保 `ccr-cli` 不再承载桌面 UI 已经开发或即将开发的核心产品功能。

CCR 的产品形态是桌面 UI。CLI 可以继续作为开发、自动化和调试入口存在，但不能成为 provider 管理、Route Pool 管理、client 注入、server 生命周期、preset/profile、metrics/history 等核心功能的主要业务实现位置。

本计划要建立清晰边界：

- UI 核心业务逻辑归属 `ccr-app-core` 或专用非 UI crate。
- `ccr-ui` 调用 core/service API。
- `ccr-cli` binary 只做薄包装、调试命令或兼容入口。
- 新功能默认不进入 `ccr-cli`，除非它是明确的自动化/诊断接口。

## 非目标

- 不立即删除整个 `ccr-cli` crate。
- 不删除 UI 正在复用的 Claude/Codex/OpenCode/OpenClaw 配置逻辑。
- 不在本计划内重写完整 Settings/Provider/Route Pool UI。
- 不移除 server 的 HTTP API。
- 不破坏已有用户配置备份和恢复语义。

## 背景

当前 `ccr-cli` 同时包含两类内容：

- CLI binary 命令：`start`、`stop`、`status`、`claude-activate`、`codex-activate`、`model`、`preset`、`env`、`code`、`ui`、`statusline`。
- UI 复用的配置模块：`claude_config`、`codex_config`、`opencode_config`、`openclaw_config`。

这会产生长期风险：

- 新 agent 可能继续把核心能力加进 CLI，而不是 UI/app-core。
- UI 可能继续依赖 CLI helper，导致业务逻辑边界不清。
- README 或 docs 容易把项目描述成 CLI 工具。
- `model`、`env`、`code` 这类命令会把用户引回手动 env 或单命令路径，偏离 UI-first。

## 当前 CLI 功能盘点

### CLI binary commands

保留候选：

- `start` / `stop` / `restart` / `status`：开发、自动化和故障排查有价值，但 UI 是主入口。
- `preset list/info/delete/export`：可作为调试/导出入口，但 preset 安装和选择应在 UI 中完成。
- `claude-activate` / `claude-deactivate` / `codex-activate` / `codex-deactivate`：短期可保留为包装层，业务逻辑必须迁出 CLI crate。

删除或降级候选：

- `env`：手动 shell env 路径，不符合默认 through-CCR UI 闭环。
- `code`：包装 `claude` 并注入 env，容易绕过 UI 状态和配置预览。
- `ui`：从 CLI 启动 UI，发布后价值较低。
- `model`：直接修改 Route Pool 第一条 route，应由 UI Router/Route Pool 管理。
- `statusline`：只有在明确支持 Claude statusline 集成时才保留，否则应删除或移到独立诊断工具。

### UI reused modules

必须迁出 `ccr-cli` 或至少变成非 CLI crate 的能力：

- Claude Code config activation/deactivation/status snapshot。
- Codex config activation/deactivation/status snapshot。
- OpenCode additive provider add/remove/status。
- OpenClaw additive provider add/remove/status。
- server pid/status helper，如果 UI 和 CLI 都需要，应归属 `ccr-app-core::status` 或 server lifecycle service。

## 设计方向

### 1. 定义 CLI 边界

更新长期文档，明确：

- UI 是 P0/P1 工作流的主入口。
- CLI 是 automation/debug/compatibility surface。
- CLI command 不能直接拥有核心业务实现。
- 新核心逻辑必须进入 `ccr-app-core`、`ccr-config`、`ccr-preset`、`ccr-server` 或新专用 crate。

### 2. 抽出 client config service

优先把 `ccr-cli/src/*_config.rs` 迁移到非 CLI 边界。

可选方案：

- 方案 A：迁到 `ccr-app-core/src/client_config/`。
- 方案 B：新增 `crates/ccr-client-config`，由 UI 和 CLI 同时依赖。

选择标准：

- 如果逻辑只服务 UI 状态/action，优先 `ccr-app-core`。
- 如果逻辑是独立、可被 CLI/server/installer 复用的 config domain，优先新 crate。

### 3. 让 CLI binary 变薄

迁移后：

- `ccr-cli/src/main.rs` 只解析参数、调用 core service、打印结果。
- CLI 不直接修改复杂业务结构。
- CLI 不新增 UI 已有或计划中的核心功能命令。

### 4. 删除或隐藏冲突命令

分阶段处理：

第一阶段：

- 从 README 移除 CLI-first 用法。
- 在 CLI help 文案中标注 debug/automation。
- 不再新增 CLI 核心命令。

第二阶段：

- 删除或 deprecated：`env`、`code`、`ui`、`model`。
- 如果暂时保留，必须在 help 文案中标记 legacy/debug。

第三阶段：

- 评估 `statusline` 是否有明确产品需求。
- 评估 `preset` CLI 是否只保留 export/info。
- 保留 `start/stop/status` 作为自动化接口，或改由 server/app-core 提供统一 lifecycle service。

## 修改文件

预计修改：

- `ARCHITECTURE.md`
- `README.md`
- `docs/long-term-roadmap.md`
- `docs/PRODUCT_SENSE.md`
- `docs/exec-plans/completed/`
- `crates/ccr-cli/src/main.rs`
- `crates/ccr-cli/src/lib.rs`
- `crates/ccr-cli/src/claude_config.rs`
- `crates/ccr-cli/src/codex_config.rs`
- `crates/ccr-cli/src/opencode_config.rs`
- `crates/ccr-cli/src/openclaw_config.rs`
- `crates/ccr-ui/src/status_tab.rs`
- `crates/ccr-app-core/src/`
- 可选新增：`crates/ccr-client-config/`

## 验收测试

文档验收：

```sh
rg -n "CLI is|CLI exists|primary product|desktop app first|ccr-cli" README.md ARCHITECTURE.md docs
rg -n "cargo run --bin ccr -- (claude-activate|codex-activate|model|env|code)" README.md docs
./scripts/check-docs-structure.sh
```

代码结构验收：

```sh
rg -n "use ccr_cli::" crates/ccr-ui crates/ccr-app-core
rg -n "pub mod (claude_config|codex_config|opencode_config|openclaw_config)" crates/ccr-cli/src/lib.rs
```

目标状态：

- `ccr-ui` 不再依赖 `ccr-cli` crate 获取核心 client config 业务。
- `ccr-cli` binary 调用非 CLI crate 的 service API。
- UI 已有核心功能没有只存在于 `ccr-cli` 的实现。

行为验收：

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-cli --lib
cargo test --package ccr-ui --lib
cargo test --workspace
```

手工验收：

- UI 能查看 Claude/Codex/OpenCode/OpenClaw 当前状态。
- UI 能执行 Claude/Codex activate/deactivate 并保留备份/恢复语义。
- UI 能执行 OpenCode/OpenClaw additive add/remove，不覆盖用户非 CCR 配置。
- CLI 如果保留对应命令，只作为同一 core service 的薄包装。
- README 的快速开始仍以 `cargo run --bin ccr-ui` 启动桌面 UI 为主。

## 决策日志

- 2026-05-08：确认产品定位是桌面 UI，但当前开发版本仍通过 `cargo run --bin ccr-ui` 启动。
- 2026-05-08：确认 CLI 尚未完全移除，但不应继续承载 UI 核心功能。
- 2026-05-08：把 `env`、`code`、`ui`、`model` 标为第一批删除或降级候选。

## 完成记录

2026-05-08 完成：

- 将 Claude/Codex/OpenCode/OpenClaw client config 逻辑从 `ccr-cli/src/*_config.rs` 迁移到 `ccr-app-core/src/client_config/`。
- `ccr-ui` 不再依赖 `ccr-cli` crate。
- tray menu 不再 shell 到 `ccr` 命令执行 start/stop/activate，而是直接调用 `ccr-app-core` service。
- `ccr-cli` 删除 `env`、`code`、`ui`、`model` 命令。
- `ccr-cli` 保留 `start`、`stop`、`restart`、`status`、Claude/Codex activate/deactivate、`preset` 和 `statusline`，作为自动化/调试/兼容入口。
- README 的 CLI 示例移除 `model` 命令，Quick Start 保持桌面 UI 优先。

验证命令：

```sh
cargo fmt --check
cargo test --package ccr-app-core --lib
cargo test --package ccr-cli
cargo check --package ccr-ui
rg -n "ccr_cli|pub mod (claude_config|codex_config|opencode_config|openclaw_config)|cargo run --bin ccr -- (claude-activate|codex-activate|model|env|code)|Commands::(Env|Code|Ui|Model)|run_ccr_command" README.md docs crates/ccr-ui crates/ccr-cli crates/ccr-app-core
```

保留项：

- `statusline` 仍保留，后续如果没有明确产品集成需求可单独删除。
- `preset` CLI 仍保留为调试/导出入口，preset 主工作流仍应在 UI。
