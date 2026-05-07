# TD-012 - Release Checklist

**状态：** resolved
**优先级：** P2
**关联技术债：** TD-012
**最后更新：** 2026-05-07

## 目标

为 macOS DMG/pkg 和 Windows MSI 打包流程建立统一 release checklist，降低后续修改 packaging 脚本时的验证成本。

## 非目标

- 不重写 packaging 脚本。
- 不发布新的 release。
- 不实现自动更新。
- 不要求本阶段完成跨平台 CI 打包。

## 背景

仓库已有 `packaging/` 脚本和 `dist/` 产物，但没有统一说明如何构建、安装、卸载和 smoke test。

## 设计方向

新增 release 文档，覆盖：

- release prerequisites。
- build commands。
- macOS app bundle。
- macOS DMG。
- macOS pkg。
- Windows MSI。
- install smoke test。
- uninstall smoke test。
- known signing/notarization gaps。

建议文档位置：

```text
docs/release.md
```

或者如果更偏打包维护：

```text
packaging/README.md
```

优先建议 `docs/release.md`，并从 `docs/index.md` 链接。

## 修改文件

- `docs/release.md`
- `docs/index.md`
- `packaging/macos-dmg/README.txt` optionally cross-link
- `docs/exec-plans/tech-debt-tracker.md`

## 验收测试

文档检查：

- 覆盖 `packaging/macos/build-pkg.sh`。
- 覆盖 `packaging/macos-dmg/build-app.sh`。
- 覆盖 `packaging/macos-dmg/build-dmg.sh`。
- 覆盖 `packaging/windows/build-msi.ps1`。
- 包含 install/uninstall smoke checklist。

可选本地验证：

```sh
cargo build --release
```

## 决策日志

- 2026-05-07：先补 release checklist，不触碰打包脚本行为。

## 完成记录

完成：2026-05-07。

## 完成偏差

- 原计划只要求 release checklist；实际已新增 `docs/release.md` 并从 `docs/index.md` 链接。
- 没有实际构建 DMG、pkg 或 MSI，也没有执行安装/卸载 smoke。
- 没有修改 packaging 脚本行为；后续发现的 packaging 脚本问题应另开执行计划处理。
- `cargo build --release` 未作为本计划的一部分完成验证；本轮完成的是文档闭环。
