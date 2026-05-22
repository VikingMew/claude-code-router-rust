# Original CCR Feature Gap Analysis

**状态：** 长期跟踪文档  
**最后验证：** 2026-05-22
**范围：** 记录原版 JS/TS `claude-code-router` 支持、但当前 `ccr-rust` 未支持或未完整支持的能力  
**用途：** 和 `cc-switch-feature-gap-analysis.md` 分开维护，避免把原版 CCR 缺口和 cc-switch 缺口混在一起

## 产品边界

Rust 版是 UI 优先的桌面程序。原版 CCR 是 Node/TypeScript CLI/server/web UI 项目。对齐原版 CCR 时，优先保证桌面 UI、server routing 和 client 注入闭环。

原版 CLI 的参数名、命令结构和交互形式不作为 Rust 版需要跟踪的边界。需要关注的是这些 CLI 背后代表的产品能力，例如启动 client、应用 profile、安装 preset、输出 statusline 数据、非交互自动化等。

Gemini 相关能力不进入当前 Rust 产品目标。原版 CCR 中与 Gemini 相关的 transformer、CLI 或配置能力在本文只作为“原版存在但当前划掉”的信息，不作为近期实现要求。

## 当前 Rust 已具备的原版相关能力

- Provider / Router / Transformer 基础配置模型
- Router 场景：default、background、think、longContext、webSearch、image
- Subagent model tag 路由
- Custom JS router，使用 QuickJS 执行 `router.js` 或 `CUSTOM_ROUTER_PATH`
- SSE parser、serializer、rewrite stream
- Image agent 基础能力
- Config load/save、环境变量插值、JSON5、备份
- Admin reload/config 相关基础接口
- Server lifecycle、client activate/deactivate、preset/statusline 等基础 CLI 辅助能力；当前 CLI 命令面为 `start`、`stop`、`restart`、`status`、`claude-activate`、`claude-deactivate`、`codex-activate`、`codex-deactivate`、`preset`、`statusline`、`help`
- Claude / Codex activate/deactivate
- Preset 基础 export/list/info/delete/load/install library 能力
- Desktop UI：status、config、settings、logs、presets、transformers、token counter、endpoint test
- Codex `/v1/responses` 入口
- Through-CCR upstream pipeline：Messages/Responses 入站会按目标 provider 重建 body/header，并在 Route Pool attempt 中重新应用
- 多个常用 transformer 已移植：anthropic、openai、deepseek、cleancache、tooluse、reasoning、forcereasoning、streamoptions、openrouter、groq、enhancetool、vertex-claude、cerebras、vercel、maxtoken、maxcompletiontokens 等
- Rust 额外能力：provider API kind 显式/推断、Route Pool ordering、endpoint speed test、auto launch、Claude/Codex 原生配置注入
- Claude Code model env 独立配置，不再从 CCR route model 推导

## 功能缺口总表

| 功能区 | 原版 CCR 支持 | ccr-rust 当前状态 | 决策 |
| --- | --- | --- | --- |
| CLI-backed capabilities | 原版 CLI 暴露启动、profile、preset、statusline、activate、ui 等能力 | Rust 已有部分能力，形态以 UI 和 core API 为准 | 跟踪能力，不跟踪参数 |
| Preset marketplace | file/url/name install、manifest metadata、schema prompt、conflict strategies | 基础 preset library 和部分 UI | 需要补齐 |
| Custom transformers | config 加载外部 JS transformer/plugin | 只有内置 Rust transformer | 重要缺口 |
| Transformer parity | 原版依赖 `@musistudio/llms` 生态 | 已移植常用 transformer，仍非完全等价 | 分阶段补齐 |
| Config schema | 原版 README 的完整字段和环境变量 | Rust 有核心字段，已加入 Claude Code model mapping，仍有部分缺口 | 长期补齐 |
| Web UI parity | React web 管理界面 | Rust desktop UI，功能形态不同 | 以桌面 UI 为准 |
| Server API parity | 原版 server API、preset API、UI API | Rust 有基础 API | 按 UI 需要补齐 |
| Responses/Anthropic 转换 | 原版通过 transformer 生态处理 | Rust 已有最小 provider API kind 协议矩阵、Responses reasoning 降级策略、非流式 tool request 映射、image multimodal 映射、stateful 字段保留/拒绝策略和不兼容跨协议 streaming gate；完整 stream event schema、streaming tool delta 和更细 provider-specific multimodal 边界仍缺 | 继续补齐 |
| Non-interactive / CI | 原版支持 CI 环境变量和非交互 | Rust 不完整 | 后置 |
| Logs | pino server logs + app logs | Rust 基础日志查看 | 继续补查询/下载 |
| Statusline | 原版 statusline 配置更完整 | Rust 有基础 statusline | 后置补齐 |
| Packaging | npm package、Node server、web UI | Rust desktop/package | 不要求一比一 |

## 详细缺口

### 1. Preset 系统不完整

原版 CCR 的 preset 系统包含导出、安装、列表、详情、删除、manifest metadata、动态 schema、敏感字段占位、merge conflict 策略和交互式安装能力。

Rust 当前已有基础 preset crate，但还缺：

- preset install 能力的完整入口，入口形态优先 UI/core API
- 从 URL / GitHub / registry 安装
- preset marketplace 或内置 preset registry
- manifest metadata 完整字段
- schema prompt 的完整交互流程，优先 UI
- conflict strategy：ask、overwrite、merge、skip
- install preview
- preset update / version check
- preset 和 default profile 的统一模型

Phase 45 应优先复用 preset 系统，不再新增一套重复的 default profile 概念。

### 2. Custom transformer plugin 加载

原版 CCR 可以在配置中声明外部 transformer，并由 Node 运行时加载 JS transformer。Rust 当前只有内置 transformer registry。

仍缺：

- `transformers` 配置字段的完整兼容
- 外部 transformer manifest/schema
- JS/WASM/plugin sandbox 策略
- transformer options 的通用传递
- model-specific transformer override 的完整 UI
- plugin 错误隔离和日志

这是和原版 CCR 最大的 server 能力差异之一。

### 3. Transformer parity

Rust 已经移植多个常用 transformer，但不是原版 `@musistudio/llms` 的完整等价实现。

需要继续核对：

- `sampling`
- `customparams`
- `gemini-cli`（当前产品划掉）
- `qwen-cli`
- `chutes-glm`
- 各 transformer 的 streaming 差异
- tool use / enhance tool 的边界情况
- Anthropic Messages 与 OpenAI Responses 双向转换

涉及 Gemini 的部分只记录，不进入近期目标。

### 4. 原版 CLI 背后的能力仍需核对

Rust CLI 当前只暴露 `start`、`stop`、`restart`、`status`、`claude-activate`、`claude-deactivate`、`codex-activate`、`codex-deactivate`、`preset`、`statusline` 和 `help`。`env`、`model`、`ui` 不属于当前 CLI 命令面。原版 CCR 的 CLI 参数和命令形态不作为 Rust 版兼容目标；本文只按背后的产品能力继续核对。

仍需按能力核对：

- 启动 Claude Code 或其他 client 的能力是否完整
- preset install 能力是否补齐到原版行为
- model/profile 选择和保存能力是否等价
- statusline 的数据模型、输出字段和配置项是否等价
- `NON_INTERACTIVE_MODE` 等 CI 行为
- shell activate / client 注入背后的环境配置能力

### 5. Config schema 和环境变量

原版 CCR README 暴露了更多配置和环境变量。Rust 当前 schema 更收敛。

仍需核对：

- `API_TIMEOUT_MS`
- `LOG`
- `LOG_LEVEL`
- `NON_INTERACTIVE_MODE`
- `StatusLine`
- `claudeCodeSettings`
- Claude Code model env 与 CCR route model 的边界
- 自定义 transformer 配置
- transformer 全局配置和 model override 的细节兼容
- preset 中携带 client settings 的能力

Rust 已有 `AppSettings.log_level`、proxy、Route Pool、auto launch 等字段，但不是原版 schema 的完整镜像。

### 6. Web UI 与 Desktop UI 的差异

原版 CCR 的 UI 是 React/Vite web 管理界面。Rust 版是桌面 UI，且产品方向是 UI 优先。

不要求像素级或路由级 parity，但仍缺：

- 更完整的 preset install flow
- transformer plugin 管理
- provider/model 表单校验
- config diff/preview
- logs 查询和下载
- statusline 设置
- advanced server/env settings

### 7. Server API parity

原版 server 面向 web UI 和 CLI 暴露更多 API。Rust 目前以桌面 UI 需要为准。

仍需按需补齐：

- preset install/list/detail/delete API 完整性
- transformer list/config API
- config validation API
- log query API
- statusline/config API
- service lifecycle API 的错误语义

### 8. Responses 到 Anthropic Messages 的转换可靠性

Rust 已经有 Codex `/v1/responses` 入口、provider API kind aware upstream builder 和一组最小可靠 Responses 映射能力：

- Anthropic Messages、OpenAI Chat Completions、OpenAI Responses 三种 provider API kind 的基础 body/header 转换矩阵。
- Responses `reasoning` 字段发往 OpenAI Responses upstream 时保留，发往非 Responses provider 时不透传非法 Responses-only 字段。
- 非流式 request body 的基础 tool request 映射，包括 Anthropic `tool_use` 到 OpenAI Chat `tool_calls` 的方向。
- text/image content 的有限 multimodal 映射，不支持的 content type 返回明确错误。
- Responses stateful 字段发往 Responses upstream 时保留；state-only 请求发往非 Responses provider 时明确拒绝。
- 不兼容跨协议 streaming 当前被 gate 拒绝，避免把错误 SSE schema 直接透传。

“Codex 使用 Claude Sonnet”仍需要继续补齐的真实边界：

- 完整跨协议 stream event schema 转换。
- streaming tool delta 的跨协议转换。
- 更细 provider-specific multimodal 能力和限制表达。
- tool result、error、max token / sampling 等字段的更多协议边界测试。

这部分需要专门测试，不应只靠 UI 能写配置来判断完成。

### 9. Logging 和 observability

原版 CCR 有 server-level 和 application-level logs 的约定。Rust 目前有基础日志查看，但还缺：

- HTTP request log 查询
- routing decision log 查询
- transformer error 结构化展示
- Route Pool event log
- 下载/清理日志
- log level 与运行时 reload 的一致行为

### 10. 原版存在但当前不追踪的能力

这些能力来自原版或其生态，但当前 Rust 产品目标不优先：

- Gemini 相关配置和 transformer
- Node/npm 形态的一比一发布
- 原版 web UI 的完整复刻
- 原版外部依赖生态的全量兼容

## 建议优先级

### P0 / 近期

- Phase 45 默认配置 profiles，复用 preset 模型
- Codex Responses 到 Anthropic Messages 的剩余协议边界测试，重点是完整 stream event schema、streaming tool delta、provider-specific multimodal 和 tool result/error 参数边界
- Provider API kind 已接入 server upstream 基础构建；仍需补齐 endpoint test、transformer 推荐和 direct-to-provider 可选模式预览；默认 client injection 仍只指向本地 CCR server
- Preset install 的 UI/core 最小闭环

### P1

- Custom transformer plugin 设计
- Preset marketplace/source install
- Config schema compatibility audit
- Logs 查询和导出

### 后期

- 原版 CLI 背后的能力补齐
- Statusline 完整配置
- CI / non-interactive mode
- 原版 web UI 专属能力迁移
