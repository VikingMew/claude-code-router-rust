# Phase 48 - Auto Launch

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

增加开机自启动能力。

用户可以在设置页开启或关闭 CCR 桌面程序的系统登录启动。程序启动后应按现有策略默认启动 server，并让 Status 页面立即显示真实状态。

本 phase 遵循 UI 优先：开机自启动和 server auto start 必须首先通过设置页完成。CLI 只作为后置能力或调试入口，不作为 P0 验收条件。

## 背景

`cc-switch` 有 auto launch 支持：

- `../cc-switch/src-tauri/src/auto_launch.rs`

当前 Rust 版没有系统登录项管理。用户需要手动启动应用，server 默认启动策略也缺少设置入口。

## 完成项

- [x] 增加 auto launch 设置项
- [x] macOS 支持登录启动
- [x] Windows 支持登录启动
- [x] Linux 如不支持，明确显示 unsupported
- [x] UI 设置页提供开关
- [x] 启动时读取设置并恢复 server 默认启动策略
- [x] Status 页面显示 server 启动状态
- [x] 写入和读取设置有测试覆盖

## 行为规则

- Auto launch 只控制应用是否随系统登录启动
- Server auto start 是独立设置，但默认应开启
- 应用启动时如果 `server_auto_start = true`，需要启动 CCR server
- 如果 server 已经运行，不重复启动
- 如果启动失败，Status 页面显示失败原因

## 设置结构

建议加入：

```json
{
  "AppSettings": {
    "auto_launch": false,
    "server_auto_start": true
  }
}
```

## UI 设计

Settings 页面增加 `Startup` 分组：

- Launch at login
- Start server when app opens
- 当前平台状态
- 最近一次启动错误

## CLI 后置能力

CLI 可以后续提供 `autolaunch status/enable/disable` 和 `server-auto-start enable/disable`，但本 phase 不以 CLI 为主入口。

## 修改文件

预计涉及：

- `ccr-rust/crates/ccr-cli/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/*`
- `ccr-rust/crates/ccr-config/src/lib.rs`
- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/packaging/macos/*`
- `ccr-rust/packaging/windows/*`

## 预留的开发偏差

- 本 phase 按 UI 优先落地，CLI auto launch 命令未作为主入口实现；只保留底层 helper 供 UI 调用。
- macOS 第一版使用 LaunchAgent plist 文件实现。
- Windows 第一版使用 Startup folder `.cmd` 文件实现。
- Linux 第一版明确返回 unsupported。
- `server_auto_start` 只影响桌面 UI 启动时是否自动启动 server，不改变 `ccr code` 等 CLI 行为。

## 规划的测试用例

- Settings 默认值：新用户默认 `auto_launch = false`，`server_auto_start = true`。
- Settings 读取：缺少 AppSettings 时自动补默认值。
- Settings 保存：切换 auto launch 后设置持久化，未知字段不丢失。
- Platform adapter：macOS enable/disable/status 调用正确的平台实现。
- Platform adapter：Windows enable/disable/status 调用正确的平台实现。
- Unsupported 平台：返回明确 unsupported 错误，不 panic。
- UI：Startup 设置页能显示 enabled/disabled/unsupported。
- UI：开关 auto launch 会更新设置并调用平台 adapter。
- Server auto start：app 启动时如果 server 已运行，不重复启动。
- Server auto start：app 启动时如果 server 未运行且设置开启，会调用 start server。
- Server auto start 失败：Status 页面显示错误原因。
- UI：Startup 开关状态和 settings 文件一致。

## 验证

```bash
cargo test --package ccr-config
cargo test --package ccr-ui
cargo build --package ccr-ui
```

## 验收标准

- 用户可以开启和关闭开机自启动
- 用户可以配置应用打开时是否自动启动 server
- Status 页面能显示启动后的真实 server 状态
- macOS 和 Windows 有平台实现或明确错误
- 设置持久化并有测试覆盖
