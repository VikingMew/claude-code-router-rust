# Phase 55 - Claude Code Model Mapping

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

修正 Claude Code activate 写入模型环境变量的边界。

Claude Code 通过 CCR 使用任意上游 provider 时，Claude Code 只应该连接本地 CCR server，并使用 Claude Code 自己理解的模型名或模型类别提示。真实上游模型，例如 `glm-4.6`、`gpt-5-codex`、`claude-sonnet-*`，应该由 CCR Router 和 provider config 决定。

本 phase 要做到：

- 默认 Claude Code model env 不再从 CCR route model 推导。
- 新增可配置的 Claude Code 模型映射：
  - `ANTHROPIC_MODEL`
  - `ANTHROPIC_DEFAULT_HAIKU_MODEL`
  - `ANTHROPIC_DEFAULT_SONNET_MODEL`
  - `ANTHROPIC_DEFAULT_OPUS_MODEL`
- 在 UI Advanced 设置里可以编辑这些映射。
- activate Claude Code 时按该映射写入 `~/.claude/settings.json`。
- 文档明确区分 client-visible model mapping 和 CCR upstream routing model。

## 背景

当前实现曾把 CCR default route 的 model 写入 Claude Code env，例如：

```json
{
  "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-4.6",
  "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-4.6",
  "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-4.6",
  "ANTHROPIC_MODEL": "glm-4.6"
}
```

这会让 Claude Code 侧直接看到上游模型名。问题是：

- Claude Code 的模型 env 是 client 侧模型提示，不等于 CCR 上游路由。
- 上游 provider 可能对这些模型名做针对性兼容、降级或拒绝。
- CCR 已经有 Router、Transformer、Provider API kind 和 failover，应该由 server 侧决定真实上游请求。

`cc-switch` 支持按 Claude Code provider 配置这些 env 字段；原版 CCR 默认 `createEnvVariables` 不主动写入 route model env，而是主要写 `ANTHROPIC_BASE_URL`、token、proxy 和 telemetry 相关 env。

## 设计

### 1. 配置结构

在 `AppSettings` 增加：

```rust
pub struct ClaudeCodeModelSettings {
    pub model: String,
    pub haiku_model: String,
    pub sonnet_model: String,
    pub opus_model: String,
}
```

默认值使用 Claude Code 可理解的 Claude 系列模型名，而不是当前 CCR route 的 upstream model。

### 2. Claude Code 注入

`activate_ccr` 仍然写入：

- `ANTHROPIC_BASE_URL=http://127.0.0.1:<port>`
- `ANTHROPIC_AUTH_TOKEN=any`

同时写入 `AppSettings.claude_code_models` 中的四个模型 env。

这些 env 只描述 Claude Code 侧模型映射。CCR server 收到请求后，仍通过 Router/Provider/Transformer 选择真实上游 provider 和 model。

### 3. UI Advanced 配置

Settings 的 Advanced 页面增加 Claude Code model mapping：

- Anthropic model
- Haiku model
- Sonnet model
- Opus model

保存后写入主配置。该设置影响下一次 Claude Code activate 或重新 activate，不直接改正在运行中的 Claude Code 配置文件。

### 4. 非目标

- 不新增 direct-to-provider 模式。
- 不让 Claude Code 根据 provider kind 直接请求上游 provider。
- 不改变 CCR Router 的 route model 语义。
- 不实现 cc-switch universal provider。
- 不追踪原版 CCR CLI 参数形态。

## 预计修改文件

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-cli/src/claude_config.rs`
- `ccr-rust/crates/ccr-app-core/src/settings.rs`
- `ccr-rust/crates/ccr-ui/src/settings_tab.rs`
- `ccr-rust/docs/client-model-mapping.md`
- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`
- `ccr-rust/docs/ccr-original-feature-gap-analysis.md`

## 规划的测试用例

- ✅ `AppSettings` 缺省反序列化时生成默认 Claude Code model mapping。
- ✅ Claude Code settings builder 保留已有 env，并写入四个配置化模型 env。
- ✅ Claude Code settings builder 不再从 route model 推导 `ANTHROPIC_MODEL`。
- ✅ 非 object `env` 被替换为 object 后仍写入模型映射。
- ✅ Settings state 修改 Claude Code model mapping 不标记 server restart required。
- ✅ Settings UI 的 Advanced 页面能加载带默认 model mapping 的 state。
- ✅ 全工作区测试通过。
- ✅ `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 预留偏差

- 没有实现 activate 后对已写入 Claude Code settings 的热更新；当前配置影响下一次 Claude Code activate。
- 没有新增 direct-to-provider provider kind 写入逻辑；默认仍保持 through-CCR。

## 验证结果

- `cargo test --package ccr-types --package ccr-cli --package ccr-app-core --package ccr-ui`
- `cargo test --workspace`
- `cargo build --package ccr-ui`
- `cargo llvm-cov --workspace --lib --summary-only`

最终 line coverage：81.28%。
