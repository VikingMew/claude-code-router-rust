# plan-014 - Route Pool Runtime Recovery

**状态：** completed
**优先级：** P1
**计划编号：** plan-014
**最后更新：** 2026-05-21

## 目标

- Add authenticated UI-facing Route Pool runtime recovery actions for clear-ban and reset-route.
- Add queryable Route Pool runtime event history for failed, banned, skipped, recovered, exhausted and user recovery events.
- Show runtime state, recent events and recovery controls in the Status/Routing UI.
- Document that Route Pool ban/failure health state is current-server-session state and is cleared by server restart.

## 非目标

- Do not add a new routing model or automatic route ordering.
- Do not use endpoint test results as Route Pool ban/reset input.
- Do not expand `/api/admin/*` for this feature.
- Do not change client injection semantics.

## 背景

CCR is UI-first and Route Pool is the only primary routing model. Runtime ban/failure state currently lives only in server memory and is exposed read-only through `/api/route-pool/status`; users cannot clear a ban or inspect the recent Route Pool event chain from the UI.

## 设计方向

- Keep ban/failure health state in memory for the current server session.
- Add an in-memory bounded event ring to the server and continue writing Route Pool events to the app log.
- Add authenticated `/api/route-pool/events`, `/api/route-pool/clear-ban` and `/api/route-pool/reset-route` endpoints.
- Put blocking UI HTTP calls in `ccr-app-core::runtime_status`; keep egui code focused on rendering and dispatch.

## 修改文件

- `crates/ccr-server/src/main.rs`
- `crates/ccr-app-core/src/runtime_status.rs`
- `crates/ccr-app-core/src/status.rs`
- `crates/ccr-ui/src/status_tab.rs`
- `docs/long-term-roadmap.md`

## 验收测试

- `cargo fmt --check`
- `cargo test --package ccr-server route_pool`
- `cargo test --package ccr-app-core runtime_status`
- UI compile coverage through package tests/build where practical.

## Do / 执行记录

- 实际修改: Added in-memory Route Pool event history, authenticated `/api/route-pool/events`, `/api/route-pool/clear-ban` and `/api/route-pool/reset-route`, app-core clients, Status/Routing UI event display and clear/reset controls, and roadmap persistence semantics.
- 实际偏离计划: Live GUI manual QA was not performed in this unattended session; UI coverage was compile/unit-level plus app-core/server behavior tests.
- 中途决策: Follow newest human Linear comment and ticket description; implement non-persistent health state with explicit UI/docs text.

## Check / 验证与偏差

- 验证命令: `cargo fmt --check`; `cargo test --package ccr-server route_pool`; `cargo test --package ccr-app-core runtime_status`; `cargo test --package ccr-ui status_tab`; `cargo test --workspace`.
- 手工 QA: Not run against a live desktop window in this unattended session.
- 发现的偏差: None in automated verification.
- 代码和文档不一致: Resolved by updating `docs/long-term-roadmap.md` with session-scoped Route Pool health-state semantics.

## Act / 处理与沉淀

- 已处理偏差: Captured non-persistence as explicit product behavior in UI text and roadmap.
- 长期文档更新: `docs/long-term-roadmap.md`.
- 新增/更新技术债: None.
- 后续计划: None.

## 决策日志

- 2026-05-21: Route Pool ban/failure state remains non-persistent. Recovery actions and recent event history are session-scoped and exposed through authenticated UI-facing APIs.
- 2026-05-23: Newest Linear human feedback says the branch still has merge conflicts and asks to pull latest and fix them. Treat current work as conflict resolution on required branch `vikingmew-ccr-22` while preserving the completed CCR-22 feature scope.

## 完成记录

Implemented and verified on 2026-05-21. Route Pool recovery actions and recent events are session-scoped and exposed through authenticated UI-facing APIs.
