# Phase 43 — Codex Backend and Config Switch

**状态：** ✅ 已完成
**优先级：** P1

## 目标

补齐 Codex 侧能力，使 CCR 不只服务 Claude Code：

- Codex 请求可通过 CCR 的 `/v1/responses` 后端进入
- CCR 可写入/恢复 `~/.codex/config.toml`
- UI 可操作 Codex 配置切换
- 新增逻辑覆盖率大于 80%

## 完成项

- [x] 新增 `POST /v1/responses`
- [x] 支持 `provider,model` 路由
- [x] 支持普通模型名使用默认 provider
- [x] 上游请求移除 provider 前缀
- [x] 新增 `ccr codex-activate`
- [x] 新增 `ccr codex-deactivate`
- [x] 备份 `~/.codex/config.toml`
- [x] 支持原始 Codex config 不存在时恢复删除
- [x] UI Status Tab 添加 Codex Config Switch
- [x] CLI 单元测试
- [x] Server 单元测试
- [x] 相关 crate 覆盖率大于 80%

## 配置写入格式

```toml
model_provider = "ccr"

[model_providers.ccr]
name = "CCR"
base_url = "http://127.0.0.1:<port>/v1"
wire_api = "responses"
requires_openai_auth = false
```

## 验证

```bash
cargo test --package ccr-cli
cargo test --package ccr-server
cargo build --package ccr-server
cargo build --package ccr-ui
cargo llvm-cov --package ccr-cli --ignore-filename-regex 'claude_config.rs|main.rs' --fail-under-lines 80 --summary-only
cargo llvm-cov --package ccr-server --lib --fail-under-lines 80 --summary-only
```

## 后续增强

- [ ] 增加 Codex config 的 dry-run/preview 命令
- [ ] 增加 `/v1/responses` 到 Anthropic Messages 的转换器
- [ ] 增加端到端 Codex CLI 实机测试
