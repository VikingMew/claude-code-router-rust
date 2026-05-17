# plan-013 - Logs Tab Progressive Reverse Loading

**状态：** completed
**优先级：** P1
**计划编号：** plan-013
**最后更新：** 2026-05-17

## 目标

切换到 Logs tab 时不再因为同步读取和渲染完整日志文件而卡住 UI。

本计划要完成：

- Logs tab 首屏只加载最新日志片段，并按倒序展示最新日志在上方。
- 日志读取做成渐进式载入，用户向下查看更多时再加载更早的日志。
- 自动刷新只追加或重读必要范围，不在 UI 线程反复整文件 `read_to_string`。
- 过滤时仍保持 UI 可响应；过滤结果按倒序展示，并限制单次展示数量。
- Clear、auto refresh、中文日志展示和现有 target/event filter 行为继续可用。

## 非目标

- 不重做 Logs tab 的完整信息架构。
- 不引入外部日志数据库或持久化索引。
- 不改变 server/app log 写入格式。
- 不改变 `/api/logs` 或 `/api/logs/query` 的语义，除非 UI 需要一个很小的内部 helper。
- 不在本计划内实现全字段高级查询 UI。

## 背景

当前 `crates/ccr-ui/src/logs_tab.rs` 在 Logs tab 获得焦点时调用 `refresh_now()`，同步执行：

- `std::fs::read_to_string(app_log_path())` 读取完整日志文件。
- 把完整内容放入 `self.content`。
- UI 每次展示时通过 `filtered_content()` 生成完整过滤字符串。
- `TextEdit::multiline` 渲染完整文本。

当日志文件包含长请求 body、长 prompt 或大量 upstream events 时，切换到 Logs tab 会在 UI 线程做大量文件 IO、字符串分配、过滤和文本布局，因此明显卡顿。

日志查看的真实使用方式更接近 tail：先看最新事件，再按需追溯更早内容。因此默认顺序应该倒序，且读取和展示都应该有上限。

## 设计方向

### 1. 数据模型

把 Logs tab 从“一个完整 content 字符串”改成“可增量展示的行缓冲”：

- `visible_lines: Vec<String>`：当前展示行，最新日志在前。
- `loaded_bytes_from_end: u64`：从文件末尾向前已加载的字节范围。
- `snapshot: LogFileSnapshot`：文件长度和修改时间。
- `loading_state`：idle/loading/error。
- `has_more_older: bool`：是否还能继续加载更早日志。

第一版可以仍在 UI 线程读取小块文件，但单次读取必须有明确字节上限，例如 128 KiB 或 256 KiB。更稳妥的实现是用后台线程读取 chunk，然后 UI 轮询结果；若实现成本不高，优先后台读取。

### 2. 倒序和渐进式载入

首屏加载：

- focus Logs tab 时只从文件末尾读取一个 chunk。
- 按行切分，倒序展示。
- 如果最后一行没有换行，要正确保留。
- 对超长单行做显示截断或上限保护，避免单个 request body 卡住文本布局。

继续加载：

- UI 底部提供 `Load older` 按钮，或当滚动接近底部时触发。
- 每次向文件更早位置读取下一个 chunk。
- 新加载的旧行追加到 `visible_lines` 末尾，保持整体“新到旧”。

自动刷新：

- 如果文件长度增加，只读取新增尾部范围。
- 新行插入 `visible_lines` 前端。
- 如果文件被 Clear 或 rotate 后长度变小，重置加载状态并重新加载尾部 chunk。

### 3. 过滤

过滤不应对完整历史日志做无界扫描。

第一版过滤范围：

- 对当前已加载 `visible_lines` 做 target/event 过滤。
- UI 文案或状态显示 `Filtering loaded log slice`，避免暗示已经搜索全量历史。
- 如果需要全量搜索，后续再增加专门的 query action。

过滤结果：

- 保持倒序。
- 限制展示行数，例如最多 500 行。
- filter 输入变化不触发整文件读取。

### 4. UI 行为

推荐 UI：

- 顶部保留 Target / Event / Clear。
- 增加轻量状态：`Showing latest N lines`、`Loading older...`、`File changed, refreshed latest slice`。
- 日志内容用 `ScrollArea` + `Label` 或行级渲染，避免把巨大字符串塞进 `TextEdit::multiline`。
- 每一行 monospace、可选择复制整行；如果 egui 当前复制能力不足，先保留可读展示。

### 5. 可测试 helper

把纯逻辑拆成小函数，方便单元测试：

- 从尾部 chunk 读取并按行倒序。
- 合并新增尾部日志到倒序行缓冲。
- 加载更早 chunk 并追加旧行。
- 过滤当前 loaded lines。
- 处理 truncation 和 rotate/clear。

## 修改文件

预计修改：

- `crates/ccr-ui/src/logs_tab.rs`
- `crates/ccr-app-core/src/logging.rs`，仅当需要共享 chunk/query helper 时修改
- `docs/exec-plans/active/plan-013-logs-tab-progressive-reverse-loading.md`

## 验收测试

代码检查：

```sh
cargo fmt --check
cargo test --package ccr-ui
cargo test --workspace
```

预期覆盖测试用例：

- Logs tab 初始创建不读取日志文件。
- focus Logs tab 只加载最新 chunk，不读取完整大文件。
- 最新日志倒序展示在最上方。
- `Load older` 会追加更早日志到列表末尾。
- 文件追加新行后，auto refresh 只把新增行插入顶部。
- 文件被 clear 或 rotate 后，加载状态重置且不 panic。
- target/event filter 只过滤当前 loaded lines，并保持倒序。
- 超长单行不会让展示字符串无界增长。
- 中文日志行在 chunk 边界附近不会被破坏或 panic。

手工 QA：

- 造一个包含大量 upstream body 的大日志文件，切换 Logs tab 不出现明显卡顿。
- 最新日志默认出现在顶部。
- 连续点击 `Load older` 可以逐步看到更早日志。
- Clear 后 UI 立即变空，新日志继续出现在顶部。

## Do / 执行记录

- 实际修改:
  - `LogsTab` 从完整 `content: String` 改为 `visible_lines: Vec<String>` 倒序行缓冲。
  - 首次进入 Logs tab 时只从日志文件尾部读取最多 256 KiB。
  - 新日志追加时只读取新增字节范围，并把新行插入顶部。
  - 增加 `Load older`，按 256 KiB chunk 向前加载更早日志并追加到底部。
  - UI 从 `TextEdit::multiline` 整体文本布局改为 `ScrollArea` 中逐行 monospace 渲染。
  - target/event filter 改为只过滤已加载行，并保持倒序。
  - 自动刷新路径保留最多 2000 行，用户主动 `Load older` 可以继续扩展可见历史。
  - 单行展示增加 4000 字符截断，避免巨大 request body 造成布局卡顿。
  - 新增 chunk 读取、倒序展示、追加刷新、加载旧日志、过滤和超长行截断测试。
- 实际偏离计划:
  - 未引入后台线程；第一版使用小块同步读取，单次 IO 有 256 KiB 上限。
  - 未实现滚动接近底部自动加载旧日志；第一版使用明确的 `Load older` 按钮。
  - 未实现行级复制控件；继续优先保证可读和不卡顿。
- 中途决策:
  - 保持日志查询范围为当前已加载 slice，避免 filter 输入触发全量文件扫描。
  - 文件增长时按旧长度到新长度读取新增范围；文件清空、rotate 或长度回退时重置为最新 chunk。

## Check / 验证与偏差

- 验证命令:
  - `cargo fmt --check`
  - `./scripts/check-docs-structure.sh`
  - `cargo test --package ccr-ui`
  - `cargo test --workspace`
- 手工 QA:
  - 未启动桌面 UI 做截图验证；本次验证以单元测试和 workspace 测试为主。
- 发现的偏差:
  - 现实现仍在 UI 线程读取 chunk，但读取量有硬上限；如果未来日志文件所在磁盘很慢，仍可继续演进为后台线程。
  - filter 状态没有额外显示 “Filtering loaded log slice” 文案，只通过状态行说明当前显示 loaded lines。
- 代码和文档不一致:
  - 无新增不一致。

## Act / 处理与沉淀

- 已处理偏差:
  - 用 `Load older` 明确表达渐进式加载边界。
  - 用 chunk 大小、自动刷新行数上限和单行截断约束 UI 线程工作量。
- 长期文档更新:
  - 本次是局部 UI 性能修复，未更新长期文档。
- 新增/更新技术债:
  - 未新增技术债。后台线程加载和滚动触底自动加载可作为后续体验增强，不阻塞本计划目标。
- 后续计划:
  - 如大日志在慢磁盘上仍有可感知卡顿，再新增计划把 chunk IO 移到后台 worker，并增加加载状态 channel。

## 决策日志

- 2026-05-17：创建计划。把 Logs tab 从完整日志文本加载改成倒序、渐进式、有限窗口加载，优先解决切 tab 卡顿。

## 完成记录

完成：2026-05-17。

- Logs tab 首屏改为读取最新 256 KiB，最新日志倒序展示在顶部。
- 自动刷新只读取新增尾部范围；文件 clear/rotate 后重新加载最新 chunk。
- `Load older` 渐进式加载更早日志。
- 自动刷新最多保留 2000 行，用户主动 `Load older` 可继续扩展；过滤最多展示 500 行，单行最多 4000 字符。
- 保留 Target/Event/Clear 行为。

验证：

```sh
cargo fmt --check
./scripts/check-docs-structure.sh
cargo test --package ccr-ui
cargo test --workspace
```

## 完成偏差

- 未实现后台线程读取；当前通过 256 KiB chunk 限制把同步 IO 控制在小范围内。
- 未实现滚动触底自动加载；使用 `Load older` 按钮作为第一版渐进式载入入口。
- 未增加全量历史搜索；过滤只作用于当前 loaded slice。
