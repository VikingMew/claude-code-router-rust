# Phase 66 - Per-Tab Scroll Areas

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

让每个 UI tab 都拥有自己的滚动区域，保证内容较长时仍可访问，并且切换 tab 时不会互相污染滚动状态。

这个 phase 的核心是修正桌面 UI 的基础布局：tab header 固定在顶部，每个 tab 的内容区域独立滚动。

## 背景

当前 `ccr-ui` 的主布局是：

```rust
egui::TopBottomPanel::top("tabs").show(ctx, ...);
egui::CentralPanel::default().show(ctx, |ui| match self.tab {
    Tab::Config => self.config_tab.show(ui),
    Tab::Router => self.router_tab.show(ui),
    Tab::Presets => self.preset_tab.show(ui),
    Tab::Status => self.status_tab.show(ui),
    Tab::Logs => self.logs_tab.show(ui),
    Tab::Transformers => self.transformers_tab.show(ui),
    Tab::TokenCounter => self.token_counter_tab.show(ui),
    Tab::Settings => self.settings_tab.show(ui),
});
```

只有 Logs tab 内部使用了：

```rust
egui::ScrollArea::vertical()
```

其他 tab 内容变长时可能出现：

- 底部按钮不可见。
- provider list 太长时无法完整编辑。
- Router 中 provider pool / failover 过长时不可达。
- Status 中 client 状态增多后内容溢出。
- Settings 左侧导航和右侧内容在小窗口下不稳定。

## 设计方向

### 1. 每个 tab 独立 ScrollArea

在 `app.rs` 的 `CentralPanel` 中为每个 tab 包一层独立 `ScrollArea`：

```rust
egui::ScrollArea::vertical()
    .id_salt("status_tab_scroll")
    .auto_shrink([false, false])
    .show(ui, |ui| {
        self.status_tab.show(ui);
    });
```

每个 tab 使用稳定且不同的 `id_salt`：

- `status_tab_scroll`
- `config_tab_scroll`
- `router_tab_scroll`
- `presets_tab_scroll`
- `transformers_tab_scroll`
- `token_counter_tab_scroll`
- `settings_tab_scroll`

Logs tab 已经有自己的滚动逻辑，本 phase 需要二选一：

- 保留 Logs 内部滚动，不在 app 外层再包一层。
- 或迁移到 app 外层统一滚动，移除 Logs 内部滚动。

推荐：保留 Logs 内部滚动。Logs 是大文本视图，已有自动刷新和焦点逻辑，不要在本 phase 改动它。

### 2. 避免嵌套滚动

不要给同一个 tab 同时加外层和内层垂直滚动。

已知例外：

- Logs tab 保留自己的 `ScrollArea`。

如果未来某个 tab 内部有局部滚动，例如大型 JSON preview，需要明确局部滚动高度，不能让它和页面滚动抢事件。

### 3. 固定顶部 tab bar

Top tab bar 必须继续使用 `TopBottomPanel::top("tabs")`，不跟随内容滚动。

验收行为：

- 滚动 Status/Config/Router/Settings 内容时，顶部 tab bar 不移动。
- 切换 tab 后，每个 tab 恢复自己的滚动位置。

### 4. Settings 页面特殊处理

Settings 页面是左右布局：

- 左侧 section nav
- 右侧 section content

本 phase 先使用整个 Settings tab 的页面级滚动。

后续如果 Settings 内容继续变长，可以单独 phase 改成：

- 左侧 nav 固定
- 右侧 content 独立滚动

本 phase 不做这个复杂拆分。

### 5. Status 页面特殊处理

Status 页面已经是第一页面，内容会继续增长：

- Server
- Routing
- Client Injection
- Claude / Codex / OpenCode / OpenClaw

Status 必须有自己的稳定滚动状态，不能和 Config/Router 共用。

### 6. Router / Config 页面特殊处理

Config 页面 provider 多时应滚动。

Router 页面 provider pool/failover 多时应滚动。

按钮仍应在内容流中，不强制 sticky footer。

## 非目标

- 不重新设计 tab 顺序。
- 不把 Config 改名为 Providers。
- 不引入 virtual list。
- 不实现 sticky Save bar。
- 不重做 Settings 左右布局。
- 不修改 Logs tab 自动刷新逻辑。
- 不修改 provider endpoint test 行为。

## 预计修改文件

- `ccr-rust/crates/ccr-ui/src/app.rs`

可能需要：

- `ccr-rust/crates/ccr-ui/src/logs_tab.rs`（如果决定迁移 Logs 滚动，但本 phase 推荐不动）

## 规划的测试用例

### 编译/结构

- `app.rs` 为 Status tab 使用独立 `ScrollArea`。
- `app.rs` 为 Config tab 使用独立 `ScrollArea`。
- `app.rs` 为 Router tab 使用独立 `ScrollArea`。
- `app.rs` 为 Presets tab 使用独立 `ScrollArea`。
- `app.rs` 为 Transformers tab 使用独立 `ScrollArea`。
- `app.rs` 为 Token Counter tab 使用独立 `ScrollArea`。
- `app.rs` 为 Settings tab 使用独立 `ScrollArea`。
- Logs tab 不出现双层垂直 scroll。

### UI 行为

- provider 数量较多时 Config tab 可滚动到底部。
- provider pool/failover 数量较多时 Router tab 可滚动到底部。
- Status client 区块较多时 Status tab 可滚动到底部。
- Settings 内容超过窗口高度时 Settings tab 可滚动到底部。
- 顶部 tab bar 滚动时保持固定。
- 切换 tab 后每个 tab 的滚动位置彼此独立。

### Regression

- Logs tab 可继续显示长日志内容。
- Logs tab focus auto-refresh 逻辑不受影响。
- Token Counter 输入框仍可正常编辑。
- Provider inline test 结果展示不受影响。

### 验证命令

- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-ui --bin ccr-server`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- 每个主要 tab 都有独立滚动区域。
- Logs tab 没有双层垂直滚动。
- 长内容不会被窗口底部截断。
- 顶部 tab bar 不随内容滚动。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 实现采用 `app.rs` 统一 helper `show_tab_scroll()` 包装 Status、Config、Router、Presets、Transformers、Token Counter、Settings。
- Logs tab 保留自身内部 `ScrollArea`，没有在 app 外层再包一层，避免双层垂直滚动。
- Settings 暂时仍是整个 tab 页面级滚动，没有拆成左侧 nav 固定、右侧内容独立滚动。
