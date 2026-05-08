# plan-004 - Linux UI Startup GTK/EGL Resilience

**状态：** completed
**优先级：** P0
**计划编号：** plan-004
**最后更新：** 2026-05-08

## 目标

修复 Ubuntu/Linux 下运行 `cargo run --bin ccr-ui` 时 UI 启动崩溃的问题。

当前错误示例：

```text
libEGL warning: failed to get driver name for fd -1
MESA: error: ZINK: failed to choose pdev
libEGL warning: egl: failed to create dri2 screen

Gtk-CRITICAL **: gtk_icon_theme_get_for_screen: assertion 'GDK_IS_SCREEN (screen)' failed
thread 'main' panicked at gtk-0.18.2/src/auto/menu.rs:29:9:
GTK has not been initialized. Call `gtk::init` first.
```

最终目标：

- `ccr-ui` 在普通 Ubuntu 桌面环境可正常启动。
- tray 初始化失败不能导致主 UI 崩溃。
- 无 tray、无完整 GTK screen、Wayland/X11 差异、GPU/EGL fallback 场景都有明确降级路径。
- README/release docs 记录 Linux 启动依赖和故障排查命令。

## 非目标

- 不重写 UI 框架。
- 不要求所有 headless/server 环境都能显示原生 GUI。
- 不在本计划中做完整 Linux 打包。
- 不把 tray 作为 UI 启动的硬依赖。
- 不把 GPU/EGL warning 全部消除作为完成标准；只要求不崩溃并有可用 fallback。

## 背景

当前启动路径：

- `crates/ccr-ui/src/main.rs` 调用 `eframe::run_native(...)`。
- `CcrApp::new()` 中创建 `TrayManager` 并调用 `tray_manager.init()`。
- `TrayManager::init()` 直接创建 `tray_icon::menu::Menu`、`MenuItem`、`Submenu` 和 `TrayIcon`。

问题：

- Linux `tray-icon` 依赖 GTK/libappindicator 路径。
- 当前代码没有显式初始化 GTK，也没有把 tray 初始化中的 panic 隔离。
- 即使 `tray_manager.init()` 返回 `Err` 会被打印，GTK crate 内部 panic 仍会终止主进程。
- GPU/EGL/ZINK warning 说明当前环境可能没有可用 DRI/EGL device；eframe/wgpu/glow 需要有 fallback 策略。

产品角度：

- CCR 是桌面 UI 优先产品，主窗口必须优先启动。
- system tray 是增强能力，不应该阻止用户进入 Status/Config/Logs。

## 设计方向

### 1. Tray 初始化降级

`TrayManager::init()` 必须变成 best-effort。

实现要求：

- Linux 下在创建 tray menu 前执行 GTK 初始化检查。
- 如果 GTK 不可用、无 screen、无 appindicator/tray backend，返回错误并禁用 tray。
- 使用 `std::panic::catch_unwind` 隔离 `tray-icon` 或 `gtk` 内部 panic。
- `CcrApp::new()` 记录错误到 stderr 和 app log，但继续启动主窗口。
- `TrayManager::poll_event()` 在 tray 未启用时稳定返回 `TrayEvent::None`。

可接受实现：

- 新增 `TrayInitStatus` 或 `TrayAvailability`，用于测试和 UI Settings/Status 展示。
- Linux 环境变量 `CCR_DISABLE_TRAY=1` 可强制禁用 tray，便于开发、CI、远程桌面和问题排查。

### 2. GTK 初始化策略

Linux 下需要明确初始化顺序：

- 尝试调用 GTK 初始化，或使用 `tray-icon` 推荐的 Linux 初始化路径。
- 初始化失败时禁用 tray，不 panic。
- 不把 GTK 初始化放到非 Linux 平台路径。

验收时需确认：

- 缺少 display/screen 时不会 panic。
- GTK 初始化失败时不会创建 menu。
- tray 成功时现有菜单功能仍可用。

### 3. eframe 渲染 fallback

针对 EGL/ZINK/Mesa warning，主 UI 应尽量使用更稳的渲染配置。

候选方向：

- 优先使用 eframe `glow` renderer 或禁用 wgpu 路径，降低 Vulkan/ZINK 依赖。
- 文档记录环境变量 fallback，例如：

```sh
WGPU_BACKEND=gl cargo run --bin ccr-ui
LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui
CCR_DISABLE_TRAY=1 cargo run --bin ccr-ui
```

具体变量需以实际 eframe/wgpu 版本验证为准，不能只凭猜测写成唯一方案。

### 4. 日志和用户反馈

启动阶段失败需要可诊断：

- tray disabled reason 写入 stderr。
- 如果 app log 初始化可用，写入 app log。
- 不能只显示底层 GTK panic。
- README/release docs 增加 Linux troubleshooting 小节。

## 修改文件

预计修改：

- `crates/ccr-ui/src/main.rs`
- `crates/ccr-ui/src/app.rs`
- `crates/ccr-ui/src/tray_manager.rs`
- `crates/ccr-ui/Cargo.toml`
- `docs/release.md`
- `README.md`
- `docs/exec-plans/active/README.md`
- `docs/exec-plans/tech-debt-tracker.md`

可能修改：

- `crates/ccr-app-core/src/logging.rs`
- `docs/RELIABILITY.md`

## 验收测试

代码验收：

```sh
cargo fmt --check
cargo test --package ccr-ui
cargo check --package ccr-ui
cargo test --workspace
```

结构验收：

```sh
rg -n "CCR_DISABLE_TRAY|catch_unwind|gtk::init|TrayInit|tray.*disabled|WGPU_BACKEND|LIBGL_ALWAYS_SOFTWARE" crates/ccr-ui README.md docs
```

手工验收：

```sh
cargo run --bin ccr-ui
CCR_DISABLE_TRAY=1 cargo run --bin ccr-ui
LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui
```

需要观察：

- 主窗口能打开。
- tray 初始化失败时进程不 panic。
- `Gtk-CRITICAL` 或 `libEGL` warning 即使出现，也不会阻止 UI 打开。
- tray disabled reason 有可读输出。
- Status、Config、Router、Logs tab 仍能打开。

可选环境验收：

```sh
WAYLAND_DISPLAY= DISPLAY= CCR_DISABLE_TRAY=1 cargo run --bin ccr-ui
```

如果没有显示环境，允许 eframe 自身失败，但错误必须是主窗口显示环境不可用，而不是 tray/GTK menu panic。

## 决策日志

- 2026-05-08：确认 Linux 启动 crash 来自 tray/GTK 初始化路径，tray 不应成为主窗口硬依赖。
- 2026-05-08：确认 GPU/EGL warning 与 tray/GTK panic 需要分开处理；本计划完成标准是不崩溃并有 documented fallback。
- 2026-05-08：纠正 fallback 偏差：`WINIT_UNIX_BACKEND=x11` 对 `winit 0.30` 已移除，不能作为当前方案；实际可用方案是移除 `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`，让 winit 在 `DISPLAY` 存在时走 X11。

## 完成记录

- 2026-05-08：Linux tray 初始化改为 best-effort，支持 `CCR_DISABLE_TRAY=1` 强制禁用。
- 2026-05-08：Linux tray menu 创建前显式执行 `gtk::init()`；GTK/tray 初始化失败或 panic 时返回错误并让主 UI 继续启动。
- 2026-05-08：`TrayManager::poll_event()` 在 tray 未启用时稳定返回 `TrayEvent::None`。
- 2026-05-08：UI 启动改用 `eframe::Renderer::Glow`，在 WSL、`LIBGL_ALWAYS_SOFTWARE` 或 `CCR_SOFTWARE_RENDERING` 场景下请求软件渲染。
- 2026-05-08：README 和 release docs 补充 Ubuntu/Linux 依赖与 WSL/EGL/tray fallback 命令。
- 2026-05-08：WSL/fallback 场景默认移除 `WAYLAND_DISPLAY` 和 `WAYLAND_SOCKET`，设置 `GDK_BACKEND=x11` 与 `LIBGL_ALWAYS_SOFTWARE=1`，并捕获 eframe/glutin 启动 panic，避免 `failed to find a matching configuration for creating glutin config` 直接打印裸栈。
- 2026-05-08：用户在 WSL2 中确认 `env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET CCR_DISABLE_TRAY=1 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui` 可成功启动。

验证：

```sh
cargo fmt --check
cargo test --package ccr-ui
cargo check --package ccr-ui
cargo test --workspace
./scripts/check-docs-structure.sh
```

说明：

- 未在自动验证中启动长期运行的 GUI 进程。
- WSL/远程桌面建议手动使用 `env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET CCR_DISABLE_TRAY=1 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui` 验证实际窗口显示。
