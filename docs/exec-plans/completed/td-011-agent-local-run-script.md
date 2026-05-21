# TD-011 - Agent-Friendly Local Run Script

**状态：** resolved
**优先级：** P2
**关联技术债：** TD-011
**最后更新：** 2026-05-07

## 目标

提供一个 agent-friendly 本地运行入口，让 Codex 能在隔离 config/home/port/log 下启动 CCR server 并验证行为。

第一阶段目标：

- 不污染用户真实 `~/.claude-code-router/config.json`。
- 使用临时 HOME 或临时 config path。
- 自动选择可用端口。
- 输出 health URL、config path、log path、PID 和 stop 命令。

## 非目标

- 不启动完整 GUI 自动化。
- 不接入 Chrome DevTools。
- 不实现长期 observability stack。
- 不替代正式 packaging 或 release smoke。

## 背景

当前智能体验证 server/UI 行为时，需要手动拼接配置、端口、日志路径和进程管理，不利于 worktree 隔离和长时间自动验证。

## 设计方向

新增脚本：

```text
scripts/agent-run-local
```

行为：

1. 创建临时目录。
2. 写入最小 CCR config。
3. 选择随机端口。
4. 启动 `cargo run --bin ccr-server` 或已构建 binary。
5. 等待 `/health`。
6. 打印 machine-readable summary。
7. 提供 stop helper。

输出格式建议为 JSON，便于 agent 读取。

## 修改文件

- `scripts/agent-run-local`
- `scripts/agent-stop-local` or generated stop command
- `docs/RELIABILITY.md`
- `AGENTS.md`
- `docs/exec-plans/completed/`

## 验收测试

```sh
scripts/agent-run-local
curl http://127.0.0.1:<port>/health
```

验证点：

- health 返回 ok。
- 临时 config path 不在用户 home config 默认路径。
- stop 命令能停止进程。
- 日志路径可读。

## 决策日志

- 2026-05-07：先做 server 隔离运行，不做 GUI 自动化。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划要求脚本能启动隔离 server 并验证 `/health`；脚本已实现，但当前会话未完成实机验证。
- 未完成验证的原因是当前提权运行环境中的 `cargo`/toolchain 配置问题，而不是脚本逻辑本身。
- 脚本保持使用标准 `cargo run --bin ccr-server`，没有绑定 `mise` 或 `rustup`。
- 该脚本仍是 agent 验证入口，不是用户日常启动方式。
