# TD-013 - Legacy Plans Inventory

**状态：** resolved
**优先级：** P1
**关联技术债：** TD-013
**最后更新：** 2026-05-07

## 目标

盘点顶层 `plans/` 目录中的所有历史 phase 文档，为后续迁移和删除做准备。

输出必须包括：

- 每个 `plans/phase-*.md` 的状态。
- 是否仍包含当前架构需要保留的信息。
- 是否引用已删除 runtime 概念。
- 推荐动作：migrate / summarize / delete。

## 非目标

- 不在本计划中删除文件。
- 不改代码。
- 不迁移文档内容。

## 背景

仓库已经采用 `docs/exec-plans/completed/` 和 `docs/exec-plans/completed/` 作为 canonical execution plan 系统。顶层 `plans/` 只应作为临时历史区，最终应清空或删除。

直接删除 `plans/` 有风险，因为里面仍有完成记录、验证命令和历史决策。需要先建立清单。

## 设计方向

新增迁移清单文档：

```text
docs/exec-plans/legacy-plans-inventory.md
```

清单字段：

- source path
- phase title
- status
- last known verification
- current relevance
- legacy terminology risk
- recommended action

推荐动作语义：

- `migrate`：迁到 `docs/exec-plans/completed/`。
- `summarize`：只把关键结论汇总到长期文档或 completed summary。
- `delete`：确认没有独立价值后删除。

## 修改文件

- `docs/exec-plans/legacy-plans-inventory.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

```sh
find plans -maxdepth 1 -type f -name 'phase-*.md' | sort
rg -n '^# |\\*\\*状态：\\*\\*|\\*\\*优先级：\\*\\*|cargo test|cargo fmt' plans -g 'phase-*.md'
```

验收标准：

- 每个 `plans/phase-*.md` 都出现在 inventory 中。
- 每个条目都有推荐动作。
- inventory 明确哪些文档包含旧 routing 名称。

## 决策日志

- 2026-05-07：先 inventory，避免把历史执行证据直接删除。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划只要求 inventory；实际创建了 `docs/exec-plans/legacy-plans-inventory.md` 并把所有 legacy phase 标记为 migrate。
- 未逐个做深度内容审查或摘要分类，因为后续决定保留全文迁移。
- inventory 中的原始路径是历史路径，用于追溯，不代表顶层 `plans/` 仍存在。
