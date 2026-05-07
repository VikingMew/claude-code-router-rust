# TD-003 - CI And Mechanical Checks

**状态：** resolved
**优先级：** P0
**关联技术债：** TD-003
**最后更新：** 2026-05-07

## 目标

建立最小 CI 和机械校验入口，让仓库的格式、测试和文档结构能在 PR 上被自动验证。

第一阶段必须覆盖：

- Rust formatting。
- workspace tests。
- execution plan 目录结构检查。
- 长期文档入口存在性检查。

## 非目标

- 不在本阶段追求完整 release CI。
- 不要求覆盖所有平台打包。
- 不要求立刻加入 coverage gate。
- 不要求实现 doc-gardening agent。

## 背景

当前仓库没有 `.github/workflows`。格式化、测试、文档链接和计划状态都依赖人工本地执行，不符合 agent-readable record system 的要求。

## 设计方向

新增基础 GitHub Actions workflow：

- 触发：pull request 和 push。
- 运行环境：优先 Ubuntu latest。
- 步骤：
  - checkout。
  - install stable Rust。
  - `cargo fmt --check`。
  - `cargo test --workspace`。
  - shell 脚本检查关键文档路径存在。

文档结构检查可以先用简单 shell test，不引入额外依赖：

- `AGENTS.md`
- `ARCHITECTURE.md`
- `docs/index.md`
- `docs/exec-plans/index.md`
- `docs/exec-plans/tech-debt-tracker.md`
- `docs/exec-plans/active`
- `docs/exec-plans/completed`

## 修改文件

- `.github/workflows/ci.yml`
- 可选：`scripts/check-docs-structure.sh`
- `docs/QUALITY_SCORE.md`
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

本地验证：

```sh
cargo fmt --check
cargo test --workspace
test -f AGENTS.md
test -f ARCHITECTURE.md
test -f docs/index.md
test -f docs/exec-plans/index.md
test -f docs/exec-plans/tech-debt-tracker.md
test -d docs/exec-plans/active
test -d docs/exec-plans/completed
```

PR 验证：

- CI workflow 能在 PR 上运行。
- fmt 或 test 失败时 PR 显示失败。

## 决策日志

- 2026-05-07：第一阶段先做最小 CI，不引入 coverage、clippy 或外部 doc checker，降低启用成本。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划提到可选 `scripts/check-docs-structure.sh`；实际已实现该脚本并接入 CI。
- CI 只覆盖最小闭环：文档结构、`cargo fmt --check`、`cargo test --workspace`。
- 未加入 clippy、coverage gate、dead link checker 或定时 doc-gardening；这些仍属于后续增强。
- CI workflow 已写入仓库，但尚未在远端 GitHub PR 环境中实际跑过。
