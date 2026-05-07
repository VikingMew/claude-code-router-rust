# Product Sense

**状态：** 长期产品判断文档
**最后验证：** 2026-05-07

## Target User

The target user is a local AI coding user who needs multiple clients and providers to work reliably through one local router.

They care about:

- whether the local server is running;
- whether their client is currently using CCR;
- which provider/model route will be attempted;
- why a provider failed;
- how to recover without damaging existing config.

## Product Priorities

1. Safe local operation.
2. Clear client injection state.
3. Reliable routing and retry behavior.
4. Provider diagnostics that explain failures.
5. Fast recovery from bad config.
6. Presets that accelerate setup without hiding what changed.

## Tradeoffs

- Prefer visible operational detail over simplified but ambiguous UI.
- Prefer preserving user config over aggressive cleanup.
- Prefer explicit advanced modes over automatic inference when behavior could surprise users.
- Prefer local observability over external dependencies.

## Things To Avoid

- Treating endpoint test success as proof of provider quality.
- Mixing client-visible model names with upstream route strings.
- Reintroducing legacy routing names as active concepts.
- Hiding route-pool ban/retry state from the user.
