# Release Checklist

**状态：** 长期发布文档
**最后验证：** 2026-05-07

## Scope

This document records the current packaging entry points and smoke checks for CCR desktop releases.

## Prerequisites

- Rust stable toolchain.
- macOS packaging requires macOS host tools.
- Windows MSI packaging requires Windows host tools and WiX.
- Release builds should start from a clean working tree.

## Baseline Verification

```sh
cargo fmt --check
cargo test --workspace
cargo build --release
```

## macOS App Bundle

Script:

```sh
packaging/macos-dmg/build-app.sh
```

Smoke checklist:

- app bundle is created;
- `ccr-ui` launches;
- server can start from the Status tab;
- `/health` returns ok on the configured local port.

## macOS DMG

Script:

```sh
packaging/macos-dmg/build-dmg.sh
```

Smoke checklist:

- DMG is created under `dist/`;
- mounted DMG contains the app bundle;
- app can be copied to Applications;
- app can start and stop the local server.

## macOS pkg

Script:

```sh
packaging/macos/build-pkg.sh
```

Smoke checklist:

- installer completes;
- installed binaries are present;
- uninstall script removes installed files;
- user config under `~/.claude-code-router` is not removed unless explicitly documented.

## Windows MSI

Script:

```powershell
packaging/windows/build-msi.ps1
```

Smoke checklist:

- MSI builds successfully;
- app installs;
- `ccr-ui` launches;
- uninstall removes installed program files.

## Signing And Notarization

Signing and notarization are not documented as complete in this repository yet. Do not imply a package is signed/notarized unless the release process explicitly verifies it.

## Release Notes

Release notes should include:

- user-visible UI changes;
- routing behavior changes;
- client injection changes;
- config migration notes;
- known issues and rollback guidance.
