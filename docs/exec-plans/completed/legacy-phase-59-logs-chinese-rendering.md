# Phase 59 - Logs Chinese Rendering and Daily Rotation

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

修复 Logs 页面不能正确显示中文的问题，并让 app log 按天 rotate。

日志中出现中文内容时，Logs tab 应该直接显示可读中文，例如：

```text
message="这是一个什么项目？"
```

而不是显示为不可读字符、乱码、方框，或者 JSON unicode escape：

```text
message="\u8fd9\u662f\u4e00\u4e2a\u4ec0\u4e48\u9879\u76ee\uff1f"
```

## 当前怀疑点

当前日志链路是：

1. `append_app_log()` 调用 `format_app_log_line()`
2. `format_app_log_line()` 用 `serde_json::to_string()` quote value
3. 写入 `~/.claude-code-router/claude-code-router.log`
4. Logs tab 用 `std::fs::read_to_string()` 读取
5. `egui::TextEdit::multiline()` 渲染

潜在问题：

- `serde_json::to_string()` 可能把非 ASCII 字符转成 `\uXXXX` escape，导致日志文件本身不含中文。
- Logs tab 使用 `TextEdit::multiline(&mut &str)` 的只读展示方式可能不是最佳日志渲染控件。
- 默认 egui 字体在部分环境下可能缺少 CJK glyph，导致中文显示为方框。
- 某些 upstream response body 内层本身是 JSON 字符串，可能出现二次转义，需要只处理我们自己的 log formatter，不应该破坏原始 JSON 语义。
- 当前 app log 固定写入 `~/.claude-code-router/claude-code-router.log`，长期运行后文件会不断变大，也不方便按日期定位问题。

## 设计方向

### 1. 日志 formatter 保留 Unicode

调整 app log value quoting：

- 继续使用 JSON 风格双引号，避免空格、换行打乱 log line。
- 只 escape 必须 escape 的字符：
  - `\`
  - `"`
  - 控制字符，如 newline/tab 已由 `sanitize_value()` 处理为空格
- 不把中文转成 `\uXXXX`。

示例：

```text
event="messages_received" prompt="这是一个什么项目？"
```

需要保留：

- 单行日志。
- 字段值带空格仍被 quote。
- API key redaction 不受影响。
- response truncation 仍按字符数，不按 byte 破坏 UTF-8。

### 2. Logs tab 使用更稳的只读渲染

评估是否把当前：

```rust
egui::TextEdit::multiline(&mut display)
```

改为更明确的只读模式：

```rust
egui::TextEdit::multiline(&mut display)
    .interactive(false)
```

或者改用 `ui.monospace(...)` / `Label`。

如果 `TextEdit` 字体正常支持中文，保留 TextEdit 即可；否则需要配置 CJK fallback 字体。

### 3. 字体支持

检查 UI 是否已有字体配置。

如果没有 CJK 字体：

- 在 UI 启动时注册系统可用 CJK 字体或 bundled fallback。
- macOS 优先尝试系统字体：
  - `/System/Library/Fonts/PingFang.ttc`
  - `/System/Library/Fonts/STHeiti Light.ttc`
  - `/System/Library/Fonts/STHeiti Medium.ttc`
- 字体注册应该是 best-effort，找不到不 panic。

本 phase 优先修 formatter；只有确认是 glyph 问题时再加字体 fallback。

### 4. App log 按天 rotate

调整 app log 路径策略：

- 当前日期日志写入：

```text
~/.claude-code-router/claude-code-router-YYYY-MM-DD.log
```

- `app_log_path()` 返回当天日志文件。
- 新增可测试 helper，例如：

```rust
app_log_path_for_date(date)
app_log_file_name_for_date(date)
```

- Logs tab 默认读取当天日志。
- `Clear` 默认清空当天日志。
- 自动刷新只跟踪当天日志文件。

本 phase 不做日期选择器。历史日志保留在同目录下，后续可以在 Logs 页面增加日期选择。

兼容策略：

- 旧的 `~/.claude-code-router/claude-code-router.log` 不迁移、不删除。
- 新版本启动后只写当天 rotate 文件。
- 如果当天 rotate 文件不存在，Logs tab 显示空内容，不 fallback 读取旧固定文件，避免新旧日志混在一起。

## 非目标

- 不实现完整日志搜索。
- 不重写日志格式为 JSONL。
- 不实现历史日志日期选择器。
- 不改变 redaction 规则。
- 不解析 upstream response body 的内部 JSON escape。
- 不引入大型字体资源到仓库，除非确认系统字体不可用。

## 预计修改文件

- `ccr-rust/crates/ccr-app-core/src/logging.rs`
- `ccr-rust/crates/ccr-ui/src/logs_tab.rs`
- 可能修改 `ccr-rust/crates/ccr-ui/src/app.rs` 或 `main.rs`，如果需要注册 CJK fallback 字体。

## 规划的测试用例

- `format_app_log_line()` 对中文字段保留原始中文。
- `format_app_log_line()` 仍 escape 双引号和反斜杠。
- `format_app_log_line()` 输出仍是单行。
- `truncate_for_log()` 不截断坏 UTF-8，不破坏中文字符边界。
- `app_log_path()` 指向当天 `claude-code-router-YYYY-MM-DD.log`。
- `app_log_path_for_date()` 生成稳定的按天日志文件名。
- `append_app_log_line_to_path()` 能写入中文。
- `LogsTab::load_from_path()` 能读取并保留中文内容。
- `Clear` 清空当天 rotate 日志，不影响旧固定日志或其他日期日志。
- 如果增加字体配置，字体注册缺少系统字体时不 panic。
- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- 新写入的 app log 字段值中中文直接可读，不显示为 `\uXXXX`。
- Logs tab 读取中文日志后显示中文。
- app log 写入当天 rotate 文件，例如 `claude-code-router-2026-05-01.log`。
- Logs tab 默认读取当天 rotate 文件。
- 日志仍保持一行一条记录。
- quote、空格、换行、反斜杠处理不回退。
- API key redaction 不回退。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 未增加 CJK fallback 字体注册。排查后本次实际问题在 app log formatter 把字段值用 JSON string 方式转义，修复 formatter 后日志文件本身保留中文，Logs tab 的现有读取和展示路径可以显示中文。
- `Clear` 保持清空当天 app log 文件；旧固定日志文件和其他日期日志不会被迁移或清理。

## 实际修改

- `ccr-app-core/src/logging.rs`
  - app log 改为写入 `claude-code-router-YYYY-MM-DD.log`。
  - 新增按日期生成日志文件名/路径的 helper。
  - 自定义字段 quoting，保留中文和其他非 ASCII 字符，只 escape 必要控制字符、引号和反斜杠。
  - 增加中文保留、escape、UTF-8 截断边界、按天文件名测试。
- `ccr-ui/src/logs_tab.rs`
  - 日志读取测试覆盖中文内容。
- `ccr-server/src/main.rs`
  - `/api/logs` 的读取和清空改为使用当天 app log 路径。

## 验证结果

- `cargo test --workspace` 通过。
- `cargo build --bin ccr-server --bin ccr-ui` 通过。
- `cargo llvm-cov --workspace --lib --summary-only` 通过，行覆盖率 `82.33%`。
