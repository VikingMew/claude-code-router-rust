# ccr-28 - Official Provider Profiles

**状态：** active
**优先级：** P1
**计划编号：** ccr-28
**最后更新：** 2026-05-22

## 目标

把 Anthropic Official、OpenAI / Codex Official 和 GitHub Copilot Official 作为可应用、可排序、可诊断的内置 profile/provider 接入现有 Route Pool 模型。

## 非目标

- 不新增 direct-to-provider 默认行为。
- 不改变 Claude/Codex/OpenCode/OpenClaw/Hermes 默认 client injection 语义。
- 不实现 Copilot OAuth、IDE token 抓取、`copilot_internal/*` refresh 或 IDE header 仿冒。
- 不在 preset、默认配置、测试快照或 UI 文案中持久化真实 secret。

## 背景

Linear CCR-28 最新人工评论要求 UI-first：官方 provider 应提示来自已登录 Claude/Codex/Copilot，能在 CCR activate 后继续识别安全备份里的原始登录状态。Copilot 第三方参考只能作为非权威调研边界，不能成为授权来源。

官方 reference：

- Anthropic Messages endpoint: `https://api.anthropic.com/v1/messages`
- OpenAI Responses endpoint: `https://api.openai.com/v1/responses`
- OpenAI Codex model reference: `https://developers.openai.com/api/docs/models/gpt-5.3-codex`，2026-05-22 查询显示 `gpt-5.3-codex` 是默认 Codex model，并支持 `v1/responses`。
- GitHub Copilot LLM endpoint: `https://api.githubcopilot.com/chat/completions`，认证来自 agent 收到的 GitHub token/header，不提供 CCR 可安全读取的本地登录 token 来源。

## 设计方向

- 内置官方 profile 使用非 secret resolver marker，不使用 env placeholder。
- app-core 增加官方登录/secret detection：可用、备份可识别、未找到、官方来源不支持安全读取。
- Endpoint test 和 server upstream headers 使用同一 resolver。
- GitHub Copilot profile 展示官方 endpoint，但不自动启用 Route Pool candidate。

## 修改文件

- `crates/ccr-preset/src/lib.rs`
- `crates/ccr-app-core/src/official_provider.rs`
- `crates/ccr-app-core/src/endpoint.rs`
- `crates/ccr-app-core/src/status.rs`
- `crates/ccr-server/src/lib.rs`
- `crates/ccr-ui/src/preset_tab.rs`
- `crates/ccr-ui/src/status_tab.rs`
- `docs/provider-api-kinds.md`

## 验收测试

- `cargo test --package ccr-preset`
- `cargo test --package ccr-app-core`
- `cargo test --package ccr-server`
- `cargo test --workspace`

## Do / 执行记录

- 实际修改:
  - 校准 `anthropic-official` 和 `openai-codex-official`，新增 `github-copilot-official`。
  - 官方 profile 改用 `ccr-secret://official/*` resolver marker，不再要求 env placeholder。
  - 新增 app-core 官方凭据状态检测和 resolver，覆盖当前官方 client 文件与 CCR activation backup。
  - Endpoint test 和 server upstream header 统一通过 resolver 获取实际 secret；缺失或不支持时返回诊断，不记录 secret。
  - Status/Preset UI 展示官方 provider 登录状态。
  - 更新 provider API kind 文档和官方 reference。
- 实际偏离计划:
  - GitHub Copilot 仅展示官方 endpoint profile，Route Pool candidate disabled，不自动启用。
  - 无法完成 workspace/server/ui 编译验证，原因是本机磁盘空间耗尽。
- 中途决策:
  - Copilot 不实现本地 token 读取，因为 GitHub 官方文档只说明 Copilot agent 场景中的 agent-provided token/header。

## Check / 验证与偏差

- 验证命令:
  - `cargo fmt --check` - pass。
  - `cargo test --package ccr-preset` - pass，12 tests。
  - `cargo test --package ccr-app-core` - pass，183 tests。
  - `cargo test --package ccr-server` - blocked by environment: `No space left on device` while compiling dependencies before CCR source compile.
  - `cargo test --package ccr-ui` - blocked by environment: `No space left on device` while creating target files.
  - `cargo test --workspace` - not run because disk had only about 117 MiB available before cleaning target and about 947 MiB after cleaning target, still insufficient for server/UI/workspace dependency builds.
- 手工 QA:
  - 代码审查确认 Copilot profile 不引用 `copilot_internal/*`、IDE token 抓取、IDE header 仿冒或 refresh flow。
- 发现的偏差:
  - 广泛验证受本地磁盘空间限制。
- 代码和文档不一致:
  - 已更新 `docs/provider-api-kinds.md` 记录官方 profile/resolver 和 Copilot 限制。

## Act / 处理与沉淀

- 已处理偏差:
  - Copilot 限制写入 profile 描述、status 诊断和长期文档。
- 长期文档更新:
  - `docs/provider-api-kinds.md`
- 新增/更新技术债:
  - 无。
- 后续计划:
  - 无。

## 决策日志

- 2026-05-22: Copilot official profile 使用 provider-only disabled candidate，因为 GitHub 官方文档没有给 CCR 可安全读取本地 Copilot 登录 token 的稳定来源。

## 完成记录

- 2026-05-22: 实现完成；target 清理后仍无法完成 server/ui/workspace 编译验证，阻塞原因为本机磁盘空间不足。
