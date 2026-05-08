# Execution Plans Index

**状态：** 长期索引文档
**最后验证：** 2026-05-08

This directory is the canonical home for execution plans and technical debt tracking.

It follows the article-style layout:

```text
docs/exec-plans/
  index.md
  tech-debt-tracker.md
  active/
  completed/
```

## Current Entry Points

- `tech-debt-tracker.md` - known technical debt, documentation debt and validation debt.
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

Use this PDCA outline for complex new work:

```md
# plan-001 - Title

**状态：** active
**优先级：** P0/P1/P2
**计划编号：** plan-001
**最后更新：** YYYY-MM-DD

## 目标

## PDCA

### Plan

- Problem:
- Scope:
- Non-goals:
- Risks:
- Intended verification:

### Do

- Implementation steps:
- Files expected to change:

### Check

- Verification commands:
- Manual QA:
- Observed result:

### Act

- Follow-up:
- Documentation updates:
- Remaining debt:

## 非目标

## 背景

## 设计方向

## 修改文件

## 验收测试

## 决策日志

## 完成记录
```

## Lifecycle

1. Create a plan under `active/`.
2. Use `plan-001`, `plan-002` style IDs for all new workstreams.
3. Fill the PDCA section before implementation starts. At minimum, `Plan` must
   state the problem, scope, non-goals, risks and intended verification.
4. During implementation, update `Do` with actual steps and changed files.
5. Before completion, update `Check` with verification commands, manual QA and
   observed results.
6. Before moving to `completed/`, update `Act` with follow-up decisions,
   documentation changes and remaining debt.
7. Link it from `tech-debt-tracker.md` when it resolves or advances a debt item.
8. Keep progress and decisions in the plan, not only in chat history.
9. Move the plan to `completed/` when implemented and verified.
10. Update `tech-debt-tracker.md` with completion evidence.
