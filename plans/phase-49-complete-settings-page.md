# Phase 49 - Complete Settings Page

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

把当前零散配置整理成完整设置页。

设置页需要覆盖应用行为、server、client 注入、网络代理、日志、目录、备份和启动项。它应该成为用户管理 CCR 桌面行为的主入口，而不是只依赖 CLI 或手动改配置文件。

本 phase 是 UI 优先方向的基础 phase。所有设置项优先通过桌面 UI 完成闭环；CLI 不作为本 phase 的主入口。

## 背景

`cc-switch` 的 settings 覆盖更完整：

- `../cc-switch/src-tauri/src/settings.rs`
- `../cc-switch/src/components/settings/*`
- `../cc-switch/src-tauri/src/services/proxy.rs`
- `../cc-switch/src-tauri/src/commands/global_proxy.rs`

当前 Rust 版已有状态页和部分配置能力，但设置页不完整，很多行为没有 UI 入口。

## 完成项

- [x] 建立统一 AppSettings 数据结构
- [x] Settings 页面增加分组导航
- [x] Startup 设置：开机自启、server auto start
- [x] Server 设置：host、port、log level、默认启动策略
- [x] Client 设置：Claude/Codex config path、注入状态
- [x] Network 设置：global proxy、per-provider proxy 预留
- [x] Failover 设置：enable、trigger、排序、添加、删除
- [x] Logs 设置：日志目录、日志等级、打开日志目录
- [x] Backup 设置：配置备份列表、恢复入口
- [x] Appearance 设置：theme 预留
- [x] 每个设置项保存后立即生效或提示需要重启
- [x] 设置读写有测试覆盖

## Settings 分组

建议页面结构：

```text
Settings
  Startup
  Server
  Clients
  Network
  Failover
  Logs
  Backups
  Appearance
  Advanced
```

## 行为规则

- 影响运行中 server 的设置，需要显示是否需要 restart
- 修改 client config path 不应立刻覆盖用户文件，必须通过 inject 操作触发
- 修改 port 后，Status 页面应显示待重启状态
- 设置保存失败必须显示具体错误
- 设置文件写入前保留现有未知字段

## 修改文件

预计涉及：

- `ccr-rust/crates/ccr-types/src/lib.rs`
- `ccr-rust/crates/ccr-config/src/lib.rs`
- `ccr-rust/crates/ccr-ui/src/*`

## 预留的开发偏差

- 本 phase 按 UI 优先落地，设置页成为主入口；没有新增 CLI 设置命令。
- Appearance 第一版提供 theme 字段和 UI 位置，未实现完整主题切换。
- Network 第一版提供 global proxy 设置入口，per-provider proxy 仅作为后续预留说明。
- Backup 第一版显示备份目录，完整备份恢复 UI 后续再做。
- 打开日志目录没有调用系统 GUI open，只显示路径，避免引入平台权限差异。
- 需要重启的设置用页面提示表达，没有实现细粒度 server/app restart 分类。

## 规划的测试用例

- AppSettings 默认值：缺少 settings 文件时生成完整默认设置。
- AppSettings merge：读取已有配置时保留未知字段。
- Settings 保存：修改 server port 后持久化，并标记 `restart_required`。
- Settings 保存：修改 log level 后如果可热更新，则立即生效；否则显示需要重启。
- Client path：修改 Claude/Codex config path 不会自动注入或覆盖用户文件。
- Startup 分组：开机自启和 server auto start 开关能读写同一份 settings。
- Network 分组：global proxy 字段保存后可重新读取。
- Logs 分组：日志目录为空时使用默认目录，非法目录返回可展示错误。
- Backups 分组：备份列表为空时显示空状态，有备份时按时间排序。
- UI 导航：每个 settings 分组可以切换，切换不丢失未保存提示。
- 错误处理：settings 写入失败时显示错误，不更新 UI 为已保存状态。
- 回归：Status 页面读取 settings 后仍能正常刷新 server 状态。

## 验证

```bash
cargo test --package ccr-config
cargo test --package ccr-cli
cargo test --package ccr-ui
cargo build --package ccr-ui
```

## 验收标准

- Settings 页面覆盖启动、server、client、network、logs、backups、appearance
- 设置读写会保留未知字段
- 需要重启的设置会明确提示
- 开机自启和 server auto start 能从设置页控制
- 日志和备份入口可用
- 设置逻辑有测试覆盖
