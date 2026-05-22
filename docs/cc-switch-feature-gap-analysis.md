# cc-switch Feature Gap Analysis

**状态：** 长期跟踪文档  
**最后验证：** 2026-05-22
**范围：** 记录 `../cc-switch` 支持、但当前 `ccr-rust` 未支持或未完整支持的能力  
**用途：** 作为后续 phase 拆分依据，不代表所有功能都必须实现

## 产品边界

`ccr-rust` 是 UI 优先的桌面程序。后续设计、验收和测试应优先围绕 UI 完成闭环；CLI 只作为后置能力、调试入口或自动化辅助。

`cc-switch` 和原版 CCR 的 CLI 参数、命令命名、命令层交互不作为 Rust 版需要跟踪的边界。文档只跟踪这些入口背后的能力，例如 provider 切换、client 注入、server lifecycle、statusline、preset/profile 应用和非交互自动化。

以下能力不纳入当前 Rust 版目标：

- Gemini 相关能力
- Prompt 管理

以下能力保留，但放到后期：

- MCP 管理
- Skills 管理

## 当前代码已实现范围

本节只记录当前代码已经实现的范围，不表示对应功能已经完整追平 `cc-switch`。

已实现：

- Claude Code / Codex 的原生配置写入路径、backup、activate/deactivate 和状态检测
- Claude Code through-CCR 模式下的 client-visible model mapping：`ANTHROPIC_MODEL`、haiku、sonnet、opus 可在 Advanced 设置中配置
- Server start / stop / restart / status
- Status 作为首屏，页面打开和刷新时读取 live 状态
- Tray task icon 的基础入口
- Codex `/v1/responses` 后端入口
- Provider API kind 的选择、推断、显式选择状态
- Provider / Router / Transformer 的基础配置 UI；Router 已使用左右双列表的 Route Pool 工作区
- Server through-CCR pipeline：`/v1/messages` 和 `/v1/responses` 共用 upstream request 构建，按 provider kind 生成 body/header
- Responses 最小可靠映射：基础 provider API kind 协议矩阵、Responses reasoning 保留/降级策略、非流式 tool request 映射、text/image multimodal 映射、stateful 字段保留/拒绝策略，以及不兼容跨协议 streaming gate
- Route Pool provider/model 排序，以及网络错误、5xx、429 的 server retry
- Endpoint candidates、批量测速、stream heuristic、测速结果写入 Route Pool
- 开机自启动设置
- 设置页主分组骨架
- 基础 preset export/list/delete、preset library install、内置 default profiles、日志查看、token counter
- 配置备份、环境变量插值、proxy 基础配置

同一批功能仍未完成的范围：

- Client 注入当前只负责把 Claude/Codex 指向本地 CCR server；Claude Code model env 已独立于 CCR route model；尚未提供 direct-to-provider 模式选择和预览
- Tray task icon 只是基础入口，不等价于完整桌面控制面板
- Codex `/v1/responses` 到目标 provider 的最小协议转换已接入；剩余边界是完整跨协议 stream event schema 转换、streaming tool delta、provider-specific multimodal 细节，以及 tool result/error/max token/sampling 等更多协议边界测试
- Provider / Router / Transformer UI 仍缺完整校验、diff、preview 和插件管理
- Route Pool 仍缺 health score、event log、retry budget、清除 ban 状态和真实 HTTP retry 链路 mock 测试
- Endpoint test 仍缺历史记录、模型级结果和 health/Route Pool 联动
- 设置页仍缺 language、terminal、per-client directory override、per-provider proxy、backup restore、import/export、auto update、WebDAV
- Preset 仍缺 marketplace/source install、动态 schema 完整交互、conflict strategy、update/version check、client-specific 模板元数据
- Proxy 仍缺 cc-switch 的 takeover、hot switch、adapter、body filter、error mapper、health

## 功能缺口总表

| 功能区 | cc-switch 支持 | ccr-rust 当前状态 | 决策 |
| --- | --- | --- | --- |
| 多客户端支持 | Claude、Codex、OpenCode、OpenClaw、Gemini | Claude/Codex/OpenCode/OpenClaw 已有 through-CCR 注入；Gemini 划掉 | 继续补齐统一 client 抽象和预览 |
| Provider 存储 | SQLite DB、current provider、切换历史、迁移 | JSON config 为主 | 长期缺口 |
| Universal Provider | 一个 provider 同步到多个 client | 无 | 长期缺口 |
| Provider presets | client-specific 官方/第三方 presets、图标、模板 | 已有 preset export/list/delete、library install 和内置 default profiles；缺 marketplace、schema 交互、conflict strategy、client-specific 模板元数据 | 后续扩展 |
| Client live config | 按 client 写原生配置 | 已有 Claude/Codex 覆盖式写入、backup、状态检测；已有 OpenCode/OpenClaw additive CCR provider 写入/移除/状态检测；默认写本地 CCR server；Claude Code model mapping 可配置；缺 direct-to-provider 模式选择和预览 | 继续补齐 |
| MCP 管理 | import/sync/toggle/validate/presets | 无 | 后期 |
| Skills 管理 | repo/install/import/sync | 无 | 后期 |
| 本地代理接管 | proxy takeover、hot switch、adapter、health | CCR server routing | 长期缺口 |
| Responses 协议转换 | provider adapter 中处理 Responses、Messages、Chat 等请求/响应 shape | 已有最小 request/header 转换、Responses reasoning 降级、非流式 tool request 映射、image multimodal 映射、stateful 字段策略和 streaming gate；缺完整 stream event schema、streaming tool delta、provider-specific multimodal 能力矩阵 | 继续补齐 |
| Route Pool | 排序、策略、UI、health/circuit breaker | 已有左右双列表 Route Pool UI、连续失败 ban、网络/5xx/429 retry，且每次 attempt 会按目标 provider 重建 body/header；缺 health score、事件日志、retry budget、清除 ban 状态 | 继续补齐 |
| Usage 统计 | 请求日志、价格、用量、脚本统计 | 基础日志 | 长期缺口 |
| Session 管理 | session 列表、消息、resume | 无 | 后期 |
| Workspace | workspace 文件管理 | 无 | 后期 |
| WebDAV 同步 | 配置/数据同步 | 无 | 后期 |
| Import / Export | 完整 app 数据导入导出 | preset 级别 | 后期 |
| Deep links | provider/MCP/skill import | 无 | 后期 |
| 自动更新 | Tauri updater UI | 无 | 后期 |
| 开机自启动 | enable/disable/status | 已有 enable/disable/status 设置；缺更完整的平台边界展示和 UI 验证 | 继续补齐 |
| 代理设置 | global proxy、per-provider proxy | global/基础字段 | per-provider proxy 缺 |
| Endpoint speed test | endpoint 批量测速、排序、模型检查 | 已有批量测速、stream heuristic、写入 Route Pool；缺历史记录、模型详情、health 联动 | 继续补齐 |
| 完整设置页 | theme、language、terminal、目录 override、备份等 | 已有设置页分组骨架；缺 language、terminal、目录 override、备份恢复、import/export 等实际能力 | 继续补齐 |
| 数据备份 | DB backup、配置备份列表 | 文件级 backup | 备份管理 UI 缺 |
| UI 体验 | provider card、icon、wizard、dashboard | 基础桌面 UI | 长期改善 |

## 详细缺口

### 1. 多客户端应用模型

`cc-switch` 把不同 AI coding client 作为一等 app 管理。当前 Rust 版已经覆盖 Claude Code、Codex、OpenCode、OpenClaw 的 through-CCR 注入，但尚未建立统一的 `ClientApp` 抽象。

仍缺：

- 每个 client 的 config directory override
- client-specific provider form 和状态页
- 跨 client 的统一状态模型
- OpenCode MCP 同步、OpenClaw MCP/env/tools/session 等深度能力

Gemini 相关能力已划掉，不进入当前目标。

### 2. Provider 数据库和 current provider 状态

`cc-switch` 使用数据库保存 provider、settings、current provider、universal provider、proxy 和 fallback routing 配置，并带迁移逻辑。

相关能力当前 Rust 没有：

- provider DB schema
- current provider 持久化
- provider add/update/delete 的结构化 API
- provider switch 前回填 live config
- provider migration
- provider icon、排序、分类、metadata
- provider common config snippet 提取

Rust 目前仍以 `config.json` 为核心。短期可以继续保留这个模型，但如果要做到 cc-switch 的 provider 管理体验，需要引入 provider store 或在 config 上补齐等价抽象。

### 3. Universal Provider

`cc-switch` 的 universal provider 可以把同一个 provider 映射到多个 client，并分别生成对应 client 的模型、endpoint、auth 和写入规则。

当前 Rust 没有 universal provider 概念。Phase 53 已经加入 provider API kind，但它只解决“这个 provider 应该按什么 API 使用”，还没有解决“一个 provider 如何写入多个 client”。

需要补齐：

- universal provider schema
- per-client mapping
- per-client model defaults
- 从一个 provider 生成 Claude/Codex/OpenCode/OpenClaw live config
- UI 表单和预览

### 4. Provider presets 和默认配置

`cc-switch` 对 Claude、Codex、OpenCode、OpenClaw 有独立 provider presets，并带 `apiFormat`、endpoint candidates、icon、theme、模板字段。

Rust 当前的 preset 已经复用为默认配置入口。Phase 45 已补齐内置 default config profiles：

- Anthropic Official
- OpenAI / Codex Official
- Claude Code with GPT / Codex
- Codex with Claude Sonnet

这些 profile 会写入 provider、`ProviderApiKind`、Route Pool、required env，并通过 Presets 页面应用。

仍缺：

- endpoint candidates 写入 profile
- provider icon/theme/category
- template form 和 required field UI
- profile merge/overwrite preview

### 5. Client live config 和 provider kind 的边界

Phase 46 已经修正 Claude/Codex 的原生写入方向，Phase 53 已经加入 `ProviderApiKind` 和显式/推断选择。

默认 through-CCR 模式下，Claude/Codex 不需要知道上游 provider 类型。client 只应该连到本地 CCR server，例如 `http://127.0.0.1:<port>`；Anthropic/OpenAI/OpenRouter/DeepSeek 等上游差异由 CCR server、router、transformer 和 endpoint test 处理。

剩余缺口是把边界表达清楚：

- Claude/Codex 注入 UI 明确展示当前使用 through-CCR 模式，只写本地 server endpoint
- Claude Code model env 是 client-visible model mapping，不能从 CCR route model 自动推导
- Provider kind 只影响 CCR server 如何请求上游、测速 payload、transformer 推荐和配置校验
- 如果未来支持 direct-to-provider，才需要根据 provider kind 判断能否直接写入 client 原生配置
- UI 可以提供 direct-to-provider 作为高级可选模式，但默认不能把 provider kind 泄漏到 client 注入逻辑
- 用户显式选择过 provider kind 后不能被推断覆盖
- provider kind 变更后，Config、Endpoint Test、Server routing 使用同一套解析结果

### 6. Responses 协议转换边界

当前 Rust server 已完成最小可靠能力：

- `/v1/messages` 和 `/v1/responses` 会按目标 provider API kind 重建 upstream body/header。
- OpenAI Responses upstream 保留兼容的 Responses `reasoning` 字段；OpenAI Chat 和 Anthropic Messages upstream 不透传非法 Responses-only reasoning 字段。
- 非流式 request body 已有基础 tool request 映射，包括 Anthropic `tool_use` 到 OpenAI Chat `tool_calls`。
- text/image content 已有有限 multimodal 映射；不支持的 content type 会明确拒绝。
- Responses `previous_response_id`、`conversation_id`、`prompt` 等 stateful 字段在 Responses upstream 保留，state-only 请求发往非 Responses provider 时明确拒绝。
- 不兼容跨协议 streaming 当前会被 build error gate 阻止，避免错误 SSE schema passthrough。

仍缺的真实边界：

- 完整跨协议 stream event schema 转换。
- streaming tool delta 的跨协议转换。
- 更细的 provider-specific multimodal 能力矩阵、UI 提示和配置校验。
- tool result、error、max token、sampling 等字段的更多协议边界测试。

### 7. 本地代理接管和协议转换

`cc-switch` 的 proxy 是桌面应用内部的一套代理接管系统，支持 provider adapter、request/response transform、streaming、body filtering、error mapping、model mapping 和 health。

Rust 现在有 CCR server routing，但不是 cc-switch 那种 hot switch proxy takeover。

仍缺：

- hot provider switch
- provider adapter abstraction
- proxy takeover UI
- per-provider auth transform
- response processing pipeline
- streaming adapter
- body filter
- model mapper
- error mapper
- health endpoint 和 UI 状态

### 8. Route Pool 高级能力

Route Pool 已经覆盖 provider/model 排序、UI、server retry 和 circuit breaker 运行时状态。Phase 67 把 Router 页面改成左右双列表的 Route Pool 工作区：左侧是 available routes，右侧是 active pool 顺序。Phase 68 删除了旧 routing 兼容入口和导入路径。

仍缺 cc-switch 更完整的运行时能力：

- provider health score
- route pool event log
- retry budget / cooldown
- 清除单个 route ban 状态
- request queue state
- 测速、health、Route Pool 的联动策略

### 9. Usage、价格和统计

`cc-switch` 有 usage、价格、脚本统计和 dashboard 方向的实现。Rust 当前只有基础日志查看。

仍缺：

- 请求级 usage 记录
- token usage 持久化
- model price 配置
- provider/model cost dashboard
- 时间范围筛选
- 导出统计

### 10. 设置页剩余内容

Phase 49 已经完成完整设置页的主结构，但部分设置仍是轻量实现或占位。

仍缺：

- language
- terminal preference
- per-client directory override
- per-provider proxy
- backup list / restore
- app import/export
- auto update settings
- WebDAV settings
- 更完整的 theme / appearance 细节

### 11. 后期保留能力

这些能力在 cc-switch 中存在，但当前 Rust 版不应立即展开：

- MCP 管理
- Skills 管理
- Session 管理
- Workspace 文件管理
- WebDAV 同步
- Deep links
- Auto update
- OpenCode / OpenClaw 深度集成（MCP、env/tools、session、插件生态）

## 建议优先级

### P0 / 近期

- 明确 Claude/Codex 注入默认只写本地 CCR server endpoint
- Provider kind 已接入 server upstream 构建；仍需补齐 endpoint test、transformer 推荐、配置校验展示和 direct-to-provider 可选模式预览
- 补齐 status 页面中 activate/start server 的可用性判断

### P1

- Universal provider 的最小模型
- Provider presets 的 client-specific 模板
- per-provider proxy
- endpoint test 历史和模型级结果
- Route Pool health / circuit breaker

### 后期

- MCP / Skills
- Usage dashboard
- Session / Workspace
- WebDAV / deep link / auto update
- OpenCode / OpenClaw 深度集成
