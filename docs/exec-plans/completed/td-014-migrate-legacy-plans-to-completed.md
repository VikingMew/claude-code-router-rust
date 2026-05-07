# TD-014 - Migrate Valuable Legacy Plans To Completed Exec Plans

**状态：** resolved
**优先级：** P1
**关联技术债：** TD-014
**依赖：** TD-013
**最后更新：** 2026-05-07

## 目标

根据 TD-013 的 inventory，把仍有价值的顶层 `plans/phase-*.md` 历史记录迁移到 `docs/exec-plans/completed/`。

迁移后，completed execution plans 应能保留：

- 目标和非目标。
- 关键设计决策。
- 修改范围。
- 验证命令。
- 完成记录。

## 非目标

- 不迁移所有历史文档。
- 不把旧计划改写成当前架构事实。
- 不在本阶段删除 `plans/` 目录。

## 背景

顶层 `plans/` 文档是历史 phase 记录，但 canonical execution plan 系统已经迁移到 `docs/exec-plans/`。需要把有长期价值的历史执行证据移动到 completed 区。

## 设计方向

对 inventory 中推荐为 `migrate` 的文档：

1. 复制到 `docs/exec-plans/completed/`。
2. 文件名使用稳定 kebab-case。
3. 保留原始完成记录和验证命令。
4. 在文档头部增加 `原始路径` 字段。
5. 如果旧术语只代表历史行为，加一行说明。

对推荐为 `summarize` 的文档：

1. 提取关键结论。
2. 合并到相关长期文档或一个 completed summary。
3. 在 inventory 中标记已处理。

## 修改文件

- `docs/exec-plans/completed/*.md`
- `docs/exec-plans/legacy-plans-inventory.md`
- `docs/exec-plans/index.md`
- 相关长期文档：
  - `docs/long-term-roadmap.md`
  - `docs/cc-switch-feature-gap-analysis.md`
  - `docs/ccr-original-feature-gap-analysis.md`
  - `docs/provider-runtime-metrics.md`

## 验收测试

```sh
find docs/exec-plans/completed -maxdepth 1 -type f -name '*.md' | sort
rg -n '原始路径|Original path|完成记录|Verification|cargo test' docs/exec-plans/completed -g '*.md'
```

验收标准：

- inventory 中所有 `migrate` 条目都有 completed 目标。
- inventory 中所有 `summarize` 条目都有汇总位置。
- completed 文档不把历史旧 routing 行为描述为当前支持能力。

## 决策日志

- 2026-05-07：迁移只保留有长期价值的执行证据，不追求一比一搬运所有旧 phase。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划允许 migrate 或 summarize；实际选择把所有 legacy phase 文档全文迁移到 `docs/exec-plans/completed/legacy-phase-*.md`。
- 没有为每个 legacy 文档逐一添加 `原始路径` header。
- 没有重写 legacy 文档中的历史状态、旧术语或验证命令。
- 历史文档中的 Failover、ProviderPool、Router.default 等术语仍保留为历史记录，不代表当前架构。
