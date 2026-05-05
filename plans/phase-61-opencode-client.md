# Phase 61 - OpenCode Client Support

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

在当前 Claude Code / Codex client-native 注入能力之外，增加 OpenCode client 支持。

这个 phase 的核心是 UI 优先地把本地 CCR server 注入到 OpenCode 的原生配置里，而不是增加新的上游 provider 协议。默认模式仍然是 through-CCR：

- OpenCode 连接本地 CCR server。
- 上游 provider 类型、模型路由、transformer、provider pool、failover 都由 CCR server 处理。
- OpenCode 注入层只负责按 OpenCode 的原生配置结构写入一个可用 provider。

## 背景

当前 Rust 项目已经支持：

- Claude Code：
  - 写入 `~/.claude/settings.json`
  - 设置 `env.ANTHROPIC_BASE_URL = http://127.0.0.1:<port>`
  - 可选写入 Claude Code model mapping env
  - 备份、恢复、状态检测
- Codex：
  - 写入 `~/.codex/config.toml`
  - 写入 `~/.codex/auth.json`
  - 配置本地 `ccr` model provider 指向 `http://127.0.0.1:<port>/v1`
  - 备份、恢复、状态检测

这两类 client 都更接近“当前配置覆盖/恢复”模型。

`cc-switch` 对 OpenCode 的设计不同：OpenCode 是累加式 provider 管理，不是单一 current provider 覆盖。

## cc-switch OpenCode 参考

相关文件：

- `../cc-switch/src/config/opencodeProviderPresets.ts`
- `../cc-switch/src/types.ts`
- `../cc-switch/src-tauri/src/opencode_config.rs`
- `../cc-switch/src-tauri/src/provider.rs`
- `../cc-switch/src-tauri/src/services/provider/live.rs`
- `../cc-switch/src-tauri/src/services/provider/mod.rs`
- `../cc-switch/src-tauri/src/commands/provider.rs`

OpenCode 配置路径：

```text
~/.config/opencode/opencode.json
```

OpenCode provider 写入位置：

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "ccr": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "CCR",
      "options": {
        "baseURL": "http://127.0.0.1:<port>/v1",
        "apiKey": "any"
      },
      "models": {
        "<model>": {
          "name": "<model>"
        }
      }
    }
  }
}
```

cc-switch 行为要点：

- OpenCode 使用 `provider.<id>` map，多个 provider 可以共存。
- `add/update/switch` 都是写入或更新 live config 中的 provider fragment。
- `delete` 可以同时从数据库和 live config 删除。
- `removeFromLiveConfig` 只从 live config 移除，不删除数据库里的 provider。
- 可从 live config 反向导入 provider。
- OpenCode 支持 MCP 同步，但本 phase 只关注 provider/client 注入，不扩展 MCP。

## 当前 Rust 项目缺口

### 1. Client 类型不完整

Rust 当前只把 Claude 和 Codex 作为可注入 client：

- Status 页面只显示 Claude/Codex 注入状态。
- Settings 的 Clients 只配置 Claude/Codex 路径。
- `ccr-cli` 只有 `claude_config.rs` 和 `codex_config.rs`。
- `InjectionSnapshot` 只有通用状态，没有 client 枚举/列表建模。

需要增加 OpenCode 作为 UI 一等 client。

### 2. OpenCode 不能按覆盖式 backup/restore 处理

Claude/Codex 当前是：

1. 激活前备份整个原文件。
2. 写入 CCR 管理配置。
3. deactivate 恢复原文件。

OpenCode 更适合：

- 增量写入 `provider.ccr`。
- deactivate / remove 只移除 CCR 管理的 provider entry。
- 不删除用户其他 provider、mcp、plugin 等配置。
- 仍然可以做安全备份，但恢复不应粗暴覆盖用户激活后新增的其他 provider。

### 3. OpenCode 是累加式 provider，不存在“当前 provider”

Status 和 UI 文案不能叫 “Activate current provider” 或 “Switch provider”。

建议用：

- `Add CCR provider to OpenCode`
- `Remove CCR provider from OpenCode`

Status 应显示：

- Provider present / missing
- Config path
- CCR endpoint 是否匹配当前 port

### 4. 默认模型策略需要复用现有 client model mapping

OpenCode 注入时必须选择写入的 model。

默认策略：

- 优先使用 `Router.default`。
- 如果 route 是 `provider,model`，client provider 写入 model 部分。
- 如果 route 是 provider-only，写入保守默认 model：
  - 优先使用现有 Codex 默认模型配置
  - 否则使用 `gpt-5-codex`
- 不把 `provider,model` 原样写入 client 模型名，避免 client 误认为模型名包含 provider prefix。

后续可以把 `AppSettings` 扩展为 per-client 默认模型：

```json
{
  "AppSettings": {
    "opencode_config_path": "...",
    "opencode_model": "gpt-5-codex"
  }
}
```

本 phase 可以先从 `Router.default` 推导，不做完整高级模型映射页。

## 设计方向

### 1. Client 枚举和路径配置

新增 client 枚举值：

```rust
pub enum ClientKind {
    Claude,
    Codex,
    OpenCode,
}
```

扩展 `AppSettings`：

```rust
pub opencode_config_path: Option<String>,
```

默认路径：

```text
~/.config/opencode/opencode.json
```

路径配置在 Settings -> Clients 中显示，和 Claude/Codex 保持一致。

### 2. OpenCode 注入模块

新增：

```text
ccr-rust/crates/ccr-cli/src/opencode_config.rs
```

核心函数：

```rust
opencode_config_path()
opencode_injection_snapshot(port)
activate_opencode_ccr()
deactivate_opencode_ccr()
build_opencode_config(original_json, port, model)
opencode_points_to_ccr(path, port)
remove_opencode_ccr_provider(original_json)
```

写入规则：

- 解析 JSON；文件不存在时创建：

```json
{ "$schema": "https://opencode.ai/config.json" }
```

- 确保 `provider` 是 object。
- 写入或更新 `provider.ccr`。
- 保留其他 top-level 字段、其他 provider、mcp、plugin。
- `options.apiKey` 写 `"any"`。
- `options.baseURL` 写 `http://127.0.0.1:<port>/v1`。
- `npm` 默认 `@ai-sdk/openai-compatible`。
- `models` 写入推导出的 model。
- remove 只删除 `provider.ccr`；如果 `provider` 变空，可以保留空 object 或删除，需测试固定行为。

### 3. Status 页面

Status 页面新增 client section：

- OpenCode Config

显示：

- Config path
- Active / inactive / drifted
- CCR endpoint
- Add CCR Provider 按钮
- Remove CCR Provider 按钮

按钮启用规则：

- Add 需要 server running。
- Remove 不需要 server running。
- 如果已经 active and current，Add 禁用。
- 如果 inactive 或 drifted，Add 启用。
- 如果 provider present，Remove 启用。

Status 页面点开时仍要即时读取文件状态，和 Claude/Codex 当前 live status 行为一致。

### 4. Settings 页面

Settings -> Clients 增加：

- OpenCode config path

文案必须明确：

- 改路径只影响后续读取/写入。
- 不会立即移动文件。
- OpenCode 是增量 provider 写入，不会覆盖完整配置。

### 5. CLI 后置策略

项目方向是 UI 优先，CLI 不是本 phase 的主要边界。

本 phase 可以为 UI 调用暴露 Rust 函数；是否增加用户可见 CLI 命令后置：

- 不强制增加 `ccr opencode-activate`。
- 如果为了测试或临时调试增加 hidden/internal command，需要在文档中标记非主要入口。
- 用户文档以 UI 操作为主。

## 非目标

- 不实现 OpenCode MCP 同步。
- 不实现 OpenCode plugin / Oh My OpenCode 管理。
- 不实现 OpenCode 代理接管。
- 不新增 OpenCode 直连上游 provider 模式。
- 不把 OpenCode 加入 provider pool 的候选概念；provider pool 仍属于 CCR server 上游路由。
- 不实现 OpenCode endpoint speed test 专用逻辑。

## 预计修改文件

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-app-core/src/status.rs`
- `ccr-rust/crates/ccr-app-core/src/settings.rs`
- `ccr-rust/crates/ccr-cli/src/lib.rs`
- `ccr-rust/crates/ccr-cli/src/opencode_config.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- `ccr-rust/crates/ccr-ui/src/settings_tab.rs`
- `ccr-rust/crates/ccr-ui/src/tray_manager.rs`（如果 task icon 要显示新 client 状态）
- `ccr-rust/docs/client-model-mapping.md`
- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`

## 规划的测试用例

### OpenCode config

- 文件不存在时创建带 `$schema` 的配置。
- 已有 `provider` 不是 object 时返回明确错误，不静默覆盖。
- `build_opencode_config()` 保留未知 top-level 字段。
- `build_opencode_config()` 保留已有其他 provider。
- `build_opencode_config()` 保留已有 mcp/plugin 配置。
- `build_opencode_config()` 写入 `provider.ccr.npm = "@ai-sdk/openai-compatible"`。
- `build_opencode_config()` 写入 `provider.ccr.options.baseURL = http://127.0.0.1:<port>/v1`。
- `build_opencode_config()` 写入 `provider.ccr.options.apiKey = "any"`。
- `build_opencode_config()` 从 `Router.default = "openai,gpt-5-codex"` 推导 model `gpt-5-codex`。
- provider-only route 时使用稳定 fallback model。
- `opencode_points_to_ccr()` 能识别当前 port。
- `opencode_points_to_ccr()` 在 endpoint 端口不同或 provider 缺失时返回 false。
- remove 只删除 `provider.ccr`，不删除其他 provider。
- invalid JSON 返回错误，不 panic。

### Status/UI

- `StatusSnapshot` 增加 OpenCode 后仍能读取 Claude/Codex 状态。
- server stopped 时 OpenCode Add 按钮禁用。
- server running 且 inactive 时 Add 按钮启用。
- active and current 时 Add 按钮禁用、Remove 启用。
- drifted 时展示 drifted 状态，并允许重新 Add。
- Settings -> Clients 显示 OpenCode path。
- 修改 OpenCode path 不标记 server restart required。

### 安全和备份

- activate 前创建 timestamped backup 或 safety snapshot。
- 增量写入失败时不破坏原文件。
- 临时文件写入后原子 rename。
- Unix 下新写入文件权限不弱于当前 Claude/Codex 策略。
- deactivate 不覆盖激活后用户新增的其他 provider。

### 验证命令

- `cargo test --package ccr-cli --lib`
- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-server --bin ccr-ui`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- UI Status 页面能看到 Claude、Codex、OpenCode 三类 client 状态。
- OpenCode 可以通过 UI 添加 CCR provider 到 `~/.config/opencode/opencode.json`。
- OpenCode 可以通过 UI 移除 CCR provider，且不影响其他 provider/mcp/plugin。
- Status 页面点开时读取 OpenCode live 文件即时状态。
- Settings 页面能配置 OpenCode config path。
- 所有新增写入逻辑有单元测试覆盖。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 已实现 OpenCode additive provider 写入/移除/status，未采用 Claude/Codex 的整文件 backup/restore 语义。
- Status 页面使用 additive client 状态：missing / current / drifted；没有把 OpenCode 塞进 `InjectionSnapshot` 的 backup 文案。
- 本次没有实现 OpenCode MCP 同步、plugin/Oh My OpenCode 管理、direct-to-provider 模式或 endpoint test 专用逻辑。
- OpenCode 写入模型从 `Router.default` 推导；provider-only route 使用 `gpt-5-codex` fallback。
- Settings -> Clients 已加入 OpenCode config path；路径变化只影响后续读写，不移动文件。
