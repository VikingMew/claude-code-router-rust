# Legacy Plans Inventory

**状态：** completed inventory
**最后验证：** 2026-05-07

This inventory records the top-level `plans/phase-*.md` files before removing the top-level `plans/` directory.

Decision: preserve all legacy phase documents by moving them to `docs/exec-plans/completed/legacy-phase-*.md`. Their internal status lines remain historical and do not represent current active work.

## Inventory

| Original path | Title | Historical status | Action |
| --- | --- | --- | --- |
| `plans/phase-36-api-tokenizer.md` | Phase 36 - API Tokenizer Backend | implementing | migrate |
| `plans/phase-37-config-backup.md` | Phase 37 - Config Backup on Apply | implementing | migrate |
| `plans/phase-38-proxy-support.md` | Phase 38 - Proxy Support | implementing | migrate |
| `plans/phase-39-macos-installer.md` | Phase 39 - macOS installer pkg | pending | migrate |
| `plans/phase-39b-macos-dmg.md` | Phase 39B - macOS dmg | completed | migrate |
| `plans/phase-40-windows-msi.md` | Phase 40 - Windows MSI | pending | migrate |
| `plans/phase-41-system-tray.md` | Phase 41 - System Tray | completed | migrate |
| `plans/phase-42-macos-app-bundle.md` | Phase 42 - macOS app bundle | planned | migrate |
| `plans/phase-43-codex-backend.md` | Phase 43 - Codex Backend and Config Switch | completed | migrate |
| `plans/phase-44-live-status-dashboard.md` | Phase 44 - Live Status Dashboard | completed | migrate |
| `plans/phase-45-default-config-profiles.md` | Phase 45 - Default Config Profiles | completed | migrate |
| `plans/phase-46-client-native-provider-injection.md` | Phase 46 - Client-Native Provider Injection | completed | migrate |
| `plans/phase-47-failover-priority-ordering.md` | Phase 47 - Failover Priority Ordering | completed | migrate |
| `plans/phase-48-auto-launch.md` | Phase 48 - Auto Launch | completed | migrate |
| `plans/phase-49-complete-settings-page.md` | Phase 49 - Complete Settings Page | completed | migrate |
| `plans/phase-50-endpoint-speed-test.md` | Phase 50 - Endpoint Speed Test | completed | migrate |
| `plans/phase-51-core-coverage-lift.md` | Phase 51 - Core Coverage Lift | completed | migrate |
| `plans/phase-52-ui-logic-decoupling.md` | Phase 52 - UI Logic Decoupling | completed | migrate |
| `plans/phase-53-provider-api-kind-choice.md` | Phase 53 - Provider API Kind Choice | completed | migrate |
| `plans/phase-54-through-ccr-pipeline-alignment.md` | Phase 54 - Through-CCR Pipeline Alignment | completed | migrate |
| `plans/phase-55-claude-code-model-mapping.md` | Phase 55 - Claude Code Model Mapping | completed | migrate |
| `plans/phase-56-endpoint-test-diagnostics-logging.md` | Phase 56 - Endpoint Test Diagnostics Logging | completed | migrate |
| `plans/phase-57-anthropic-compatible-model-and-502-diagnostics.md` | Phase 57 - Anthropic-Compatible Model Resolution and 502 Diagnostics | completed | migrate |
| `plans/phase-58-logs-tab-focus-auto-refresh.md` | Phase 58 - Logs Tab Focus-Aware Auto Refresh | completed | migrate |
| `plans/phase-59-logs-chinese-rendering.md` | Phase 59 - Logs Chinese Rendering and Daily Rotation | completed | migrate |
| `plans/phase-60-provider-pool-circuit-breaker.md` | Phase 60 - Provider Pool and Circuit Breaker | completed | migrate |
| `plans/phase-61-opencode-client.md` | Phase 61 - OpenCode Client Support | completed | migrate |
| `plans/phase-62-openclaw-client.md` | Phase 62 - OpenClaw Client Support | completed | migrate |
| `plans/phase-63-status-page-information-architecture.md` | Phase 63 - Status Page Information Architecture | completed | migrate |
| `plans/phase-64-inline-provider-endpoint-test.md` | Phase 64 - Inline Provider Endpoint Test | completed | migrate |
| `plans/phase-65-router-tab-routing-controls.md` | Phase 65 - Router Tab Routing Controls | completed | migrate |
| `plans/phase-66-per-tab-scroll-areas.md` | Phase 66 - Per-Tab Scroll Areas | completed | migrate |
| `plans/phase-67-router-dual-list-route-pool.md` | Phase 67 - Router Dual List Route Pool | completed | migrate |
| `plans/phase-68-remove-legacy-routing-code.md` | Phase 68 - Remove Legacy Routing Code | completed | migrate |

## Legacy Terminology Risk

Several legacy phase files contain historical terms such as Failover, ProviderPool, Primary Route and Router.default. Those terms are historical only. Current architecture remains Route Pool-only as documented in `ARCHITECTURE.md` and `docs/long-term-roadmap.md`.
