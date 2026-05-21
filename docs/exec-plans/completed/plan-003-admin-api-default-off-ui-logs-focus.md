# plan-003 - Admin API Default Off And UI Logs Focus

**状态：** completed
**优先级：** P0
**计划编号：** plan-003
**最后更新：** 2026-05-08

## 目标

默认关闭 admin API，并暂停把 admin API 作为短期维护重点。

近期工程焦点转向：

- 桌面 UI 的主工作流闭环。
- 用户可见、agent 可查询的日志写入和日志查询。
- 真实请求/attempt 的最小可观测性。
- server runtime API 的 through-CCR 行为稳定性。

`/api/admin/*` 这类管理 API 暂时不作为公开产品面维护。需要保留时必须显式启用，并且默认不能暴露。

## 非目标

- 不删除 client runtime API：`/v1/messages`、`/v1/responses`。
- 不删除 UI 需要的受控 API：config、logs、route-pool status、runtime metrics、presets。
- 不在本计划内设计完整远程管理 API。
- 不把 UI 功能迁到 admin API。
- 不引入外部 observability backend。

## 背景

当前 server 注册了：

- `POST /api/admin/reload`

该 handler 代码中有 TODO：尚未接入主 auth check。对于本地桌面产品来说，这类 admin API 的短期价值低于 UI 状态、配置写入、日志和 Route Pool 诊断。

继续维护 admin API 会带来风险：

- 安全边界容易扩大。
- 新功能可能绕过 UI-first 产品路径。
- agent 可能把管理能力继续加到 `/api/admin/*`，形成第二套产品面。
- 日志和 UI 仍未完全闭环时，admin API 会分散工程重点。

## 设计方向

### 1. 默认关闭 admin API

新增或复用 app setting/config flag：

```json
{
  "AppSettings": {
    "admin_api_enabled": false
  }
}
```

命名可以按 Rust 类型和 JSON 风格最终调整，但语义必须清楚：

- 默认值是 false。
- 未配置时视为 false。
- 只有显式配置 true 时才注册或允许 `/api/admin/*`。

实现可选方式：

- 启动时只在 enabled 时注册 admin routes。
- 或 handler 入口检查 flag，未开启返回 `404 Not Found` 或 `403 Forbidden`。

推荐优先使用“不注册 route”或 `404`，减少它作为公开 API 的暗示。

### 2. admin API 不作为短期扩展点

短期禁止新增 `/api/admin/*` 功能，除非同时满足：

- 有明确 UI 无法覆盖的开发/恢复需求。
- 默认关闭。
- 有 auth check。
- 有文档说明用途、风险和手动启用方式。
- 有测试覆盖 disabled/enabled 两种行为。

### 3. 聚焦 UI 和 logs

近期 P0/P1 工作优先级：

- UI 内完成 server 状态、client 状态、provider endpoint test、Route Pool 状态和 logs 的闭环。
- 确保关键用户动作写入结构化日志。
- logs query API 保持可用，并服务 UI 与 agent 诊断。
- Runtime metrics 继续区分真实 client traffic 和 endpoint test。

### 4. 文档边界

长期文档需要明确：

- admin API 默认关闭。
- admin API 暂不属于主产品面。
- UI 是管理入口。
- logs/query 是当前 agent-readable 诊断入口。
- 任何未来 admin API 都必须显式启用、鉴权、测试和文档化。

## 修改文件

预计修改：

- `docs/long-term-roadmap.md`
- `docs/SECURITY.md`
- `docs/RELIABILITY.md`
- `docs/exec-plans/active/README.md`
- `docs/exec-plans/completed/`
- `crates/ccr-types/src/lib.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-server/src/handlers/admin.rs`

可能修改：

- `crates/ccr-ui/src/settings_tab.rs`
- `crates/ccr-app-core/src/settings.rs`
- `README.md`

## 验收测试

文档验收：

```sh
rg -n "admin API|/api/admin|admin_api_enabled|默认关闭|default off" docs README.md ARCHITECTURE.md
./scripts/check-docs-structure.sh
```

代码验收：

```sh
rg -n "/api/admin|reload_config|admin_api" crates/ccr-server crates/ccr-types crates/ccr-ui crates/ccr-app-core
cargo fmt --check
cargo test --package ccr-server
cargo test --package ccr-types
cargo test --workspace
```

行为验收：

- 默认配置下 `POST /api/admin/reload` 不可用。
- 显式启用 admin API 后，`POST /api/admin/reload` 可用且必须通过 auth check。
- admin API disabled/enabled 行为有 server tests。
- UI 仍能通过现有 config/logs/route-pool/runtime-metrics API 工作。
- logs query API 不受 admin API 关闭影响。

## 决策日志

- 2026-05-08：确认近期不维护 admin API 产品面，默认关闭。
- 2026-05-08：确认优先级转向桌面 UI 功能闭环和日志写入/查询。
- 2026-05-08：确认 `/api/admin/*` 未来如恢复，需要显式启用、鉴权、测试和文档。

## 完成记录

2026-05-08 完成：

- 在 `AppSettings` 中新增 `admin_api_enabled`，默认 false。
- `POST /api/admin/reload` 默认返回 `404 Not Found`。
- admin API 显式启用后必须通过主 auth check。
- 增加 server tests 覆盖 disabled 和 enabled/auth 行为。
- 长期文档记录 admin API 默认关闭、短期不作为扩展点，近期 focus UI 和 logs/query。

验证命令：

```sh
cargo fmt --check
cargo test --package ccr-types
cargo test --package ccr-server
rg -n "admin API|/api/admin|default-off|默认关闭|logs query|UI.*logs|admin_api_enabled" docs/long-term-roadmap.md docs/SECURITY.md docs/RELIABILITY.md docs/exec-plans/completed/plan-003-admin-api-default-off-ui-logs-focus.md docs/exec-plans/completed/
```
