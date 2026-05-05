# Phase 58 - Logs Tab Focus-Aware Auto Refresh

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

优化 Logs 页面刷新方式：

- Logs 页面有焦点时自动刷新。
- Logs 页面没有焦点时不自动刷新。
- Logs 页面重新获得焦点时立即刷新。
- 去掉手动 `Refresh` 按钮。
- 保留 `Clear` 按钮。

这个 phase 的目标是让日志页面符合 UI-first 使用方式：用户切到 Logs 时看到最新状态，停留在 Logs 时自动跟进变化，离开 Logs 时不做无意义的文件读取和 UI 更新。

## 当前状态

当前 `LogsTab` 在构造时读取一次日志：

```rust
pub fn new() -> Self {
    Self {
        content: Self::load(),
    }
}
```

页面上有两个按钮：

- `Refresh`：点击后读取 `~/.claude-code-router/claude-code-router.log`
- `Clear`：清空日志文件后重新读取

这导致几个问题：

- 用户在 Logs tab 停留时，新日志不会自动出现。
- 用户切回 Logs tab 时，需要手动点 Refresh。
- 当前 Logs tab 会记录大量 `logs_refresh_clicked`，这些是 UI 操作噪音。
- Debug 时用户容易误判“没有日志”，实际只是页面没有刷新。

## 设计方向

### 1. Logs tab 增加焦点/可见状态

为 `LogsTab` 增加状态字段：

- `content: String`
- `was_visible: bool`
- `last_refresh: Option<Instant>`
- `last_file_modified: Option<SystemTime>` 或 `last_loaded_len: usize`

`show()` 被调用时可以视为 Logs tab 当前可见；如果上一帧不可见、本帧可见，即“获得焦点”，立即刷新。

由于当前 UI 是 tab 页面结构，非当前 tab 的 `show()` 不会被调用，因此可以用父级 tab 切换状态或 `show()` 调用本身判断可见性。

### 2. 自动刷新策略

Logs tab 可见时：

- 首次进入立即刷新。
- 每隔固定间隔刷新一次，例如 `500ms` 或 `1s`。
- 刷新时尽量避免无意义重绘：
  - 如果文件不存在，显示空内容。
  - 如果文件 metadata 未变化，可以跳过读取。
  - 如果 metadata 不可靠，再 fallback 到读取并比较内容。

Logs tab 不可见时：

- 不主动读取文件。
- 不记录刷新事件。
- 不触发 repaint。

重新获得焦点时：

- 立即刷新。
- 可以写一条低噪音 UI log：`logs_tab_focused`，但不记录每次自动刷新，避免刷屏。

### 3. 移除 Refresh 按钮

Logs tab 顶部只保留：

- `Clear`

不再显示：

- `Refresh`

如果需要反馈，可以显示一行轻量状态，例如 `Last updated HH:MM:SS`，但不是必要目标。本 phase 优先保持 UI 简洁。

### 4. Clear 行为

`Clear` 保持现有能力：

- 创建 parent directory。
- truncate app log file。
- 写入 `logs_clear_clicked`。
- 立即刷新显示内容。

注意：清空后写入 `logs_clear_clicked`，所以页面不会完全空白，这是当前行为，可以保留。

## 非目标

- 不实现完整日志过滤器。
- 不合并 server tracing log 和 app log。
- 不做日志虚拟列表。
- 不改日志文件路径。
- 不引入后台线程或 async runtime。
- 不在 Logs tab 不可见时轮询文件。

## 实际修改文件

- `ccr-rust/crates/ccr-ui/src/logs_tab.rs`
- `ccr-rust/crates/ccr-ui/src/app.rs`

## 规划的测试用例

- `LogsTab` 初始化时可以为空，不因日志文件不存在 panic。
- Logs tab 第一次显示时会读取日志。
- Logs tab 连续可见且刷新间隔未到时不会重复读取。
- Logs tab 连续可见且刷新间隔到达时会刷新。
- Logs tab 从不可见切换到可见时立即刷新。
- Logs tab 不可见时不刷新。
- `Clear` 清空日志后立即重新加载内容。
- UI 不再渲染 `Refresh` 按钮。
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- 用户切到 Logs 页面时无需点击按钮即可看到最新日志。
- 用户停留在 Logs 页面时，新日志会自动出现。
- 用户离开 Logs 页面后不会继续轮询日志文件。
- 顶部不再出现 `Refresh` 按钮。
- `Clear` 仍然可用。
- 自动刷新不会产生大量 `logs_refresh_clicked` 日志噪音。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- `LogsTab::show()` 只会在当前 tab 是 Logs 时被调用，因此可见时自动刷新通过 `show()` 内的 1 秒间隔实现；不可见时由 `app.rs` 在其他 tab 分支调用 `set_visible(false)`，不会读取日志文件。
- 没有增加 `Last updated` 文案，保持 Logs 页面顶部只剩 `Clear`，避免新增视觉噪音。
- 文件变化判断使用 metadata 的 `modified + len` 快照；如果 metadata 不可用，会退化为空快照并在下一次 due refresh 重新比较。
- 自动刷新不写 `logs_refresh_clicked`，只在重新进入 Logs 时写一条 `logs_tab_focused`。

## 验证结果

- `cargo test --package ccr-ui`：通过。
- `cargo test --workspace`：通过。
- `cargo build --package ccr-ui`：通过。
- `cargo llvm-cov --workspace --lib --summary-only`：通过，line coverage `81.57%`。
