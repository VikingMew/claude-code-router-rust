# TD-015 - Remove Top-Level Plans Directory

**状态：** resolved
**优先级：** P1
**关联技术债：** TD-015
**依赖：** TD-013, TD-014
**最后更新：** 2026-05-07

## 目标

删除顶层 `plans/` 目录中的历史 phase 文件，并移除所有把 `plans/` 当作当前计划系统的引用。

完成后，canonical planning layout 只有：

```text
docs/exec-plans/
  index.md
  completed/
  active/
  completed/
```

## 非目标

- 不删除 `docs/exec-plans/`。
- 不删除已迁移到 completed 的历史证据。
- 不改 runtime 行为。

## 背景

文章建议把 execution plans 放在文档知识库中，使用 active/completed 分层。当前仓库已经建立 `docs/exec-plans/`，但顶层 `plans/` 仍存在，会让智能体误判新计划入口。

## 设计方向

删除顺序：

1. 确认 TD-013 inventory 完成。
2. 确认 TD-014 migration 完成。
3. 搜索所有 `plans/` 引用。
4. 将当前计划引用改到 `docs/exec-plans/`。
5. 将历史说明改为 `docs/exec-plans/completed/` 或 inventory。
6. 删除顶层 `plans/` 文件。
7. 如果目录为空，删除目录。

## 修改文件

- 删除：`plans/phase-*.md`
- 删除：`plans/index.md`
- 更新：
  - `AGENTS.md`
  - `docs/index.md`
  - `docs/long-term-roadmap.md`
  - `docs/exec-plans/index.md`
  - `docs/exec-plans/completed/`
  - any docs referencing top-level `plans/`

## 验收测试

```sh
test ! -d plans
rg -n '(^|[^A-Za-z0-9_/.-])plans/' AGENTS.md ARCHITECTURE.md docs crates packaging -g '*.md' -g '*.rs' -g '*.toml' -g '*.sh' -g '*.ps1'
find docs/exec-plans -maxdepth 2 -type f -print | sort
```

允许的残留：

- `docs/exec-plans/legacy-plans-inventory.md` 中明确标记的原始历史路径。
- completed execution plans 中的 `原始路径` 字段。

## 决策日志

- 2026-05-07：最终目标是删除顶层 `plans/`，避免双计划系统。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划要求删除顶层 `plans/` 并清理当前计划系统引用；实际已删除目录并将历史 phase 移入 `docs/exec-plans/completed/`。
- 仍允许在 inventory 和 migrated legacy 文档中出现历史 `plans/phase-*.md` 路径作为追溯信息。
- 没有删除 legacy 文档内部对旧路径的历史引用。
- 当前 canonical execution plan 系统只保留在 `docs/exec-plans/`。
