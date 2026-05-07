# Phase 54 - Through-CCR Pipeline Alignment

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

修正 through-CCR 模式下的 server 侧协议边界。

Claude Code 和 Codex 不需要知道上游 provider 类型。它们只需要连接本地 CCR server：

- Claude Code 写入 `http://127.0.0.1:<port>`
- Codex 写入 `http://127.0.0.1:<port>/v1`

上游 provider 是 Anthropic、OpenAI、OpenRouter、DeepSeek 还是其他兼容协议，应该由 CCR server 根据 provider config、router、transformer、provider API kind 统一处理。

当前 client 注入层基本符合这个边界，但 server 侧 pipeline 仍不完整。本 phase 要把 `/v1/messages`、`/v1/responses`、failover、upstream headers 和 status live 检测对齐到同一套 through-CCR 逻辑。

## 当前问题

### 1. `/v1/responses` 没有进入统一 transformer pipeline

Claude Code 入口 `/v1/messages` 会按 provider transformer 处理请求 body。

Codex 入口 `/v1/responses` 当前只把 route model 改成 upstream model，然后直接发给 provider。这样会导致：

- Codex 通过 CCR 使用 Anthropic/Sonnet 时，请求没有被转换成 Anthropic Messages 兼容格式
- Codex 通过 CCR 使用 DeepSeek/OpenRouter/Groq 等 provider 时，provider transformer 不一定生效
- `tooluse`、`reasoning`、`streamoptions` 等 transformer 不能可靠覆盖 Responses 请求

### 2. failover 没有按目标 provider 重新生成请求

当前 failover 尝试会复用 primary provider 已经准备好的 body，只替换 model。

这在同协议 provider 间可能勉强工作，但跨 provider kind 会错：

- OpenAI body failover 到 Anthropic endpoint
- Anthropic body failover 到 OpenAI-compatible endpoint
- DeepSeek/OpenRouter/Groq 的 transformer 没有按 failover 目标重新应用
- failover 目标 provider 的 header 格式没有参与决策

### 3. Upstream headers 固定为 OpenAI Bearer

Server 上游请求目前固定写：

```text
Authorization: Bearer <api_key>
Content-Type: application/json
```

但 Anthropic Messages 需要：

```text
x-api-key: <api_key>
anthropic-version: 2023-06-01
Content-Type: application/json
```

Endpoint test 已经按 provider kind 区分 header，但真实 server 请求还没有。

### 4. Status 注入状态不够 live

Status 页面当前主要通过 backup marker 判断 Claude/Codex 是否 activated。

这只能说明曾经执行过 activate，不能证明当前配置文件仍然指向 CCR。例如用户手动改了 `~/.claude/settings.json` 或 `~/.codex/config.toml`，UI 仍可能显示 activated。

本 phase 需要让 status 检测读取实际 client 配置文件，判断当前 endpoint 是否指向本地 CCR。

## 非目标

- 不修改 Claude/Codex 默认注入为 direct-to-provider
- 不要求 Claude/Codex 知道上游 provider kind
- 不追踪 CLI 参数兼容
- 不实现完整 custom transformer plugin
- 不实现完整 direct-to-provider 高级模式
- 不实现完整 usage dashboard / session / MCP / skills

## 设计方向

### 1. 引入统一 upstream request builder

新增可测试的 server/core helper，把“route + provider + inbound body + inbound protocol”转换为真实 upstream request。

建议结构：

```rust
pub enum InboundProtocol {
    AnthropicMessages,
    OpenAiResponses,
}

pub struct UpstreamRequest {
    pub route: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    pub stream: bool,
}
```

核心函数方向：

```rust
pub fn build_upstream_request(
    inbound: InboundProtocol,
    route: &str,
    provider: &Provider,
    body: serde_json::Value,
    transformers: &TransformerRegistry,
) -> Result<UpstreamRequest, String>
```

要求：

- route model 必须剥离 provider 前缀后写入 upstream body
- provider transformer 必须按目标 provider 应用
- upstream header 必须按 provider API kind 生成
- stream 判断必须基于最终 upstream body
- `/v1/messages` 和 `/v1/responses` 使用同一个 builder
- failover 每次 attempt 都必须重新调用 builder

如果 `ccr-server` 不适合暴露这些 helper，可以先放在 `ccr-server/src/lib.rs`，并用单元测试覆盖纯逻辑。

### 2. Responses 入站协议转换

`/v1/responses` 进入 CCR 后，server 需要先保留 Codex/OpenAI Responses 入站语义，再按目标 provider 生成 upstream body。

最低要求：

- OpenAI Responses provider：保留 Responses 风格 body，剥离 `provider,model`
- OpenAI Chat/OpenRouter/DeepSeek/Groq/Vercel provider：转换或规范化为 provider transformer 可处理的 OpenAI chat-ish body
- Anthropic Messages/Anthropic-compatible provider：转换为 Anthropic Messages-ish body，再应用 Anthropic transformer
- 不支持的转换必须明确返回 400/502，不能静默发错协议

可以先实现最小转换集：

- Responses `input: string` -> Messages `[{"role":"user","content": "..."}]`
- Responses `model` -> upstream model
- `stream` 原样保留
- `max_output_tokens` 映射到 `max_tokens`，如果目标是 Responses 则保留原字段

更复杂的 tool call、reasoning、multi-modal、response item mapping 可以在后续 phase 扩展，但本 phase 必须写清楚 unsupported 行为。

### 3. Provider kind header builder

新增 header builder：

```rust
pub fn upstream_headers(provider: &Provider) -> Vec<(String, String)>
```

规则：

- `AnthropicMessages` / `AnthropicCompatible`：`x-api-key` + `anthropic-version`
- `OpenAiChat` / `OpenAiResponses` / `OpenRouter` / `DeepSeek` / `Groq` / `Vercel`：`Authorization: Bearer`
- `Custom`：默认 `Authorization: Bearer`，后续再扩展 custom header

Header builder 必须和 endpoint test 使用同一套 provider kind resolution 规则，避免测速和真实请求不一致。

### 4. Failover 按目标 provider 重建 request

`send_with_failover` 不应接收已经转换完的 `body_json` 作为所有 attempt 的共享 body。

它应该接收：

- inbound protocol
- original inbound body
- primary route
- config
- transformer registry

每次 attempt：

1. 找到 attempt route 对应 provider
2. 用 attempt provider 重新 build upstream request
3. 发 attempt provider 的 url/header/body
4. 只在 network error、5xx、429 时继续下一个 attempt

这样 failover 才能跨 provider kind。

### 5. Live status 检测读取真实配置

Status 页面应该继续展示 backup 信息，但 activated 判断不能只看 backup marker。

Claude live 检测：

- 读取 `~/.claude/settings.json`
- 判断 `env.ANTHROPIC_BASE_URL == http://127.0.0.1:<port>`
- 可选检查 `ANTHROPIC_AUTH_TOKEN` 是否存在

Codex live 检测：

- 读取 `~/.codex/config.toml`
- 判断 `model_provider == "ccr"`
- 判断 `[model_providers.ccr].base_url == http://127.0.0.1:<port>/v1`
- 判断 `wire_api == "responses"`
- 可选检查 `~/.codex/auth.json` 是否存在 `OPENAI_API_KEY`

UI 状态建议区分：

- `ActiveAndCurrent`：当前配置指向 CCR，且 backup 存在
- `ActiveButDrifted`：backup 存在，但当前配置已经不指向 CCR
- `InjectedNoBackup`：当前配置指向 CCR，但没有 backup
- `Inactive`：当前配置不指向 CCR，也没有 backup

如果短期不想扩展 UI enum，至少要让 `is_activated()` 基于真实配置，而不是只基于 backup marker。

## 预计修改文件

- `ccr-rust/crates/ccr-server/src/lib.rs`
- `ccr-rust/crates/ccr-server/src/main.rs`
- `ccr-rust/crates/ccr-app-core/src/provider_kind.rs`
- `ccr-rust/crates/ccr-app-core/src/status.rs`
- `ccr-rust/crates/ccr-cli/src/claude_config.rs`
- `ccr-rust/crates/ccr-cli/src/codex_config.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- 可能新增 `ccr-rust/crates/ccr-server/src/upstream.rs`

## 完成项

- [x] 明确 through-CCR 模式下 Claude/Codex client endpoint 边界
- [x] 新增 upstream request builder
- [x] `/v1/messages` 使用 upstream request builder
- [x] `/v1/responses` 使用 upstream request builder
- [x] `/v1/responses` 按目标 provider kind 做最小协议转换
- [x] Upstream headers 按 provider kind 生成
- [x] Failover 每次 attempt 按目标 provider 重新生成 body/header
- [x] Failover 跨 provider kind 有单元测试
- [x] Status 读取 Claude/Codex 真实配置判断 live 注入状态
- [x] UI Status 展示 drift / no backup / inactive 等差异状态
- [x] 文档更新当前 server pipeline 已对齐 through-CCR 边界
- [x] 测试覆盖率保持 line coverage >= 80%

## 规划的测试用例

- Claude injection：生成 `ANTHROPIC_BASE_URL = http://127.0.0.1:<port>`，不包含 `/v1/messages`。
- Codex injection：生成 `base_url = http://127.0.0.1:<port>/v1`，`wire_api = "responses"`。
- Server route：Actix app 注册 `/v1/messages` 和 `/v1/responses`。
- Upstream builder：Messages inbound + Anthropic provider 生成 Anthropic headers。
- Upstream builder：Messages inbound + OpenAI Chat provider 生成 Bearer header，并应用 `openai` transformer。
- Upstream builder：Responses inbound + OpenAI Responses provider 保留 Responses body。
- Upstream builder：Responses inbound + OpenAI Chat provider 将 `input` 转为 messages。
- Upstream builder：Responses inbound + Anthropic provider 将 `input` 转为 Anthropic messages。
- Upstream builder：`provider,model` route 会剥离 provider 前缀。
- Upstream builder：unsupported Responses body 返回明确错误。
- Header builder：Anthropic kind 使用 `x-api-key` 和 `anthropic-version`。
- Header builder：OpenAI/OpenRouter/DeepSeek/Groq/Vercel 使用 Bearer。
- Header builder：Custom 默认使用 Bearer。
- Failover：primary OpenAI Chat failover 到 Anthropic 时重新生成 Anthropic body/header。
- Failover：primary Anthropic failover 到 OpenAI Chat 时重新生成 OpenAI body/header。
- Failover：429 触发下一个 provider。
- Failover：400 不触发 failover。
- Failover：network error 触发下一个 provider。
- Failover：每个 attempt 使用 attempt route 的 model。
- Status Claude：backup 存在且 settings 指向 CCR -> ActiveAndCurrent。
- Status Claude：backup 存在但 settings 不指向 CCR -> ActiveButDrifted。
- Status Claude：settings 指向 CCR 但 backup 不存在 -> InjectedNoBackup。
- Status Codex：config.toml 指向 CCR `/v1` 且 wire_api responses -> ActiveAndCurrent。
- Status Codex：base_url 是旧 port -> ActiveButDrifted。
- Status Codex：auth.json 缺失时显示 warning，但不误判 endpoint。
- UI Status：刷新时重新读取 server 和注入状态。
- Coverage：`cargo llvm-cov --workspace --lib --summary-only` line coverage >= 80%。

## 预留的开发偏差

- 本轮新增的是最小可用 upstream request builder，放在 `ccr-server/src/lib.rs`，没有额外拆成 `src/upstream.rs`。原因是当前 server 纯逻辑规模仍可控，先避免新增模块边界。
- Responses 入站转换覆盖了 `input: string`、`input: array` 和已有 `messages` 的情况；tool call、reasoning、multi-modal response item 的完整双向映射仍留给后续 phase。
- Failover 跨 provider kind 的验证通过 upstream builder 单元测试覆盖 body/header 重建逻辑，没有启动本地 mock HTTP server 做真实 retry 链路测试。
- Status live 检测逻辑放在 `ccr-cli` 的 Claude/Codex config helper 中，并由 UI 调用。它已经读取真实 client 配置文件，但还没有完全迁移到 `ccr-app-core` service。
- Codex status 对 `auth.json` 缺失只保留为后续 warning 项；当前 live endpoint 判断以 `config.toml` 中 `model_provider`、`base_url`、`wire_api` 为准。

## 验证

```bash
cargo test --package ccr-server
cargo test --package ccr-app-core
cargo test --package ccr-cli
cargo test --package ccr-ui
cargo test --workspace
cargo build --package ccr-server
cargo build --package ccr-ui
cargo llvm-cov --workspace --lib --summary-only
```

验证结果：

- `cargo test --package ccr-server` 通过
- `cargo test --package ccr-cli` 通过
- `cargo test --package ccr-app-core` 通过
- `cargo test --package ccr-ui` 通过
- `cargo test --workspace` 通过
- `cargo build --package ccr-server` 通过
- `cargo build --package ccr-ui` 通过
- `cargo llvm-cov --workspace --lib --summary-only`：Lines `80.97%`

## 验收标准

- Claude/Codex 默认 through-CCR 注入不读取上游 provider kind
- Claude/Codex 本地 endpoint 区分正确
- `/v1/messages` 和 `/v1/responses` 共用统一 upstream request 构建逻辑
- Provider kind 影响 server upstream body/header/transformer/test，而不是默认 client 注入
- Responses 入站请求可以按目标 provider kind 生成正确 upstream 请求
- Failover 跨 provider kind 时不会复用错误协议 body/header
- Status 页面打开和刷新时读取真实配置状态
- Drift / no backup / inactive 状态可区分
- 测试覆盖率保持 80% 以上
