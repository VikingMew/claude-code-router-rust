# Phase 46 — Client-Native Provider Injection

**状态：** ✅ 已完成  
**优先级：** P1

## 后续对齐说明

Phase 46 已完成 Claude/Codex 原生配置写入方向。默认 through-CCR 模式下，Claude/Codex 只需要写入本地 CCR server endpoint；上游 provider 类型由 CCR server、router 和 transformer 处理，不能要求 client 注入层理解 Anthropic/OpenAI/OpenRouter 等 provider kind。

Phase 53 加入的 `ProviderApiKind` 应主要服务于上游请求、endpoint test、transformer 推荐和配置校验。只有未来新增 direct-to-provider 高级模式时，才需要根据 provider kind 判断是否可以直接写入 client 原生配置。

## 目标

重构 CCR 对 Claude Code / Codex 的配置注入方式。

当前实现更像“把 client 指到 CCR endpoint”，但对不同 client 的原生配置结构理解不够。参考 `../cc-switch` 后，provider 注入应该按 client 自己的 live config 格式写入，而不是对所有 client 用同一种 endpoint/key 思路。

## cc-switch 参考差异

### Claude Code

`cc-switch` 主要写 Claude Code 的 live settings：

```text
~/.claude/settings.json
```

写入结构是 Claude Code 原生支持的 env 配置：

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:<port>",
    "ANTHROPIC_AUTH_TOKEN": "any",
    "ANTHROPIC_MODEL": "<model>",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "<haiku_model>",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "<sonnet_model>",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "<opus_model>"
  }
}
```

同时它还有一个很轻量的 Claude plugin config 写法：

```text
~/.claude/config.json
```

只增量设置：

```json
{
  "primaryApiKey": "any"
}
```

它不会把 Claude Code 的主配置写成：

```json
{
  "api": {
    "endpoint": "...",
    "key": "..."
  }
}
```

### Codex

`cc-switch` 对 Codex 写两个 live 文件：

```text
~/.codex/auth.json
~/.codex/config.toml
```

`auth.json` 写 OpenAI 风格 key：

```json
{
  "OPENAI_API_KEY": "<api_key>"
}
```

`config.toml` 写 Codex 原生 provider：

```toml
model_provider = "newapi"
model = "<model>"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.newapi]
name = "NewAPI"
base_url = "http://127.0.0.1:<port>/v1"
wire_api = "responses"
requires_openai_auth = true
```

这和当前 CCR 的 Codex 注入不同：我们只写 `config.toml`，并且把 `requires_openai_auth` 设为 `false`，没有写 `auth.json`。

## 当前 CCR 问题

### 1. Claude 注入目标文件可能不对

当前 CCR 写：

```text
~/.claude/config.json
```

并写入：

```json
{
  "api": {
    "endpoint": "http://127.0.0.1:<port>/v1/messages",
    "key": "<api_key>"
  }
}
```

需要验证 Claude Code 当前版本是否仍支持该结构。参考 `cc-switch`，更稳妥的方式是写：

```text
~/.claude/settings.json
```

并使用 `env.ANTHROPIC_BASE_URL` / `env.ANTHROPIC_AUTH_TOKEN` / model env keys。

### 2. Claude 注入不应该覆盖整个 settings

应增量 merge：

- 保留已有 `permissions`
- 保留已有 `mcpServers`
- 保留用户其他 settings
- 只更新 CCR 管理的 env keys

### 3. Codex 注入缺少 auth.json 策略

Codex 原生 provider 通常依赖：

```text
~/.codex/auth.json
```

即使 CCR 本地不需要真实 OpenAI key，也应明确选择一种策略：

- 写 `OPENAI_API_KEY = "any"`，并 `requires_openai_auth = true`
- 或不写 auth，并确认 `requires_openai_auth = false` 在目标 Codex 版本中可靠

该策略必须有测试和文档说明。

### 4. Client 注入应来自 CCR 本地服务配置，而不是硬编码

默认 through-CCR 模式下，Claude/Codex 注入不应解析上游 provider 类型。它只需要从当前 CCR config/default profile 解析本地 server 和默认路由信息：

- 本地 CCR host/port
- client 类型
- 当前默认 route 对应的展示 model
- Codex 是否需要 `auth.json`
- Codex 本地 provider 是否使用 `wire_api = "responses"` 指向 CCR `/v1`
- Claude 是否需要写 `ANTHROPIC_MODEL` 等 model env keys

Anthropic/OpenAI/OpenRouter/DeepSeek 等上游 provider 类型由 CCR server 处理，不属于默认 client 注入层。

如果未来新增 direct-to-provider 高级模式，才需要额外解析：

- 当前上游 provider
- 当前上游 model
- client 类型
- 上游 wire API
- 是否允许直接写入该 client 的原生 provider

## 新设计

### ClientInjectionTarget

引入 client 原生注入目标：

```rust
enum ClientInjectionTarget {
    ClaudeCode,
    Codex,
}
```

### ClientInjectionProfile

把“如何写 client live config”作为 profile 的一部分：

```rust
struct ClientInjectionProfile {
    client: ClientInjectionTarget,
    route: String,
    base_url: String,
    auth_token: String,
    model: String,
    wire_api: String,
    write_auth_file: bool,
    requires_openai_auth: bool,
}
```

在默认 through-CCR 模式下，`base_url` 应指向本地 CCR server，而不是上游 provider endpoint。

### Claude 写入规则

写入：

```text
~/.claude/settings.json
```

merge：

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:<port>",
    "ANTHROPIC_AUTH_TOKEN": "any",
    "ANTHROPIC_MODEL": "<model>",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "<model>",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "<model>",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "<model>"
  }
}
```

可选写入：

```text
~/.claude/config.json
```

仅增量设置：

```json
{
  "primaryApiKey": "any"
}
```

### Codex 写入规则

写入：

```text
~/.codex/config.toml
~/.codex/auth.json
```

config:

```toml
model_provider = "ccr"
model = "<model>"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.ccr]
name = "CCR"
base_url = "http://127.0.0.1:<port>/v1"
wire_api = "responses"
requires_openai_auth = true
```

auth:

```json
{
  "OPENAI_API_KEY": "any"
}
```

如果最终决定继续 `requires_openai_auth = false`，必须用测试或实际 Codex 版本验证，并在文档里解释为什么不写 auth。

## 与 Phase 45 的关系

Phase 45 定义 default config profiles。

Phase 46 定义这些 profile 如何注入到具体 client。

也就是说：

- Phase 45：CCR 内部 provider/router 默认配置
- Phase 46：Claude Code / Codex 原生 live config 写入方式

二者不能混在一起，否则会继续出现“CCR config 是对的，但 client 没有按预期使用”的问题。

## 完成项

- [x] 对比并记录 Claude Code 支持的真实配置文件和字段
- [x] 对比并记录 Codex 支持的真实配置文件和字段
- [x] 新增 Claude settings.json merge 写入
- [x] 保留并增量写 Claude config.json `primaryApiKey`
- [x] 移除或废弃当前 Claude `api.endpoint` / `api.key` 注入方式
- [x] 新增 Codex auth.json 写入策略
- [x] Codex config.toml 写入 `model` / `model_reasoning_effort` / `disable_response_storage`
- [x] 注入逻辑从当前 CCR 本地 server/profile 生成，不硬编码模型
- [x] activate/deactivate 支持恢复多个 live 文件
- [x] Status 页面显示每个 client 的 live config 是否已注入
- [x] 单元测试覆盖 merge、backup、restore、missing-file 场景

## 修改文件

预计涉及：

- `ccr-rust/crates/ccr-cli/src/claude_config.rs`
- `ccr-rust/crates/ccr-cli/src/codex_config.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- `ccr-rust/crates/ccr-ui/src/tray_manager.rs`
- `ccr-rust/plans/phase-45-default-config-profiles.md`

## 验证

```bash
cargo test --package ccr-cli
cargo test --package ccr-ui
cargo build --package ccr-ui
```

## 完成说明

- Claude 注入已改为写 `~/.claude/settings.json`，并只 merge `env` 字段。
- Claude 注入不再写旧的 `api.endpoint` / `api.key` 结构。
- Claude settings 不存在时会创建 missing marker，deactivate 时删除生成的 settings。
- Claude plugin config 会增量写入 `~/.claude/config.json` 的 `primaryApiKey = "any"`。
- Codex 注入已改为同时写 `~/.codex/config.toml` 和 `~/.codex/auth.json`。
- Codex `auth.json` 写入 `OPENAI_API_KEY = "any"`。
- Codex `config.toml` 写入 `model`、`model_reasoning_effort = "high"`、`disable_response_storage = true`、`wire_api = "responses"`、`requires_openai_auth = true`。
- Codex deactivate 会分别恢复/删除 config 和 auth 文件。
- `primaryApiKey` 增量写入已启用，和 `cc-switch` 的 Claude plugin config 行为一致。

需要额外手工验证：

- Claude Code 注入后实际读取 `~/.claude/settings.json`
- Codex 注入后实际读取 `~/.codex/config.toml` 和 `~/.codex/auth.json`
- Claude Code 能通过 CCR 使用 GPT/Codex profile
- Codex 能通过 CCR 使用 Claude Sonnet profile

## 验收标准

- Claude 注入不再覆盖用户完整配置
- Claude 注入使用 Claude Code 原生 settings/env 结构
- Codex 注入明确写入或明确不需要 auth.json
- activate/deactivate 能完整恢复所有 touched files
- provider/profile 切换后重新 inject 能写入正确模型
- 行为和 `cc-switch` 的 client-native 注入模型保持一致
