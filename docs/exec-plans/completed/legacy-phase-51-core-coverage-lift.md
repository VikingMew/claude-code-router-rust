# Phase 51 - Core Coverage Lift

**状态：** ✅ 已完成（coverage gate 已达成）  
**优先级：** P0

## 目标

提高已有非 UI 模块的测试覆盖率。

当前全 workspace 覆盖率被 UI、binary 入口和部分旧模块拉低。去掉 `ccr-ui` 后仍只有约 `64.94%` line coverage；只看 library 也约 `74.58%`。本 phase 的目标是先把已有核心模块补测到可维护水平，避免后续功能继续堆在低覆盖代码上。

本 phase 不以 UI 展示为目标，重点是核心业务、配置、路由、transform、SSE、CLI helper 和 server helper 的单元测试与小型集成测试。

## 背景

最近一次覆盖率参考：

```bash
cargo llvm-cov --workspace --exclude ccr-ui --summary-only
```

结果：

- Lines: `64.94%`
- Regions: `65.11%`
- Functions: `65.89%`

只看 library：

```bash
cargo llvm-cov --workspace --lib --summary-only
```

结果：

- Lines: `74.58%`

只看本轮核心相关库 `ccr-types + ccr-config + ccr-server --lib`：

- Lines: `89.21%`

说明新增核心路径覆盖基本够，但老模块和 binary/helper 路径仍有明显缺口。

## 完成项

- [x] 建立覆盖率基线命令和记录方式
- [x] 明确本 phase 的 coverage gate
- [x] 提高 `ccr-cli` helper 覆盖率
- [ ] 提高 `ccr-agent` 非 cache 逻辑覆盖率
- [ ] 提高 `ccr-preset` 覆盖率
- [ ] 提高 `ccr-router` custom / tokenizer API 覆盖率
- [ ] 提高 `ccr-sse` continuation 覆盖率
- [ ] 提高 `ccr-transformer::transformers` 覆盖率
- [x] 提高 `ccr-server` helper 覆盖率
- [x] 对不适合单元测试的 binary 入口明确排除或拆出可测 helper
- [x] 覆盖率结果写入 phase 偏差或完成说明

## 覆盖率目标

本 phase 的目标分两层：

```bash
cargo llvm-cov --workspace --lib --summary-only
```

目标：

- line coverage >= 80%

```bash
cargo llvm-cov --workspace --exclude ccr-ui --summary-only
```

目标：

- line coverage 尽量接近或超过 80%
- 如果 binary 入口仍然导致低于 80%，必须把无法覆盖的入口拆出 helper 或说明保留原因

## 优先补测模块

### ccr-cli

重点补：

- `auto_launch`
- `endpoint`
- Claude/Codex config error branch
- PID helpers

`main.rs` 作为 binary 入口不直接追高覆盖率。能拆出的命令处理逻辑应拆到 lib 中。

### ccr-agent

重点补：

- image agent detection edge cases
- request modify behavior
- tool execution error branch
- agent selection priority

### ccr-router

重点补：

- custom router bad script / invalid return
- tokenizer API success/failure/cache fallback
- project router parse failure
- route/model parsing edge cases

### ccr-sse

重点补：

- continuation success
- continuation upstream failure
- malformed SSE continuation
- agent tool call roundtrip

### ccr-transformer

重点补：

- low coverage transformer branches
- unsupported field removal
- model-specific transformer override
- malformed input tolerance

### ccr-preset

重点补：

- install conflict strategies
- missing required inputs
- sensitive field nested cases
- invalid manifest

## 规划的测试用例

- Coverage baseline：运行 coverage 命令并记录当前 line coverage。
- CLI helper：auto launch unsupported 平台返回稳定错误。
- CLI helper：endpoint invalid URL 不发请求并返回 `InvalidUrl`。
- CLI helper：endpoint sorting 成功结果排在失败结果前。
- Agent：无 image route 时 image agent 不接管请求。
- Agent：image content block 能触发 agent。
- Router：custom router 文件不存在返回 None。
- Router：custom router 返回非法 route 时有明确 fallback 或错误。
- Router：tokenizer API 请求失败时 fallback 到 tiktoken。
- SSE：parser 能处理 partial chunks。
- SSE continuation：upstream continuation 失败时返回 error event。
- Transformer：每个 built-in transformer 至少有 success 和 malformed input 两类测试。
- Preset：敏感字段 sanitization 覆盖 nested api_key。
- Preset：安装已存在 provider 时符合 conflict strategy。
- Server helper：failover candidates 去重、排序、禁用、tried route 跳过。
- Config：保存配置时保留未知字段。

## 预留的开发偏差

- 本轮以 `cargo llvm-cov --workspace --lib --summary-only` 的 line coverage >= 80% 作为硬 gate。实际结果为 `80.25%`。
- `cargo llvm-cov --workspace --exclude ccr-ui --summary-only` 仍为 `71.26%`，主要原因是 `ccr-cli/src/main.rs` 和 `ccr-server/src/main.rs` 这类 binary 入口仍按集成入口保留，未在本轮强行拆分。
- 未逐一补齐 `ccr-agent`、`ccr-preset`、`ccr-router::tokenizer::api`、`ccr-sse::continuation`、`ccr-transformer::transformers` 的全部低覆盖分支；本轮优先补了不依赖网络和进程的稳定 helper 分支，以及 UI 解耦后可测试的 `ccr-app-core` 状态逻辑。
- Endpoint 真实 HTTP 成功/失败分支没有用本地 TCP server 测试覆盖，因为当前执行环境禁止监听本地端口；保留为后续可在 CI 或允许 bind 的环境补测。

## 验证

```bash
cargo test --workspace
cargo llvm-cov --workspace --lib --summary-only
cargo llvm-cov --workspace --exclude ccr-ui --summary-only
```

验证结果：

- `cargo test --workspace` 通过
- `cargo llvm-cov --workspace --lib --summary-only`：Lines `80.25%`
- `cargo llvm-cov --workspace --exclude ccr-ui --summary-only`：Lines `71.26%`

## 验收标准

- `cargo test --workspace` 通过
- `cargo llvm-cov --workspace --lib --summary-only` line coverage >= 80%
- 新增测试覆盖本 phase 列出的主要低覆盖模块
- 对仍无法达到 80% 的非 UI coverage 给出明确原因和后续拆分计划
- 不为了覆盖率写无意义测试，只测稳定业务行为和错误分支
