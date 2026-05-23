# Execution Plans Index

**状态：** 长期索引文档
**最后验证：** 2026-05-21

This directory is the canonical home for execution plans and completion records.

It follows the article-style layout:

```text
docs/exec-plans/
  index.md
  active/
  completed/
```

## Current Entry Points

- `active/` - active execution plans.
- `completed/` - completed execution plans.
- `../long-term-roadmap.md` - long-term direction and priority buckets.
- `../cc-switch-feature-gap-analysis.md` - gap tracking against `cc-switch`.
- `../ccr-original-feature-gap-analysis.md` - gap tracking against original CCR.

## Legacy Plans

Historical top-level phase files were migrated to `docs/exec-plans/completed/legacy-phase-*.md`.

Treat those files as historical records unless a current doc explicitly references them as active. They may contain old names such as failover or ProviderPool because they record past implementation phases.

New complex work should use `docs/exec-plans/active/`.

## New Plan Template

Use this outline for complex new work. The normal plan sections are the
`Plan` part of PDCA; do not duplicate them under a separate `Plan` heading.
`Do`, `Check` and `Act` are updated while work progresses and before archival.

```md
# plan-001 - Title

**状态：** active
**优先级：** P0/P1/P2
**计划编号：** plan-001
**最后更新：** YYYY-MM-DD

## 目标

## 非目标

## 背景

## 设计方向

## 修改文件

## 验收测试

## Do / 执行记录

- 实际修改:
- 实际偏离计划:
- 中途决策:

## Check / 验证与偏差

- 验证命令:
- 手工 QA:
- 发现的偏差:
- 代码和文档不一致:

## Act / 处理与沉淀

- 已处理偏差:
- 长期文档更新:
- 新增/更新技术债:
- 后续计划:

## 决策日志

## 完成记录
```

## Lifecycle

1. Create a plan under `active/`.
2. Use `plan-001`, `plan-002` style IDs for all new workstreams.
3. Treat `目标`、`非目标`、`背景`、`设计方向`、`修改文件` and `验收测试`
   as the `Plan` part of PDCA.
4. During implementation, update `Do / 执行记录` with actual changes, plan
   deviations and implementation decisions.
5. Before completion, update `Check / 验证与偏差` with verification commands,
   manual QA, test results and discovered code/doc mismatches.
6. Before moving to `completed/`, update `Act / 处理与沉淀` with how deviations
   were handled, what long-term docs changed, and what debt or follow-up plans
   remain.
7. Link related predecessor plans or roadmap sections when useful.
8. Keep progress and decisions in the plan, not only in chat history.
9. Move the plan to `completed/` when implemented and verified.
10. Preserve completion evidence in the completed plan.

## Mechanical Checks

`./scripts/check-docs-structure.sh` enforces the execution-plan directory
shape, active plan naming, active README consistency and the repository rule
that generated notes or workpad artifacts are not canonical active plans.
