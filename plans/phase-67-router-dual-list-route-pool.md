# Phase 67 - Router Dual List Route Pool

**状态：** ✅ 已完成  
**优先级：** P0

## 目标

把 Router 页面里的 `Failover`、`Provider Pool`、`Primary Route` 这些分散 routing 概念，收敛成一个 UI 优先的双列表 Route Pool 工作区：

- 左边 list：可用 route / provider 来源。
- 右边 list：当前 Route Pool 顺序。
- 中间只有两个移动按钮：
  - `>`：把左侧选中 route 移入右侧 Route Pool。
  - `<`：把右侧选中 route 移回左侧，即从 Route Pool 移除。
- 用户通过选择 list item 和左右移动来管理请求失败后的 provider 选择顺序。
- UI 上不再要求用户理解 `Failover` 和 `RoutePool` 两套内部概念。
- Router 页面不再单独展示 `Primary Route` 编辑区；Phase 67 不再把 `Router.default` 作为底层兼容字段或交付边界。

最终用户看到的是一个统一的：

```text
Route Pool
```

而不是：

```text
Provider Pool
Failover
```

## 背景

Phase 65 把 routing 相关配置集中到了 Router tab：

- `Primary Route`
- `Provider Pool`
- `Failover`

Phase 60 已经把 provider pool 做成了主要运行时能力：

- 连续失败计数。
- ban 一小时。
- 跳过不可用 provider。
- 所有 provider 不可用时返回最后一个实际尝试 provider 的错误。

但当前 Router UI 仍然把 `Provider Pool` 和 `Failover` 分开展示，这会产生两个问题：

1. 用户不关心内部配置结构，只关心“按什么顺序尝试 provider”。
2. `Failover` 和 `Provider Pool` 在产品语义上已经重叠，分开会让用户误以为需要配置两遍。

本 phase 要修正 UI 模型：Router 页面只展示一个统一的 route pool 管理体验。用户看到和操作的是“候选 route 列表”和“当前 Route Pool 列表”，不是 primary/default/failover/provider-pool 的内部拆分。

## 设计方向

### 1. Router 页面结构

Router tab 结构调整为：

```text
Router

Route Pool
  Enabled
  Failure threshold
  Ban duration

  Available Routes          Controls          Route Pool
  ----------------          --------          --------------------
  provider-a                   >              1. provider-b
  provider-a,model-x           <              2. provider-c
  provider-b,model-y                          3. provider-a,model-x
```

不再有独立的 `Provider Pool` section。

不再有独立的 `Failover` section。

不再有独立的 `Primary Route` section。

Router tab 不显示 `Primary Route` / `Default` 这样的单独配置块，避免用户以为它和 Route Pool 是两套都必须配置的策略。

### 2. 左边 list：Available Routes

左边 list 是候选来源，不是运行时顺序。

来源：

- 当前 Config / Providers 页面里配置的 provider。
- 每个 provider 的 provider-only route：
  - `provider`
- 每个 provider 的显式 model route：
  - `provider,model`

显示规则：

- 已经在右侧 Route Pool 里的 route，不应继续作为可移入项出现；最简单实现是从左侧 list 中过滤掉。
- provider-only route 要明确表示它会透传入站 model。
- `provider,model` route 要明确表示它会固定到这个 model。
- 缺 endpoint / API key 的 provider 可以显示 warning，但不阻止添加，避免破坏高级配置。

操作：

- 用户点击左侧 list item 后，选中该 route。
- 中间 `>` 按钮把左侧选中 route 加到右边 Route Pool 末尾。
- 不允许重复添加同一个 route。
- 支持搜索或过滤可以后置，本 phase 不强制。

### 3. 右边 list：Active Route Pool

右边 list 是实际参与请求选择的有序列表。

它对应运行时 provider pool 的 candidates：

- route
- enabled
- priority
- failure count
- banned until
- last error summary

操作：

- 用户点击右侧 list item 后，选中该 route。
- 中间 `<` 按钮把右侧选中 route 从 Route Pool 移除，并让它重新回到左侧可用列表。
- 启用 / 禁用。
- 排序仍需要支持，但不能放在每行里造成“每个 route 一排按钮”的 UI；可以用 list 外部的上移 / 下移控件、键盘快捷键，或后续拖拽实现。
- 清除运行时 ban 状态可以后置，本 phase 不强制。

排序规则：

- 右侧显示顺序就是 `priority` 顺序。
- 上移 / 下移后必须重新归一化 priority。
- 删除后必须重新归一化 priority。
- 新增 route 追加到最后。

### 4. 统一 Route Pool 文案

UI 文案使用：

```text
Route Pool
```

含义：

- route pool 是 UI 的主要 routing 策略入口。
- Route Pool candidates 是 Router 页唯一主配置来源。
- provider 连续失败达到 threshold 后会被临时跳过。

不要在 UI 主体里继续强调：

```text
Provider Pool
Static Failover
```

这些可以作为内部实现或兼容说明出现在文档，不应该成为用户主要操作对象。

### 5. 底层配置兼容

短期底层仍以 `RoutePool` 为主：

- `RoutePool.enabled`
- `RoutePool.failureThreshold`
- `RoutePool.banSeconds`
- `RoutePool.candidates`

旧 `Failover` 配置不在 Router 页面作为单独编辑区展示。

兼容策略：

- 如果旧配置只有 `Failover.providers`，Router 页面可以提供一次性 import 提示。
- 如果 `RoutePool.candidates` 已存在，优先展示 RoutePool。
- 本 phase 不删除 `Failover` 结构，避免破坏旧配置文件读取；但 Router 页和本 phase 的运行时选择不把它作为隐藏 fallback。

推荐本 phase 实现最小迁移行为：

- Router 页面加载时不自动改写旧 Failover。
- 当 `RoutePool` 为空且 `Failover.providers` 非空时，显示 `Import legacy failover routes` 按钮。
- 用户点击后，把 enabled 的 failover routes 加入 active pool。

### 6. 旧 Default Route 不再参与本页模型

Router 页面不显示独立的 Primary Route / Default Route 编辑区。

原因：

- 用户需要管理的是 Route Pool，不应同时面对 Primary Route 和 Route Pool 两套看起来并列的策略。
- Phase 67 的数据模型以 `RoutePool.candidates` 为准，不再把旧 default route 当作需要同步维护的字段。
- client 注入、状态展示、运行时选择都应优先从 Route Pool 推导当前 route；如果 Route Pool 为空，则显示明确的“Route Pool empty / not configured”状态，而不是回到另一个隐藏 default route。

实现要求：

- 不在 Router tab 显示 `Primary Route` heading。
- 不在 Router tab 显示 `Default` heading。
- 不从 Router tab 写入旧 default route。
- 不需要在 Route Pool 和旧 default route 之间做同步。
- 不设计隐藏 default fallback；Route Pool 为空就是未配置，需要用户在双列表中加入 route。
- Router tab 的保存结果应以 `RoutePool.candidates` 表达用户选择。
- provider-only route 会保留入站 model。
- `provider,model` route 会固定 model。

### 7. Status 页关系

Status 页不再显示多个相似 status。

Routing 状态建议显示：

- Route Pool enabled。
- active candidate count。
- available candidate count。
- banned candidate count。
- 当前可用的第一个 candidate。

Status 页不显示旧 default route 作为主状态，也不把它作为 Route Pool 的隐藏 fallback 展示。

不再分别显示：

```text
Failover Status
Provider Pool Status
```

如果底层仍有 legacy Failover，可以在详情里显示：

```text
Legacy failover configured
```

但不要作为主状态块。

### 8. UI 交互细节

双 list 区域需要适合桌面 app：

- 左侧 list、中间移动按钮、右侧 list 三栏布局。
- 中间栏只有左右移动按钮，不能把 `Add` / `Remove` 放进每一行。
- 左右两列固定比例或可响应宽度。
- 每列内部有独立滚动区域。
- 每一行高度稳定，避免按钮导致布局跳动。
- 操作用 icon 或短按钮：
  - `>` move selected route into Route Pool
  - `<` remove selected route from Route Pool
  - enable checkbox
- 不在每个 provider/route 行里放 test 按钮。
- 不在每个 provider/route 行里放独立 Add/Remove 按钮。
- 不把每个 route 做成大卡片。
- 不使用嵌套 card。
- 内容多时只滚动 list，不把整页挤高。

## 非目标

- 不删除底层 `Failover` 配置结构。
- 不重写 server provider pool/circuit breaker 算法。
- 不增加每个 provider 的单独 endpoint test 按钮。
- 不恢复独立 Endpoint Test tab。
- 不在 Router tab 单独展示 Primary Route / Default Route 编辑区。
- 不把每行按钮式 Add/Remove 当作双 list 实现。
- 不把 CLI 作为本 phase 的主要交付边界；项目方向是 UI 优先。
- 不实现 provider 搜索、拖拽排序、批量导入导出；这些可以后置。

## 预计修改文件

- `ccr-rust/crates/ccr-ui/src/router_tab.rs`
- `ccr-rust/crates/ccr-ui/src/status_tab.rs`
- `ccr-rust/crates/ccr-app-core/src/settings.rs`

可能新增：

- `ccr-rust/crates/ccr-ui/src/router_pool_list.rs`
- `ccr-rust/crates/ccr-app-core/src/router_pool.rs`

文档：

- `ccr-rust/docs/cc-switch-feature-gap-analysis.md`

## 规划的测试用例

### Router dual list structure

- Router tab 不再显示独立 `Provider Pool` section。
- Router tab 不再显示独立 `Failover` section。
- Router tab 不再显示独立 `Primary Route` / `Default Route` section。
- Router tab 显示统一的 `Route Pool` section。
- `Route Pool` section 同时显示左侧 available routes、中间移动按钮、右侧 active pool。
- 中间只有 `>` 和 `<` 两个主移动按钮。
- route 行内不显示 `Add` / `Remove` 按钮。
- 每个 list 有独立滚动区域。

### Available routes

- provider-only route 出现在 available routes。
- provider model route 出现在 available routes。
- 已在 active pool 的 route 不出现在可移入列表，或者至少不能被再次移入。
- 选中左侧 route 后点击 `>`，route 被追加到 active pool 末尾。
- 缺少 endpoint/API key 的 route 显示 warning，但仍可添加。

### Active pool

- active pool 读取 `RoutePool.candidates`。
- enabled checkbox 修改 candidate enabled 状态。
- 选中右侧 route 后点击 `<`，route 从 active pool 删除并重新出现在左侧 available routes。
- 上移 candidate 后 priority 正确归一化。
- 下移 candidate 后 priority 正确归一化。
- 删除 candidate 后 priority 正确归一化。
- 新增 candidate 后 priority 追加到最后。

### Legacy failover compatibility

- 旧 `Failover.providers` 存在且 RoutePool 为空时，显示 import 提示。
- 点击 import 后，legacy failover routes 加入 RoutePool candidates。
- import 不删除旧 Failover 配置。
- RoutePool 已有 candidates 时，不自动覆盖。

### Status page

- Status 页面不再分别显示旧的静态 fallback 状态和运行时 pool 状态主块。
- Status 页面显示统一 Routing / Route Pool 摘要。
- RoutePool 未配置时不请求 runtime pool API。
- RoutePool enabled 且 server running 时显示 runtime candidate 状态。

### Regression

- Router tab 保存 Route Pool 时不写入旧 default route。
- Route Pool 为空时，UI 明确显示未配置，不展示隐藏 fallback。
- Config / Providers 页面保存不会丢失 RoutePool。
- Settings 页面保存不会丢失 RoutePool。
- RoutePool server runtime 行为不变。
- Logs tab 和 Endpoint Test 行为不受影响。

### 验证命令

- `cargo test --package ccr-app-core --lib`
- `cargo test --package ccr-ui`
- `cargo test --workspace`
- `cargo build --bin ccr-ui --bin ccr-server`
- `cargo llvm-cov --workspace --lib --summary-only` 行覆盖率保持大于 80%。

## 验收标准

- Router 页面有左右双 list 的 Route Pool 工作区。
- 用户可以选中左侧 available route，通过中间 `>` 加入右侧 active pool。
- 用户可以选中右侧 active route，通过中间 `<` 从 active pool 移除。
- 用户可以在右侧 active pool 启用/禁用、调整顺序。
- Router 页面不再出现单独的 Primary Route / Default Route 配置块。
- Router 页面不再依赖或同步旧 default route。
- UI 不再把 legacy Failover 和运行时 pool 拆成两个主要配置区。
- 旧 Failover 配置仍兼容，不被自动破坏。
- Status 页面 routing 状态合并，不再出现多个相似 status。
- 测试通过，覆盖率保持大于 80%。

## 预留偏差

- 右侧 Route Pool 的排序使用 list 外部的 `Up` / `Down` 按钮完成，没有做拖拽排序；这符合本 phase “不在每行放按钮”的要求。
- 旧 `Failover` 结构仍保留读取和手动 import 能力；Router 页不会把它作为独立配置区，也不会把它作为隐藏 fallback。
- `Router.default` 仍可能被其他历史 CLI/preset/tray 代码读取或写入，但 Phase 67 的 Router tab、Status routing 主状态和 server request 主路径不再依赖或同步它。
- server 在 Route Pool disabled、缺失或没有 enabled candidates 时直接返回 `Route Pool is not configured or has no enabled routes`，不再回退到 default route 或 legacy failover。
- 本 phase 没有实现清除单个 route ban 状态、搜索过滤或 runtime health score，这些留给后续 Route Pool 高级能力。
