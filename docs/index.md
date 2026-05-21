# Documentation Index

**状态：** 长期索引文档
**最后验证：** 2026-05-17

This index is the main entry point for repository knowledge. Prefer reading the smallest relevant document instead of scanning every file.

## Start Here

- `../AGENTS.md` - agent workflow and repository map.
- `../ARCHITECTURE.md` - crate boundaries and runtime paths.
- `long-term-roadmap.md` - long-term product and engineering direction.
- `exec-plans/index.md` - execution plan workflow and active/completed structure.

## Product And Design

- `DESIGN.md` - product design principles for the desktop app.
- `FRONTEND.md` - UI implementation and egui interaction guidance.
- `PRODUCT_SENSE.md` - product judgment and user workflow priorities.
- `client-model-mapping.md` - client-visible model mapping versus upstream Route Pool routes.
- `provider-api-kinds.md` - Anthropic Messages, OpenAI Chat Completions and OpenAI Responses provider protocol boundaries.

## Quality And Operations

- `QUALITY_SCORE.md` - current quality rubric and scorecard.
- `RELIABILITY.md` - reliability goals, failure modes and observability expectations.
- `SECURITY.md` - local security model, secrets handling and config safety.
- `provider-runtime-metrics.md` - long-term provider/request metrics design.
- `release.md` - packaging and release smoke checklist.

## Gap Analysis

- `cc-switch-feature-gap-analysis.md` - gap tracking against `cc-switch`.
- `ccr-original-feature-gap-analysis.md` - gap tracking against original CCR behavior.

## Documentation Rules

- Long-term docs should include status and last verification date.
- If a document references source paths, those paths should be current or explicitly marked historical.
- Completed phase docs may retain historical terminology.
- Current long-term docs must not describe removed runtime behavior as supported.
- New complex work should reference a roadmap section, gap document or existing execution plan and create an active execution plan.
