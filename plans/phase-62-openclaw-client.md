# Phase 62 - OpenClaw Client Support

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

在当前 Claude Code / Codex client-native 注入能力之外，增加 OpenClaw client 支持。

这个 phase 的核心是 UI 优先地把本地 CCR server 注入到 OpenClaw 的原生配置里，而不是增加新的上游 provider 协议。默认模式仍然是 through-CCR：

- OpenClaw 连接本地 CCR server。
- 上游 provider 类型、模型路由、transformer、provider pool、failover 都由 CCR server 处理。
- OpenClaw 注入层只负责按 OpenClaw 的原生配置结构写入可用 provider，并在安全条件下补齐默认模型入口。

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

`cc-switch` 对 OpenClaw 的设计不同：OpenClaw 是累加式 provider 管理，并且有独立的 `agents.defaults` 默认模型配置。

## cc-switch OpenClaw 参考

相关文件：

- `../cc-switch/src/config/openclawProviderPresets.ts`
- `../cc-switch/src/types.ts`
- `../cc-switch/src/lib/api/openclaw.ts`
- `../cc-switch/src/hooks/useOpenClaw.ts`
- `../cc-switch/src/hooks/useProviderActions.ts`
- `../cc-switch/src-tauri/src/openclaw_config.rs`
- `../cc-switch/src-tauri/src/services/provider/live.rs`
- `../cc-switch/src-tauri/src/services/provider/mod.rs`
- `../cc-switch/src-tauri/src/commands/provider.rs`

OpenClaw 配置路径：

```text
~/.openclaw/openclaw.json
```

OpenClaw provider 写入位置：

```json
{
  "models": {
    "mode": "merge",
    "providers": {
      "ccr": {
        "baseUrl": "http://127.0.0.1:<port>/v1",
        "apiKey": "any",
        "api": "openai-responses",
        "models": [
          {
            "id": "<model>",
            "name": "<model>"
          }
        ]
      }
    }
  }
}
```

OpenClaw 还支持：

```json
{
  "agents": {
    "defaults": {
      "model": {
        "primary": "ccr/<model>",
        "fallbacks": []
      },
      "models": {
        "ccr/<model>": {
          "alias": "CCR"
        }
      }
    }
  },
  "env": {},
  "tools": {}
}
```

cc-switch 行为要点：

- OpenClaw 使用 `models.providers.<id>` map，多个 provider 共存。
- provider 添加后可以注册 `agents.defaults.models` allowlist。
- 如果没有默认模型，可以设置 `agents.defaults.model.primary`。
- `env` 和 `tools` 是独立配置区，需要保留未知字段。
- OpenClaw MCP 在 cc-switch 中仍标记为后期能力，本 phase 不实现 MCP。

## 当前 Rust 项目缺口

### 1. Client 类型不完整

Rust 当前只把 Claude 和 Codex 作为可注入 client：

- Status 页面只显示 Claude/Codex 注入状态。
- Settings 的 Clients 只配置 Claude/Codex 路径。
- `ccr-cli` 只有 `claude_config.rs` 和 `codex_config.rs`。
- `InjectionSnapshot` 只有通用状态，没有 client 枚举/列表建模。

需要增加 OpenClaw 作为 UI 一等 client。

### 2. OpenClaw 不能按覆盖式 backup/restore 处理

Claude/Codex 当前是：

1. 激活前备份整个原文件。
2. 写入 CCR 管理配置。
3. deactivate 恢复原文件。

OpenClaw 更适合：

- 增量写入 `models.providers.ccr`。
- 在安全条件下补齐 `agents.defaults` 中的模型入口。
- deactivate / remove 只移除 CCR 管理的 provider entry，并清理明显指向 `ccr/` 的默认模型引用。
- 不删除用户其他 providers、agents、env、tools 等配置。
- 仍然可以做安全备份，但恢复不应粗暴覆盖用户激活后新增的配置。

### 3. OpenClaw 是累加式 provider，不存在“当前 provider”

Status 和 UI 文案不能叫 “Activate current provider” 或 “Switch provider”。

建议用：

- `Add CCR provider to OpenClaw`
- `Remove CCR provider from OpenClaw`

Status 应显示：

- Provider present / missing
- Config path
- CCR endpoint 是否匹配当前 port
- OpenClaw default model 是否指向 `ccr/<model>`

### 4. 默认模型策略需要复用现有 client model mapping

OpenClaw 注入时必须选择写入的 model。

默认策略：

- 优先使用 `Router.default`。
- 如果 route 是 `provider,model`，client provider 写入 model 部分。
- 如果 route 是 provider-only，写入保守默认 model：
  - 优先使用现有 Codex 默认模型配置
  - 否则使用 `gpt-5-codex`
- OpenClaw 内部默认模型引用写成 `ccr/<model>`。
- 不把 `provider,model` 原样写入 client 模型名，避免 client 误认为模型名包含 provider prefix。

后续可以把 `AppSettings` 扩展为 per-client 默认模型：

```json
{
  "AppSettings": {
    "openclaw_config_path": "...",
    "openclaw_model": "gpt-5-codex"
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
    OpenClaw,
}
```

扩展 `AppSettings`：

```rust
pub openclaw_config_path: Option<String>,
```

默认路径：

```text
~/.openclaw/openclaw.json
```

路径配置在 Settings -> Clients 中显示，和 Claude/Codex 保持一致。

### 2. OpenClaw 注入模块

新增：

```text
ccr-rust/crates/ccr-cli/src/openclaw_config.rs
```

核心函数：

```rust
openclaw_config_path()
openclaw_injection_snapshot(port)
activate_openclaw_ccr()
deactivate_openclaw_ccr()
build_openclaw_config(original_json5, port, model)
openclaw_points_to_ccr(path, port)
remove_openclaw_ccr_provider(original_json5)
```

写入规则：

- 支持读取 JSON5，写出标准 JSON。
- 文件不存在时创建：

```json
{
  "models": {
    "mode": "merge",
    "providers": {}
  }
}
```

- 确保 `models.providers` 存在。
- 写入或更新 `models.providers.ccr`：

```json
{
  "baseUrl": "http://127.0.0.1:<port>/v1",
  "apiKey": "any",
  "api": "openai-responses",
  "models": [
    {
      "id": "<model>",
      "name": "<model>"
    }
  ]
}
```

- 保留其他 providers、agents、env、tools 和未知字段。
- 如果 `agents.defaults.model.primary` 缺失，可以设置为 `ccr/<model>`。
- 如果 `agents.defaults.models` 缺失或不含 `ccr/<model>`，添加 alias。
- deactivate 只删除 `models.providers.ccr`，默认不删除用户手动维护的 agents/env/tools。
- 如果 `agents.defaults.model.primary` 指向 `ccr/`，deactivate 后应清空或移除该字段，避免 OpenClaw 指向不存在 provider。
- 如果 `agents.defaults.model.primary` 指向非 ccr provider，不修改。

### 3. Status 页面

Status 页面新增 client section：

- OpenClaw Config

显示：

- Config path
- Active / inactive / drifted
- CCR endpoint
- OpenClaw default model state
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

- OpenClaw config path

文案必须明确：

- 改路径只影响后续读取/写入。
- 不会立即移动文件。
- OpenClaw 是增量 provider 写入，不会覆盖完整配置。

### 5. CLI 后置策略

项目方向是 UI 优先，CLI 不是本 phase 的主要边界。

本 phase 可以为 UI 调用暴露 Rust 函数；是否增加用户可见 CLI 命令后置：

- 不强制增加 `ccr openclaw-activate`。
- 如果为了测试或临时调试增加 hidden/internal command，需要在文档中标记非主要入口。
- 用户文档以 UI 操作为主。

## 非目标

- 不实现 OpenClaw MCP。
- 不实现 OpenClaw env/tools 完整设置页。
- 不实现 OpenClaw session manager。
- 不实现 OpenClaw 代理接管。
- 不新增 OpenClaw 直连上游 provider 模式。
- 不把 OpenClaw 加入 provider pool 的候选概念；provider pool 仍属于 CCR server 上游路由。
- 不实现 OpenClaw endpoint speed test 专用逻辑。

## 预计修改文件

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-app-core/src/status.rs`
- `ccr-rust/crates/ccr-app-core/src/settings.rs`
- `ccr-rust/crates/ccr-cli/src/lib.rs`
- `ccr-rust/crates/ccr-cli/src/openclaw_config.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- `ccr-rust/crates/ccr-ui/src/settings_tab.rs`
- `ccr-rust/crates/ccr-ui/src/tray_manager.rs`（如果 task icon 要显示新 client 状态）
- `ccr-rust/docs/client-model-mapping.md`
- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`

可能需要：

- `ccr-rust/crates/ccr-cli/Cargo.toml` 增加 JSON5 依赖，或复用已有配置解析能力。

## 规划的测试用例

### OpenClaw config

- 文件不存在时创建 `models.mode = "merge"` 和 `models.providers`。
- JSON5 注释/尾逗号可读取。
- 写出后是合法 JSON。
- 保留未知 top-level 字段。
- 保留 `env`、`tools`、`agents` 下未知字段。
- 保留其他 `models.providers`。
- `models.providers.ccr.baseUrl` 指向 `http://127.0.0.1:<port>/v1`。
- `models.providers.ccr.api = "openai-responses"`。
- `models.providers.ccr.apiKey = "any"`。
- `models.providers.ccr.models[0].id` 使用推导 model。
- 缺少 `agents.defaults.model.primary` 时写入 `ccr/<model>`。
- 已有非 ccr primary 时不覆盖。
- `agents.defaults.models` 添加 `ccr/<model>` alias。
- remove 只删除 `models.providers.ccr`。
- remove 时如果 primary 指向 `ccr/`，清理或置空该字段。
- remove 时如果 primary 指向非 ccr provider，不修改。
- invalid JSON5 返回错误，不 panic。

### Status/UI

- `StatusSnapshot` 增加 OpenClaw 后仍能读取 Claude/Codex 状态。
- server stopped 时 OpenClaw Add 按钮禁用。
- server running 且 inactive 时 Add 按钮启用。
- active and current 时 Add 按钮禁用、Remove 启用。
- drifted 时展示 drifted 状态，并允许重新 Add。
- OpenClaw default model state 能显示 missing / ccr / external。
- Settings -> Clients 显示 OpenClaw path。
- 修改 OpenClaw path 不标记 server restart required。

### 安全和备份

- activate 前创建 timestamped backup 或 safety snapshot。
- 增量写入失败时不破坏原文件。
- 临时文件写入后原子 rename。
- Unix 下新写入文件权限不弱于当前 Claude/Codex 策略。
- deactivate 不覆盖激活后用户新增的其他 provider/env/tools/agents。

### 验证命令

- `cargo test --package ccr-cli --lib`
- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-server --bin ccr-ui`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- UI Status 页面能看到 Claude、Codex、OpenClaw 三类 client 状态。
- OpenClaw 可以通过 UI 添加 CCR provider 到 `~/.openclaw/openclaw.json`。
- OpenClaw 可以通过 UI 移除 CCR provider，且不影响其他 provider/env/tools/agents 未知字段。
- OpenClaw 初次添加时能注册 `ccr/<model>` 到默认模型/模型目录，且不覆盖已有非 CCR 默认模型。
- Status 页面点开时读取 OpenClaw live 文件即时状态。
- Settings 页面能配置 OpenClaw config path。
- 所有新增写入逻辑有单元测试覆盖。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 已实现 OpenClaw additive provider 写入/移除/status，并支持 JSON5 读取、标准 JSON 写出。
- Status 页面使用 additive client 状态：missing / current / drifted；没有把 OpenClaw 塞进 `InjectionSnapshot` 的 backup 文案。
- 初次写入会注册 `models.providers.ccr`、`agents.defaults.models["ccr/<model>"]`，并仅在 default primary 缺失时设置 `agents.defaults.model.primary`。
- 移除时只删除 `models.providers.ccr`，并只在 primary 指向 `ccr/` 时清理 primary；不清理用户其他 providers/env/tools/agents。
- 本次没有实现 OpenClaw MCP、env/tools 完整设置页、session manager、direct-to-provider 模式或 endpoint test 专用逻辑。
- OpenClaw 写入模型从 `Router.default` 推导；provider-only route 使用 `gpt-5-codex` fallback。
