# Client Model Mapping

**状态：** 长期设计文档
**最后验证：** 2026-05-21
**范围：** 记录 client-visible model mapping 与 CCR upstream routing model 的产品边界

## 产品边界

`ccr-rust` 是 UI 优先的桌面程序。Claude Code 和 Codex 的 client 配置写入应优先服务桌面 UI 的 activate/status/settings 闭环。

默认模式是 through-CCR：

- Claude Code 连接 `http://127.0.0.1:<port>`
- Codex 连接 `http://127.0.0.1:<port>/v1`
- OpenCode、OpenClaw 和 Hermes Agent 通过 additive provider/custom-provider entry 连接 `http://127.0.0.1:<port>/v1`
- 上游 provider、真实模型、协议转换、transformer、Route Pool 由 CCR server 决定

client 配置里的模型字段不等于上游 provider 的模型字段。

## Claude Code 模型映射

Claude Code 支持通过环境变量表达模型默认值或模型类别：

- `ANTHROPIC_MODEL`
- `ANTHROPIC_DEFAULT_HAIKU_MODEL`
- `ANTHROPIC_DEFAULT_SONNET_MODEL`
- `ANTHROPIC_DEFAULT_OPUS_MODEL`

这些字段是 Claude Code 侧的模型映射。它们用于让 Claude Code 生成请求时选择它认识的模型名或模型类别。CCR 收到请求后，再通过 Router 场景路由和 Route Pool 把请求映射到真实上游 route。

因此默认 through-CCR 模式不应该把 CCR 上游 route model 写入这些 env。例如 `glm-4.6`、`gpt-5-codex` 这类上游模型名不应该自动成为 Claude Code 的默认 haiku/sonnet/opus。

## CCR 上游路由模型

CCR 的真实上游模型来源于：

- `RoutePool` 第一条启用 route
- `Router.background`
- `Router.think`
- `Router.longContext`
- `Router.webSearch`
- `Router.image`
- Project-level router
- Custom router
- Route Pool candidate order

这些 route 通常使用 `provider,model` 形式，例如：

```json
{
  "RoutePool": {
    "enabled": true,
    "candidates": [
      { "route": "zhipu,glm-4.6", "enabled": true, "priority": 0 }
    ]
  },
  "Router": {
    "think": "anthropic,claude-sonnet-4-20250514"
  }
}
```

这些 route 只应该由 CCR server 解释。Claude Code 不需要知道 `zhipu`、`openai`、`openrouter` 或 `anthropic` provider kind。

## 关键用户故事

这些 story 用来校准长期计划，避免把不同层级的模型配置混在一起。

### 1. Provider-only 原样透传

用户想使用 `zenmux` 的模型，并把 Claude Code 发来的请求全部原样转发给 `zenmux`。

期望行为：

- Route Pool 里选择 `zenmux` 这种 provider-only route。
- CCR 只负责把请求送到 `zenmux` 的 endpoint。
- 上游 request body 里的 model 使用 client 入站请求里的 model。
- CCR 不自动替换成 `zenmux` 的某个固定模型。

这个 story 对应 UI 文案应是 `Use request model`，而不是 `Keep inbound` 这类内部术语。

### 2. 固定一个上游模型作为 Claude Code 全部后端

用户想把 `zenmux` 的 `gpt-5.5` 作为 Claude Code 所有请求的后端。

期望行为：

- Route Pool 里选择 `zenmux,gpt-5.5` 这种 fixed-model route。
- 不管 Claude Code 入站请求里是什么 model，CCR 发往上游时都使用 `gpt-5.5`。
- Claude Code client 配置仍只指向本地 CCR，不需要知道 `zenmux` 的 endpoint 或 provider kind。

这个 story 对应 UI 文案应是 `Fixed model`。

### 3. 只替换 Claude Code 的某一类模型

用户想把 `zenmux` 的 `glm` 作为 Claude Code Sonnet 请求的后端，其他 Claude Code 模型类别保持不变。

期望行为：

- UI 需要表达“当入站请求匹配 Sonnet 类别时，改写到 `zenmux,glm`”。
- Haiku、Opus 或其他入站模型不应被这个规则影响。
- 这不是 Claude Code env model mapping 本身，也不是单一 Route Pool 首项能完整表达的策略。

这类需求需要显式的 model mapping / route rule：

- match：client app = Claude Code，client-visible model/category = Sonnet
- action：upstream route = `zenmux,glm`
- fallback：未命中的请求继续走 Route Pool 或其他已配置路由策略

这个 story 是 Router UI 的重要边界：Route Pool 解决 provider 顺序和整体后端选择；per-client / per-model route rule 解决“只替换某一类请求”的精细映射。

## 参考边界

`cc-switch` 的 Claude provider 表单支持配置：

- `ANTHROPIC_MODEL`
- `ANTHROPIC_DEFAULT_HAIKU_MODEL`
- `ANTHROPIC_DEFAULT_SONNET_MODEL`
- `ANTHROPIC_DEFAULT_OPUS_MODEL`

原版 CCR 的 activate env 主要负责让 Claude Code 连接本地 CCR server，例如 base URL、auth token、proxy、telemetry 和 timeout 相关设置。原版默认不会把当前 route model 强写进 Claude Code model env。

设计边界：

- Claude Code model env 是 client-visible model mapping，不是 CCR upstream route。
- CCR route model 不自动写入 Claude Code model env。
- 默认 through-CCR 模式下，client 只连接本地 CCR。
- Provider API kind 只影响 CCR server 请求上游，不影响 Claude Code endpoint 注入。

## Router UI 设计要求

Router UI 需要提供这些配置入口：

- Route Pool 第一条启用 route 的明确“主路由”解释。
- `Router.background`
- `Router.think`
- `Router.longContext`
- `Router.webSearch`
- `Router.image`
- per-client / per-model route rule，例如 Claude Code Sonnet -> `zenmux,glm`

Router UI 需要解释这些规则之间的关系：

- 场景路由命中时使用对应场景 route。
- 场景 route 未配置或为空时使用 Route Pool。
- Route Pool 按启用顺序和运行时健康状态选择候选 route。
- provider-only route 表示 `Use request model`。
- `provider,model` route 表示 `Fixed model`。
- per-client / per-model route rule 用于精细替换某类 client-visible model，不应该混同于 Claude Code env model mapping。

## Client 注入设计要求

- Claude Code 和 Codex 默认写入本地 CCR endpoint。
- OpenCode、OpenClaw 和 Hermes Agent 默认只增加或更新 CCR-owned provider entry，不覆盖用户的其他 providers、tools、profiles、MCP 或 secrets。
- OpenCode、OpenClaw 和 Hermes Agent 的 client-visible model 从 Route Pool 第一条启用 route 的 `provider,model` 中只取 `model` 部分，不把 `openai,`、`anthropic,` 等 CCR upstream provider 前缀写进 client 配置。
- Hermes Agent 使用 `custom_providers` 中名为 `ccr` 的 custom endpoint，`base_url` 指向本地 CCR `/v1`，`api_mode` 使用 OpenAI-compatible chat completions 的 `chat_completions`，不写入 `~/.hermes/.env`。
- activate 前需要 config preview 和 diff。
- 已激活配置需要重新应用动作。
- direct-to-provider 模式如果存在，需要单独定义 provider kind 到 client config 的写入规则。
- per-provider / per-profile 可以提供 Claude Code model mapping 模板，但模板不能自动变成 CCR upstream route。
- Codex client-visible model 字段与 CCR route model 也需要保持同样边界。

## 设计原则

- 默认 through-CCR 模式下，client 只连接本地 CCR。
- Provider API kind 只影响 CCR server 请求上游，不影响 Claude Code endpoint 注入。
- Claude Code model env 来自独立的 client-visible model mapping。
- CCR Route Pool model 不自动写入 Claude Code model env。
- 用户显式设置的 Claude Code model mapping 优先于模板或推断。
