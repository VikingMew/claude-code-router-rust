# Phase 64 - Inline Provider Endpoint Test

**状态：** ✅ 已完成  
**优先级：** P1

## 目标

移除独立的 `Endpoint Test` tab，并把 provider endpoint 测试结果移动到每个具体 provider config box 内。

这个 phase 的核心是让测试结果跟配置对象绑定：用户在 Provider 页面第一行点击一次测试按钮，系统测试所有 provider，然后把每个 provider 的结果显示在对应 config box 里。

## 背景

当前 UI 有两个 endpoint test 入口：

- 独立 `Endpoint Test` tab：
  - `ccr-rust/crates/ccr-ui/src/endpoint_test_tab.rs`
  - `ccr-rust/crates/ccr-ui/src/app.rs`
  - `ccr-rust/crates/ccr-ui/src/main.rs`
- Provider config 页面里的集中 `Endpoint Testing` 区块：
  - `ccr-rust/crates/ccr-ui/src/config_tab.rs`
  - `ccr-rust/crates/ccr-ui/src/config_tab_new.rs`

这种设计有几个问题：

1. 测试入口和 provider 编辑上下文分离，用户需要重复选择 provider。
2. 测试结果不贴近当前 provider，多个 provider 的结果容易混在一起。
3. `Endpoint Test` tab 看起来像独立功能，但它本质上是 provider config 的验证动作。
4. Provider config 页面底部的集中 `Endpoint Testing` 区块仍然不是“每个配置 box 自己的测试”。
5. 对 UI-first 程序来说，provider 配置卡片应该能直接回答“这个 provider 当前配置能不能用”。

## 设计方向

### 1. 移除独立 Endpoint Test tab

从 UI tab 列表移除：

```text
Endpoint Test
```

对应改动：

- `Tab::EndpointTest` 删除。
- `endpoint_test_tab` 字段删除。
- `endpoint_test_tab.show(...)` 删除。
- `mod endpoint_test_tab;` 删除。
- `ccr-rust/crates/ccr-ui/src/endpoint_test_tab.rs` 删除或停止编译。

`ccr_app_core::endpoint` 保留，因为 endpoint 测试逻辑仍然需要复用。

### 2. 移除 provider config 页面里的集中测试区

Provider config 页面不再有单独的：

```text
Endpoint Testing
```

集中测试区里的 provider selector、Run Selected、Run All、结果表格都要移除或迁移。

原因：

- 这仍然要求用户离开具体 provider box。
- 结果是全局表格，不利于定位当前配置是否有效。
- 多 provider 编辑时容易误测旧配置或未保存配置。

### 3. Provider 页面第一行增加单一测试入口

Provider 页面顶部第一行增加一个全局按钮：

```text
[Test Providers]
```

行为：

- 点击后测试当前正在编辑的所有 provider。
- 不需要再选择 provider。
- 不在每个 provider box 里重复放按钮。
- 测试状态可以在顶部显示整体进度，例如 `Testing providers...`。
- 测试完成后，每个 provider 的结果写回自己的 config box。

### 4. 每个 provider box 内显示测试结果

每个 provider config box 内只显示轻量测试结果和详情，不放单独测试按钮。

建议位置：

- provider name / endpoint / api kind / model 字段之后
- transformers / advanced fields 之前或之后均可，但必须属于同一个 provider box

状态展示：

```text
Last test: Available, 725ms
Last test: Failed, HTTP 429
Last test: Invalid endpoint URL
Last test: Not tested
```

详情展示：

```text
Endpoint: ...
API kind: ...
Mode: ...
Model: ...
HTTP status: ...
Latency: ...
Note: ...
```

详情展示：

注意：

- API key 必须 redacted。
- 详情可以折叠，默认不展开。
- 不要把完整响应铺满 provider card；长响应应截断或可滚动。

### 5. 测试使用当前编辑中的 provider 数据

`Test Providers` 必须使用当前 UI 中正在编辑的 provider 数据，而不是只用已保存到磁盘的 config。

原因：

- 用户编辑 endpoint/API kind/API key 后，最自然的动作是立刻测试当前输入。
- 如果测试的是磁盘旧配置，UI 会给出误导性结果。

行为要求：

- 当前 provider name 为空时，按钮禁用或显示明确错误。
- 当前 endpoint 为空时，按钮禁用或显示明确错误。
- API key 为空时允许测试，但错误需要来自 provider，而不是 UI panic。
- API kind 使用当前 provider 的 explicit/inferred 选择结果。

### 6. 日志行为

现有 endpoint test diagnostic logging 必须保留。

日志事件需要从 tab 语义改成 provider-config 语义：

```text
[ui] event="providers_test_clicked" providers="<count>"
[endpoint-test] event="start" provider="<name>" ...
[endpoint-test] event="result" provider="<name>" ...
[ui] event="providers_test_completed" results="<count>"
```

不再使用：

```text
tab="endpoint-test"
run_selected_clicked
run_selected_completed
```

或者保留兼容字段但不要让新 UI 依赖它们。

### 7. 状态管理

测试结果需要按 provider 绑定，但测试入口只有一个。

建议 UI state 使用稳定 key：

- provider name 非空时使用 provider name。
- provider name 为空或重复时使用 provider index。

需要处理：

- provider 重命名后结果迁移或清空。
- provider 删除后清理对应测试结果。
- provider 上移/下移后结果仍跟随 provider，而不是跟随旧 index 错位。

如果当前 provider name 可以重复，结果 key 需要用内部 UI id，不能只依赖 name。

### 8. 同步/异步策略

当前 Rust egui UI 中 endpoint test 可能是 blocking 请求。

本 phase 不强制引入完整 async runtime，但必须避免 UI 明显卡死：

- 如果沿用 blocking，请把超时时间控制在现有 endpoint test 的短超时范围内。
- 测试过程中 provider box 显示 spinner / `Testing...`。
- 同一 provider 正在测试时禁用重复点击。
- 批量测试如果保留，避免长时间锁住整个 UI。

后续可以单独 phase 把 endpoint test worker 化。

## 非目标

- 不改变 endpoint test 的请求协议和 provider kind 判断。
- 不改变 phase 56 的诊断日志字段含义，只调整 UI 事件来源。
- 不实现新的测速算法。
- 不把 endpoint test 变成 provider pool health check。
- 不要求保存配置后才能测试。
- 不实现后台定时自动测试。
- 不修改 server routing 行为。

## 预计修改文件

- `ccr-rust/crates/ccr-ui/src/app.rs`
- `ccr-rust/crates/ccr-ui/src/main.rs`
- `ccr-rust/crates/ccr-ui/src/endpoint_test_tab.rs`
- `ccr-rust/crates/ccr-ui/src/config_tab.rs`
- `ccr-rust/crates/ccr-ui/src/config_tab_new.rs`
- `ccr-rust/crates/ccr-app-core/src/endpoint.rs`

可能需要：

- `ccr-rust/crates/ccr-app-core/src/settings.rs`
- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`

## 规划的测试用例

### UI structure

- app tab list 不再包含 `Endpoint Test`。
- `Tab::EndpointTest` 不再存在。
- `endpoint_test_tab.rs` 不再被编译引用。
- Providers 页面不再显示独立 `Endpoint Testing` heading。
- Providers 页面第一行显示单一 `Test Providers` 按钮。
- 每个 provider box 不显示单独测试按钮。
- 每个 provider box 显示该 provider 的 last test 状态。

### Provider-local test behavior

- 点击 `Test Providers` 测试当前编辑中的所有 provider。
- provider A 的结果不写到 provider B。
- provider B 的结果不覆盖 provider A。
- endpoint 修改后测试使用当前编辑值，而不是磁盘旧值。
- API kind explicit 修改后测试使用新 kind。
- API kind inferred 时测试使用当前推断结果。
- endpoint 为空时按钮禁用或返回明确错误。
- invalid URL 显示 `Invalid endpoint URL`，不 panic。

### Result state

- 成功结果显示 available 和 latency。
- HTTP 4xx 显示 failed 和 HTTP status。
- HTTP 5xx 显示 failed 和 HTTP status。
- 网络错误显示 connection error。
- 详情区 redacts API key。
- 长 response preview 被截断。
- provider 删除后对应结果被清理。
- provider 上移/下移后结果仍跟随正确 provider。
- provider 重命名后结果行为固定并有测试覆盖。

### Logging

- 点击 Test Providers 写入 `providers_test_clicked` UI log。
- 测试开始写入 `[endpoint-test] event="start"`。
- 测试完成写入 `[endpoint-test] event="result"`。
- UI 完成写入 `providers_test_completed`。
- 日志包含 provider name、endpoint、api kind、mode、model。
- 日志中的 API key/header secret 被 redacted。
- 不再从新 UI 写入 `run_selected_clicked` / `run_selected_completed`。

### Regression

- endpoint test request builder 现有单元测试保持通过。
- endpoint diagnostics logging 现有测试保持通过。
- config 保存/加载行为不受测试状态影响。
- provider add/delete/reorder 行为不受测试状态影响。

### 验证命令

- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-ui --bin ccr-server`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- UI 不再有独立 `Endpoint Test` tab。
- Provider config 页面不再有独立 `Endpoint Testing` 区域。
- Provider 页面第一行只有一个测试所有 provider 的按钮。
- 每个 provider config box 不出现单独测试按钮。
- 测试使用当前正在编辑的 provider 配置。
- 测试结果显示在对应 provider box 内，不混到全局表格。
- endpoint test 的诊断日志仍然完整可用。
- API key 和敏感 header 在 UI 详情和日志里都不会明文显示。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 独立 `Endpoint Test` tab 和未引用的旧 `config_tab_new.rs` 已移除；`ccr_app_core::endpoint::EndpointTestState` 暂时保留，因为相关单元测试和 failover apply 逻辑仍可作为底层能力复用。
- 纠偏后采用单一顶部 `Test Providers` 按钮，点击后测试所有 provider；每个 provider box 只显示自己的结果，不放独立测试按钮。
- 测试结果按 provider name + endpoint 绑定；重复 name+endpoint 的极端场景会共享展示结果，后续如需要可引入稳定 provider UI id。
