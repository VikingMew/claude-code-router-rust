# TD-007 - Structured Log Query

**状态：** resolved
**优先级：** P1
**关联技术债：** TD-007
**最后更新：** 2026-05-07

## 目标

让 app log 从“整文件文本读取”演进到可过滤、可查询、agent-readable 的诊断接口。

第一阶段目标：

- 保留现有文本日志和 Logs UI。
- 新增结构化查询 API。
- 支持按 target、event、provider、route、limit 过滤。
- 保持敏感 header 和 API key 脱敏。

## 非目标

- 不在本阶段引入外部 LogQL backend。
- 不要求迁移全部历史日志格式。
- 不要求替换 tracing rolling logs。
- 不实现完整 runtime metrics store。

## 背景

当前 `/api/logs` 返回完整文本日志。Codex 和 UI 只能读取整文件或做字符串过滤，不利于复现故障、查询 upstream attempts 或 Route Pool 事件。

## 设计方向

采用兼容式增量设计：

1. 保留当前 text app log。
2. 增加 parser，把现有 `key="value"` 单行日志解析为结构化 event。
3. 新增 `/api/logs/query`。
4. Query 参数支持：
   - `target`
   - `event`
   - `provider`
   - `route`
   - `limit`
5. 返回 JSON array。

如果解析失败，该行可以作为 raw fallback 返回，避免丢诊断信息。

## 修改文件

- `crates/ccr-app-core/src/logging.rs`
- `crates/ccr-server/src/main.rs`
- `crates/ccr-server/src/handlers.rs` 或新增 handlers module
- `crates/ccr-ui/src/logs_tab.rs`
- `docs/RELIABILITY.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

```sh
cargo test --package ccr-app-core --lib
cargo test --package ccr-server
cargo test --package ccr-ui
cargo test --workspace
```

测试覆盖：

- 日志 parser 能解析 target、event 和 key/value fields。
- query filter 按 target/event/provider/route 生效。
- limit 生效。
- authorization header 和 x-api-key 不泄漏。
- parser 遇到 malformed line 不 panic。

## 决策日志

- 2026-05-07：第一阶段选择解析现有文本格式，避免同时改写日志 sink 和 UI。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划只要求新增结构化查询 API；实际还给 Logs tab 增加了本地 target/event 过滤。
- 后端 `/api/logs/query` 已实现，但 UI 仍读取本地日志文件并在内存中过滤，没有改为调用 server query API。
- 第一阶段选择解析现有文本日志格式，没有引入 JSONL sink。
- 未实现 time range、provider、route 的 UI 控件；这些只在 query API 层预留。
