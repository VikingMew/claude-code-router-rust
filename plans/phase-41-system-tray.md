# Phase 41 — 系统托盘 / Task Icon

**状态：** 已完成  
**优先级：** P2

---

## 目标

为 `ccr-ui` 添加系统托盘/任务栏图标，提供本地桌面 UI 快捷入口和 CCR 常用操作。

该 phase 的 task icon 不打开 Web UI。`Open CCR UI` 只显示并聚焦当前桌面窗口。

---

## 功能清单

- [x] 托盘图标初始化
- [x] 关闭窗口时隐藏到托盘
- [x] 点击托盘图标恢复桌面窗口
- [x] `Open CCR UI`
- [x] `Hide Window`
- [x] `Server > Start Server`
- [x] `Server > Stop Server`
- [x] `Server > Restart Server`
- [x] `Server > Show Status`
- [x] `Inject > Inject Claude Code`
- [x] `Inject > Inject Codex`
- [x] `Switch Provider`
- [x] `Quit`

---

## 菜单设计

```text
Open CCR UI
Hide Window

Server
  Start Server
  Stop Server
  Restart Server
  Show Status

Inject
  Inject Claude Code
  Inject Codex

Switch Provider
  provider,first-model
  ...

Quit
```

---

## 实现说明

### Server 操作

托盘菜单通过 bundled `ccr` 二进制执行：

```bash
ccr start
ccr stop
ccr restart
```

### Inject 操作

托盘菜单通过 CLI 复用现有配置注入逻辑：

```bash
ccr claude-activate
ccr codex-activate
```

### Provider 切换

启动 UI 时读取 `~/.claude-code-router/config.json`，从每个 provider 的第一个 model 生成 route：

```text
provider_name,first_model
```

点击 provider 后写入：

```json
Router.default
```

---

## 修改文件

- `ccr-rust/crates/ccr-ui/src/tray_manager.rs`
- `ccr-rust/crates/ccr-ui/src/app.rs`
- `ccr-rust/crates/ccr-ui/Cargo.toml`

---

## 验证命令

```bash
cargo build --package ccr-ui
cargo test --package ccr-ui
```
