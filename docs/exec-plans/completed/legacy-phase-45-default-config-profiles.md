# Phase 45 — Default Config Profiles

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

把“默认配置”作为一等配置能力加入 CCR。

CCR 不应该只在用户手动填写完整 `Providers` 后才能工作。项目需要提供内置的官方 provider 配置模板，让用户可以快速选择并生成可用配置，包括：

- Claude Code 使用 OpenAI / GPT / Codex 官方模型
- Codex 使用 Anthropic / Claude Sonnet 官方模型
- Anthropic 官方配置
- OpenAI 官方 Responses 配置
- 可继续扩展 DeepSeek、OpenRouter 等非 Gemini 默认配置

## 背景

当前安装脚本会创建非常空的默认 config：

```json
{
  "Providers": [],
  "Router": {
    "default": "openai,gpt-4o"
  }
}
```

这个默认配置本身不可用，因为没有 provider、endpoint、api key placeholder、模型列表或 transformer 约定。

默认配置应该被视为一种可安装、可切换、可展示的配置来源，而不是写死在安装脚本里的空 JSON。

## 完成项

- [x] 新增内置 default config profiles
- [x] 支持 Anthropic 官方 provider profile
- [x] 支持 OpenAI 官方 Responses provider profile
- [x] 支持 Claude Code 使用 GPT / Codex 模型的 profile
- [x] 支持 Codex 使用 Claude Sonnet 模型的 profile
- [x] 默认 profiles 使用 API key placeholder，不写入真实密钥
- [x] core 支持列出内置 profiles
- [x] core 支持安装内置 profile 到 `config.json`
- [x] UI 支持选择并应用内置 profile
- [x] 安装脚本改为写入可用的 default profile 或提示选择 profile
- [x] provider / router / transformer 组合有测试覆盖

## Profile 类型

### 1. Anthropic Official

用于直连 Anthropic Messages API。

示例目标：

```json
{
  "Providers": [
    {
      "name": "anthropic",
      "api_kind": "anthropic_messages",
      "api_kind_source": "explicit",
      "api_base_url": "https://api.anthropic.com/v1/messages",
      "api_key": "$ANTHROPIC_API_KEY",
      "models": [
        "claude-sonnet-4",
        "claude-opus-4"
      ],
      "transformer": {
        "use": ["anthropic"]
      }
    }
  ],
  "Router": {
    "default": "anthropic,claude-sonnet-4"
  }
}
```

### 2. OpenAI / Codex Official

用于直连 OpenAI Responses API，服务 Codex 或 Claude Code 走 GPT/Codex 模型。

示例目标：

```json
{
  "Providers": [
    {
      "name": "openai",
      "api_kind": "openai_responses",
      "api_kind_source": "explicit",
      "api_base_url": "https://api.openai.com/v1/responses",
      "api_key": "$OPENAI_API_KEY",
      "models": [
        "gpt-5-codex",
        "gpt-5",
        "gpt-4.1"
      ],
      "transformer": {
        "use": ["openai"]
      }
    }
  ],
  "Router": {
    "default": "openai,gpt-5-codex"
  }
}
```

### 3. Claude Code Using GPT / Codex

目标：Claude Code 的请求进入 CCR 后，默认路由到 OpenAI 官方模型。

要求：

- `ccr claude-activate` 后 Claude Code 指向 CCR
- CCR default route 指向 `openai,gpt-5-codex` 或其他 GPT 模型
- 上游使用 OpenAI official provider
- 必须明确 transformer / backend wire format

### 4. Codex Using Claude Sonnet

目标：Codex 的请求进入 CCR 后，默认路由到 Anthropic Claude Sonnet。

要求：

- `ccr codex-activate` 后 Codex 指向 CCR `/v1/responses`
- CCR default route 可设置为 `anthropic,claude-sonnet-4`
- CCR 需要支持 Codex Responses request 到 Anthropic Messages request 的转换
- 如果转换器尚未实现，该 profile 必须标记为 `requires_transformer = true` 或 `experimental`

## 能力入口设计

Rust 版是 UI 优先产品。原版 CCR 或 cc-switch 的 CLI 参数名不作为本 phase 的兼容边界；这里关心的是 profile list/apply/install 能力本身。最终应优先提供 UI 入口和 core API，CLI 只作为可选调试入口。

可选调试入口示例：

```bash
ccr config profiles
ccr config apply-profile anthropic-official
ccr config apply-profile openai-codex-official
ccr config apply-profile claude-code-with-gpt
ccr config apply-profile codex-with-sonnet
```

也可以复用 preset 系统：

```bash
ccr preset install builtin:anthropic-official
ccr preset install builtin:openai-codex-official
ccr preset install builtin:claude-code-with-gpt
ccr preset install builtin:codex-with-sonnet
```

最终实现时二选一，优先复用 preset 系统，避免新增重复概念。

## UI 设计

Config 或 Presets 页面增加内置配置入口：

```text
Default Profiles

[Anthropic Official]
[OpenAI / Codex Official]
[Claude Code with GPT]
[Codex with Sonnet]
```

点击 profile 后：

- 展示将要写入的 providers 和 router
- 展示需要的 env vars，例如 `$ANTHROPIC_API_KEY` / `$OPENAI_API_KEY`
- 允许 merge / overwrite
- 写入前创建 config backup

## 数据结构方向

默认配置可以作为内置 preset manifest 存在：

```rust
pub struct BuiltinProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub providers: Vec<Provider>,
    pub router: RouterConfig,
    pub required_env: Vec<&'static str>,
    pub experimental: bool,
}
```

或直接扩展 `ccr-preset`：

```text
builtin:anthropic-official
builtin:openai-codex-official
builtin:claude-code-with-gpt
builtin:codex-with-sonnet
```

## 关键约束

- 默认配置不能包含真实 API key
- 默认配置必须写入明确的 `api_kind`，官方 profile 使用 `api_kind_source = "explicit"`
- 默认配置必须可被导出、查看、安装、覆盖
- 默认配置写入前必须备份现有 config
- 不应再创建不可用的空 `Providers: []` 默认配置
- Claude Code 和 Codex 的注入逻辑不应该各自硬编码模型，它们应读取当前 config/profile 的 Router 默认值
- `codex-with-sonnet` 依赖 Responses 到 Anthropic Messages 转换能力；如果转换器未完成，必须明确标记为 experimental

## 修改文件

预计涉及：

- `ccr-rust/crates/ccr-preset/src/lib.rs`
- `ccr-rust/crates/ccr-config/src/lib.rs`
- `ccr-rust/crates/ccr-cli/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/preset_tab.rs`
- `ccr-rust/packaging/macos/scripts/postinstall`
- `ccr-rust/packaging/windows/Product.wxs`

## 验证

```bash
cargo test --package ccr-preset
cargo test --package ccr-config
cargo test --package ccr-cli
cargo build --package ccr-ui
```

## 验收标准

- 新安装后不再生成不可用的空 provider config
- 用户可以列出内置 default profiles
- 用户可以安装 Anthropic 官方 profile
- 用户可以安装 OpenAI / Codex 官方 profile
- 用户可以配置 Claude Code 默认走 GPT / Codex 模型
- 用户可以配置 Codex 默认走 Claude Sonnet
- profile 中的 API key 使用 env placeholder
- 应用 profile 前会创建配置备份
- UI 能展示并应用内置 profile
- 所有内置 profile 有单元测试覆盖

## 预留偏差

- 本 phase 复用 `ccr-preset`，没有新增单独的 config profile crate；内置 profile 通过 `builtin_profiles()` / `builtin_profile()` 暴露。
- UI 入口放在 Presets 页面顶部的 `Default Profiles` 区域，支持查看 provider、Primary Route、required env、experimental 标记，并支持 merge / overwrite 同名 provider。
- CLI 入口暂未新增；项目方向是 UI 优先，CLI 仍作为后置调试入口。
- 安装脚本默认写入 OpenAI / Codex Official profile 的占位配置，用户仍需提供 `$OPENAI_API_KEY` 环境变量。
- `codex-with-sonnet` 已标记 `experimental`，因为复杂 Responses 到 Anthropic Messages 的 tool/reasoning/multimodal 映射仍需继续扩展。
