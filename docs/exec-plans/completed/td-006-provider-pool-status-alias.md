# TD-006 - Provider Pool Status Alias Decision

**状态：** resolved
**优先级：** P0
**关联技术债：** TD-006
**最后更新：** 2026-05-07

## 目标

解决 `/api/provider-pool/status` 与 Route Pool-only 架构方向的命名冲突。

必须在两个选项中明确选择一个：

- 删除 `/api/provider-pool/status`。
- 保留为 compatibility alias，并在代码、测试和文档中明确它不是 runtime ProviderPool 模型。

## 非目标

- 不重新引入 ProviderPool。
- 不改变 Route Pool runtime 行为。
- 不修改 UI 的主路由模型。

## 背景

Phase 68 已要求删除 ProviderPool runtime 模型，但 server 仍存在 `/api/provider-pool/status`，并映射到 Route Pool 状态。这会误导后续智能体和维护者。

## 设计方向

优先建议：删除 alias。

删除路径：

- 移除 server route。
- 搜索 UI 和 docs 是否仍引用 `/api/provider-pool/status`。
- 如果无外部兼容需求，直接删除测试中对 alias 的依赖。

兼容保留路径：

- 在 route 注册附近加注释。
- 新增测试证明 `/api/provider-pool/status` 和 `/api/route-pool/status` 返回同一结构。
- 在 `ARCHITECTURE.md` 和 `docs/long-term-roadmap.md` 标记为 compatibility alias。
- 文档明确禁止围绕 provider-pool 扩展新行为。

## 修改文件

- `crates/ccr-server/src/main.rs`
- `crates/ccr-server/src/lib.rs` 或相关 tests
- `ARCHITECTURE.md`
- `docs/long-term-roadmap.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

如果删除 alias：

```sh
rg -n 'provider-pool|ProviderPool' crates docs AGENTS.md ARCHITECTURE.md
cargo test --package ccr-server
cargo test --workspace
```

如果保留 alias：

```sh
cargo test --package ccr-server
rg -n 'provider-pool|ProviderPool' crates docs AGENTS.md ARCHITECTURE.md
```

并确认搜索结果都标明 compatibility alias 或历史记录。

## 决策日志

- 2026-05-07：倾向删除 alias，除非明确存在外部调用兼容要求。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划保留了“删除 alias”或“保留 compatibility alias”两个选项；实际选择删除 `/api/provider-pool/status`。
- 未增加 compatibility alias 测试，因为最终没有保留该 API。
- 文档中仍可能在历史 legacy phase 或该完成计划自身提到 ProviderPool；这些都是历史语境，不代表当前 runtime 模型。
- 当前有效 API 是 `/api/route-pool/status`。
