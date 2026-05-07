# Phase 53 - Provider API Kind Choice

**状态：** ✅ 已完成（第一轮 API kind choice 完成）  
**优先级：** P0

## 目标

在 UI 里明确区分 provider 使用的 API 协议类型，而不是只靠 provider 名称、URL 或 transformer 猜测。

当前 Rust 配置里的 `Provider` 只有 `name`、`api_base_url`、`api_key`、`models`、`transformer` 等字段。UI 无法可靠判断一个 provider 是 OpenAI-compatible、Anthropic-native，还是其他聚合平台格式。这个缺口会影响：

- Provider 表单展示哪些字段
- Endpoint 测速请求应该发 OpenAI Chat/Responses 还是 Anthropic Messages
- 默认 transformer 应该如何推荐
- `tooluse`、`enhancetool`、`reasoning` 等 transformer 是否应该自动提示
- 未来 direct-to-provider 模式是否能绕过 CCR server 直接写入 client 配置

本 phase 要增加一个明确的 provider API kind choice，并把它接入 UI、配置、测速、默认配置和测试。这个 choice 支持推断，但必须区分“系统推断”和“用户显式选择”：用户一旦选过，就以用户选择为准，后续不能被 URL、name、transformer 的推断结果静默覆盖。

## 背景

### 当前 Rust 项目问题

`ccr-types::Provider` 当前结构：

```rust
pub struct Provider {
    pub name: String,
    pub api_base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub endpoint_candidates: Vec<String>,
    pub transformer: TransformerConfig,
}
```

这里没有协议类型。UI 只能展示通用字段，无法知道：

- `https://api.openai.com/v1/responses` 应该走 OpenAI Responses
- `https://api.openai.com/v1/chat/completions` 应该走 OpenAI Chat Completions
- `https://api.anthropic.com/v1/messages` 应该走 Anthropic Messages
- `https://api.deepseek.com/anthropic` 是 Claude Code 可直接注入的 Anthropic-compatible endpoint
- OpenRouter / DeepSeek / Groq / Vercel 等需要各自 transformer hint

### cc-switch 参考

`../cc-switch` 对不同 client/provider 使用不同配置形态：

- Claude provider preset 使用 `settingsConfig.env`：
  - `ANTHROPIC_BASE_URL`
  - `ANTHROPIC_AUTH_TOKEN` 或 `ANTHROPIC_API_KEY`
  - `ANTHROPIC_MODEL`
  - `ANTHROPIC_DEFAULT_HAIKU_MODEL`
  - `ANTHROPIC_DEFAULT_SONNET_MODEL`
  - `ANTHROPIC_DEFAULT_OPUS_MODEL`
- Claude preset 还保留 `apiFormat?: "anthropic" | "openai_chat"`。这只适用于 cc-switch 直接写 client provider 的场景；CCR 默认 through-CCR 模式不应让 Claude 直接理解 OpenAI/Anthropic 上游差异。
- Codex provider preset 使用 `{ auth, config }`：
  - `auth.OPENAI_API_KEY`
  - `config.toml`
  - `model_provider`
  - `model`
  - `wire_api = "responses"`
  - `requires_openai_auth = true`

也就是说，cc-switch 不是只存一个 URL，而是按 client/provider 的真实协议结构写入 live config。

### CCR 原版参考

原版 CCR 的 provider 配置仍然以 transformer 表达协议差异：

```json
{
  "name": "deepseek",
  "api_base_url": "https://api.deepseek.com/chat/completions",
  "api_key": "sk-xxx",
  "models": ["deepseek-chat", "deepseek-reasoner"],
  "transformer": {
    "use": ["deepseek"],
    "deepseek-chat": { "use": ["tooluse"] }
  }
}
```

这套设计需要保留兼容，但 UI-first 的 Rust 版不能要求用户手写 transformer 才能表达 provider 协议。UI 应提供明确 choice，然后由 core 生成或建议 transformer。

## Provider API Kind

新增一个显式枚举，建议放在 `ccr-types`：

```rust
pub enum ProviderApiKind {
    OpenAiChat,
    OpenAiResponses,
    AnthropicMessages,
    AnthropicCompatible,
    OpenRouter,
    DeepSeek,
    Groq,
    Vercel,
    Custom,
}
```

配置字段建议：

```json
{
  "name": "openai",
  "api_kind": "openai_responses",
  "api_kind_source": "explicit",
  "api_base_url": "https://api.openai.com/v1/responses",
  "api_key": "$OPENAI_API_KEY",
  "models": ["gpt-5.2"],
  "transformer": { "use": ["openai"] }
}
```

兼容规则：

- 缺少 `api_kind` 的旧配置必须继续可读。
- 旧配置读取时允许按 transformer / URL / provider name 推断一个 UI hint，并标记为 inferred。
- 保存时如果用户没有改过 choice，可以写入推断出的 `api_kind`，但 `api_kind_source` 必须是 `inferred`。
- 用户在 UI 中选过 choice 后，必须写入 `api_kind_source = "explicit"`。
- `explicit` 永远优先于推断；后续 URL、name、transformer 变化不能静默覆盖 explicit choice。
- 如果 explicit choice 和 URL/transformer 明显冲突，UI 只显示 warning，并提供“重新推断”按钮，不能自动改。
- 推断只能用于迁移、默认值和提示；长期业务判断必须读取 resolved kind，且 resolved kind 需要保留 source 信息。

建议新增：

```rust
pub enum ProviderApiKindSource {
    Explicit,
    Inferred,
}

pub struct ProviderApiKindChoice {
    pub kind: ProviderApiKind,
    pub source: ProviderApiKindSource,
}
```

如果实现上不想新增嵌套结构，也可以保留两个字段：

```rust
pub api_kind: Option<ProviderApiKind>,
pub api_kind_source: ProviderApiKindSource,
```

但 core 层必须提供 `resolve_provider_api_kind(provider) -> ProviderApiKindChoice`，避免 UI 自己散落推断逻辑。

## UI Choice

Provider 表单中新增一个必填 choice：

- OpenAI Chat Completions
- OpenAI Responses
- Anthropic Messages
- Anthropic-compatible for Claude Code
- OpenRouter
- DeepSeek
- Groq
- Vercel
- Custom

UI 行为：

- choice 切换时更新默认 endpoint placeholder。
- choice 切换时展示对应 API key 字段说明。
- choice 切换时给出推荐 transformer 列表，但允许高级用户覆盖。
- choice 切换时不静默删除用户已有 transformer。
- 如果当前配置是旧配置，UI 显示 inferred badge，并提示保存后会写入显式 kind。
- 如果当前 choice 是 inferred，UI 显示“推断自 URL/transformer/name”。
- 如果用户手动选择，UI 立即把 source 标记为 explicit。
- explicit choice 下如果用户修改 URL 导致推断结果变化，UI 显示 mismatch warning，不自动改 choice。
- 提供“重新推断”动作：用户点击后才用最新 URL/transformer/name 覆盖当前 choice，并把 source 设为 inferred，或让用户确认后设为 explicit。

## Transformer Mapping

需要建立一层可测试 mapping：

```rust
pub struct ProviderApiKindDefaults {
    pub default_endpoint: Option<&'static str>,
    pub recommended_transformers: Vec<&'static str>,
    pub endpoint_test_mode: EndpointTestMode,
    pub client_injection_mode: ClientInjectionMode,
}
```

`client_injection_mode` 只表示 direct-to-provider 高级模式的可行性。默认 through-CCR 模式不需要根据 provider kind 改写 Claude/Codex client 配置。

推荐初始规则：

- `OpenAiChat`：推荐 `openai`
- `OpenAiResponses`：推荐 `openai`；direct-to-provider 模式下 Codex 才可能使用 `wire_api = "responses"`
- `AnthropicMessages`：推荐 `anthropic`
- `AnthropicCompatible`：推荐 `anthropic`；direct-to-provider 模式下 Claude 才可能写 `ANTHROPIC_BASE_URL`
- `OpenRouter`：推荐 `openrouter`
- `DeepSeek`：推荐 `deepseek`，对 chat 模型可提示 `tooluse`
- `Groq`：推荐 `groq`
- `Vercel`：推荐 `vercel`
- `Custom`：不自动推荐，只保留用户手动选择

`tooluse` 不应对所有 provider 自动开启。它应该按 provider kind + model rule 提示：

- DeepSeek chat 类模型可推荐
- OpenAI/Anthropic native 不默认推荐
- Custom 只提示，不自动改

## Endpoint Test

`ccr-app-core::endpoint` 当前使用固定 Anthropic-ish request body 测速，这不够。

需要根据 `ProviderApiKind` 选择测速 payload：

- OpenAI Chat：`/chat/completions` 风格 body
- OpenAI Responses：Responses API 风格 body
- Anthropic Messages：`/v1/messages` 风格 body，并使用 `x-api-key` / `anthropic-version`
- Anthropic-compatible：同 Anthropic Messages，但允许 provider-specific endpoint
- Custom：提供最保守连通性测试，或要求用户选择 test mode

## Client Injection Boundary

默认 through-CCR 模式下，Claude/Codex 注入不需要知道 provider kind。client 只写本地 CCR server endpoint，所有上游 provider 差异由 CCR server 处理。

Provider kind 只在 direct-to-provider 高级模式下影响 client 注入：

- Claude direct-to-provider 只适用于 Anthropic/Anthropic-compatible provider。
- Codex direct-to-provider 只适用于 OpenAI Responses 兼容 provider。
- OpenAI Chat、OpenRouter、DeepSeek、Groq 等 provider 默认应通过 CCR server 连接，除非单独验证 client 原生支持。

UI 要明确展示默认 through-CCR 模式和可选 direct-to-provider 模式的区别，不能让 provider kind 泄漏成默认 client 注入要求。

## 完成项

- [x] 在 `ccr-types` 新增 `ProviderApiKind`
- [x] 在 `ccr-types` 新增 `ProviderApiKindSource`
- [x] `Provider` 增加 `api_kind` / `api_kind_source` 字段，旧配置兼容读取
- [x] 在 `ccr-app-core` 增加 provider kind defaults / inference / validation
- [x] 在 `ccr-app-core` 增加 resolved choice，明确区分 inferred 和 explicit
- [x] UI Provider 编辑页新增 API kind choice
- [x] UI 支持 inferred badge、explicit badge、mismatch warning 和重新推断动作
- [x] UI 根据 API kind 展示 endpoint/API key/model/transformer hints
- [x] Endpoint test 根据 API kind 生成不同请求
- [x] 默认配置和保存路径写入 `api_kind`
- [ ] UI 明确展示 through-CCR 默认模式；direct-to-provider 作为可选高级模式时再读取 API kind 判断支持状态
- [x] 文档说明 provider kind 与 transformer 的关系
- [x] 补充测试并保持 `cargo llvm-cov --workspace --lib --summary-only` line coverage >= 80%

## 规划的测试用例

- Config：旧 provider 缺少 `api_kind` 时可反序列化。
- Config：新 provider 保存时包含 `api_kind`。
- Config：用户手动选择后保存 `api_kind_source = "explicit"`。
- Config：推断值保存时 `api_kind_source = "inferred"`。
- Inference：`api_base_url` 包含 `/v1/responses` 推断为 `OpenAiResponses`。
- Inference：`api_base_url` 包含 `/chat/completions` 推断为 `OpenAiChat`。
- Inference：`api_base_url` 包含 `/v1/messages` 推断为 `AnthropicMessages`。
- Inference：`transformer.use = ["openrouter"]` 推断为 `OpenRouter`。
- Resolution：explicit choice 优先于 URL 推断。
- Resolution：explicit choice 优先于 transformer 推断。
- Resolution：URL/transformer 变化不会覆盖 explicit choice。
- Resolution：重新推断动作会更新 inferred choice。
- Validation：explicit choice 与推断结果冲突时返回 warning 而不是 error。
- Defaults：`DeepSeek` 推荐 `deepseek`，并对 chat model 提示 `tooluse`。
- Defaults：`AnthropicMessages` 不推荐 `tooluse`。
- Defaults：`OpenAiResponses` 在 direct-to-provider 模式下可提示 Codex responses 支持；through-CCR 模式仍只写本地 CCR endpoint。
- Endpoint：OpenAI Chat 生成 chat completions body。
- Endpoint：OpenAI Responses 生成 responses body。
- Endpoint：Anthropic Messages 生成 messages body 和 Anthropic headers。
- UI state：切换 choice 后 transformer hint 更新但不覆盖手写 transformer。
- UI state：旧配置显示 inferred 状态。
- UI state：用户选择 choice 后显示 explicit 状态。
- UI state：explicit 状态下修改 URL 显示 mismatch warning。
- Injection：through-CCR 模式下 Claude/Codex 注入不依赖 provider kind。
- Injection：direct-to-provider 模式下 Claude direct 只允许 Anthropic/Anthropic-compatible。
- Injection：direct-to-provider 模式下 Codex direct 只允许 OpenAI Responses 兼容 provider。
- Coverage：`cargo llvm-cov --workspace --lib --summary-only` line coverage >= 80%。

## 预留的开发偏差

- 本轮完成了 provider API kind 的类型、推断、显式选择、UI choice、保存迁移和 endpoint test payload 分流。
- `api_kind` 缺失的旧 provider 会在 UI 保存时写入推断结果，并标记 `api_kind_source = "inferred"`；用户在 UI choice 中改过后才会标记 `explicit`。
- `explicit` 选择不会被 URL/name/transformer 推断覆盖；冲突时只显示 warning，并提供 `Re-infer` 动作。
- Endpoint test 已按 API kind 生成 OpenAI Chat、OpenAI Responses、Anthropic Messages 三类 payload/header；真实 HTTP 成功路径仍未使用本地 mock server 覆盖，因为当前环境不适合监听本地端口。
- Claude/Codex 注入逻辑本轮没有新增 direct-to-provider 模式。默认 through-CCR 模式下不需要读取 provider kind；后续如果提供 direct-to-provider 高级模式，应使用 `ClientInjectionMode` 阻止错误的 direct 写入。
- `config_tab_new.rs` 不是当前 app 入口，但为了避免后续启用时落后，也同步接入了 inferred save 和 API-kind endpoint test request。
## 验证

```bash
cargo test --workspace
cargo build --package ccr-ui
cargo build --package ccr-server
cargo llvm-cov --workspace --lib --summary-only
```

验证结果：

- `cargo test --workspace` 通过
- `cargo build --package ccr-ui` 通过
- `cargo build --package ccr-server` 通过
- `cargo llvm-cov --workspace --lib --summary-only`：Lines `80.74%`

## 验收标准

- UI 可以明确选择 provider API kind
- 旧配置不崩溃，且能迁移到显式 `api_kind`
- Transformer 推荐来自 provider kind mapping，而不是散落在 UI 里
- Endpoint 测速不再对所有 provider 使用同一种请求格式
- Claude/Codex 注入 UI 能清楚区分默认 through-CCR 和可选 direct-to-provider 支持状态
- 保留原版 CCR transformer 配置能力，不破坏手写高级配置
- Phase 文档开发结束后补充 `预留的开发偏差`
