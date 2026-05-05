# Phase 39B — macOS .dmg 拖拽安装包（可选）

**状态：** ✅ 已完成
**优先级：** P3（可选增强）
**依赖：** Phase 39 (.pkg) 已完成

---

## 📋 目标

为 macOS 创建 .dmg 拖拽安装包，提供更简单的用户安装体验。

---

## 🎯 与 .pkg 的区别

| 特性 | .pkg (Phase 39) | .dmg + .app (本Phase) |
|------|-----------------|----------------------|
| 安装方式 | 安装向导 | 拖拽图标 |
| 需要密码 | ✅ 是 | ❌ 否 |
| 安装位置 | `/usr/local/bin` | `/Applications` |
| 命令行使用 | ✅ 直接使用 | ⚠️ 需额外配置 |
| 用户体验 | 传统 | 现代、简单 |

---

## 📐 实现步骤

### Step 1: 创建 .app Bundle

**目录结构：**
```
Claude Code Router.app/
├── Contents/
│   ├── Info.plist
│   ├── MacOS/
│   │   └── ccr-ui
│   ├── Resources/
│   │   ├── bin/
│   │   │   ├── ccr-server
│   │   │   └── ccr-cli
│   │   ├── icon.icns
│   │   └── install-cli.sh    (可选：安装命令行工具)
│   └── PkgInfo
```

**Info.plist:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Claude Code Router</string>
    <key>CFBundleDisplayName</key>
    <string>Claude Code Router</string>
    <key>CFBundleIdentifier</key>
    <string>com.musistudio.ccr</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleExecutable</key>
    <string>ccr-ui</string>
    <key>CFBundleIconFile</key>
    <string>icon.icns</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
```

---

### Step 2: 创建构建脚本

**build-app.sh:**
```bash
#!/bin/bash
set -e

VERSION="0.1.0"
APP_NAME="Claude Code Router.app"
BUILD_DIR="build/macos-app"

echo "Building macOS .app bundle..."

# Build release binaries
cargo build --release --workspace

# Create .app structure
mkdir -p "${BUILD_DIR}/${APP_NAME}/Contents/MacOS"
mkdir -p "${BUILD_DIR}/${APP_NAME}/Contents/Resources/bin"

# Copy main executable
cp target/release/ccr-ui "${BUILD_DIR}/${APP_NAME}/Contents/MacOS/"

# Copy CLI tools to Resources/bin
cp target/release/ccr-server "${BUILD_DIR}/${APP_NAME}/Contents/Resources/bin/"
cp target/release/ccr-cli "${BUILD_DIR}/${APP_NAME}/Contents/Resources/bin/"

# Copy Info.plist
cp packaging/macos-dmg/Info.plist "${BUILD_DIR}/${APP_NAME}/Contents/"

# Copy icon (需要先创建 .icns)
if [ -f "packaging/macos-dmg/icon.icns" ]; then
    cp packaging/macos-dmg/icon.icns "${BUILD_DIR}/${APP_NAME}/Contents/Resources/"
fi

# Copy CLI install script
cat > "${BUILD_DIR}/${APP_NAME}/Contents/Resources/install-cli.sh" <<'EOF'
#!/bin/bash
# Install CLI tools to /usr/local/bin

APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${APP_DIR}/Resources/bin"

sudo ln -sf "${BIN_DIR}/ccr-server" /usr/local/bin/ccr-server
sudo ln -sf "${BIN_DIR}/ccr-cli" /usr/local/bin/ccr-cli

echo "✅ CLI tools installed to /usr/local/bin"
EOF

chmod +x "${BUILD_DIR}/${APP_NAME}/Contents/Resources/install-cli.sh"

echo "✅ .app bundle created: ${BUILD_DIR}/${APP_NAME}"
```

---

### Step 3: 创建 .dmg

**build-dmg.sh:**
```bash
#!/bin/bash
set -e

VERSION="0.1.0"
APP_NAME="Claude Code Router.app"
DMG_NAME="Claude-Code-Router-${VERSION}.dmg"
BUILD_DIR="build/macos-app"
DMG_DIR="build/dmg"

echo "Creating .dmg installer..."

# Create DMG staging directory
mkdir -p "${DMG_DIR}"

# Copy .app to DMG directory
cp -R "${BUILD_DIR}/${APP_NAME}" "${DMG_DIR}/"

# Create symlink to Applications
ln -s /Applications "${DMG_DIR}/Applications"

# Optional: Add background image and .DS_Store for custom layout
if [ -f "packaging/macos-dmg/background.png" ]; then
    mkdir -p "${DMG_DIR}/.background"
    cp packaging/macos-dmg/background.png "${DMG_DIR}/.background/"
fi

# Create DMG
hdiutil create -volname "Claude Code Router" \
    -srcfolder "${DMG_DIR}" \
    -ov -format UDZO \
    "dist/${DMG_NAME}"

echo "✅ DMG created: dist/${DMG_NAME}"

# Clean up
rm -rf "${DMG_DIR}"
```

---

### Step 4: 用户安装流程

**用户体验：**

1. 下载 `Claude-Code-Router-0.1.0.dmg`
2. 双击打开
3. 看到窗口：
   ```
   ┌─────────────────────────────────┐
   │                                 │
   │   [CCR图标]  →  [Applications]  │
   │                                 │
   └─────────────────────────────────┘
   ```
4. 拖动图标到 Applications
5. 完成！

**使用应用：**
- 在 Applications 中找到 "Claude Code Router"
- 双击启动 GUI

**安装命令行工具（可选）：**
```bash
# 方式1：右键菜单
右键应用 → 显示包内容 → Contents/Resources/install-cli.sh
运行脚本安装命令行工具

# 方式2：在GUI中添加菜单项
应用内菜单：Tools → Install Command Line Tools
```

---

## 📊 完整文件结构

```
ccr-rust/
├── packaging/
│   ├── macos/              (Phase 39 - .pkg)
│   └── macos-dmg/          (Phase 39B - .dmg)
│       ├── Info.plist
│       ├── icon.icns
│       ├── background.png  (可选)
│       ├── build-app.sh
│       └── build-dmg.sh
├── build/
│   ├── macos/              (.pkg 构建)
│   └── macos-app/          (.app bundle)
│       └── Claude Code Router.app/
└── dist/
    ├── claude-code-router-*.pkg  (Phase 39)
    └── Claude-Code-Router-*.dmg  (Phase 39B)
```

---

## 🎨 创建 .icns 图标

**需要准备：**
```
icon_512x512.png   (512x512)
icon_256x256.png   (256x256)
icon_128x128.png   (128x128)
icon_64x64.png     (64x64)
icon_32x32.png     (32x32)
icon_16x16.png     (16x16)
```

**转换为 .icns：**
```bash
# 创建 iconset
mkdir icon.iconset
cp icon_512x512.png icon.iconset/icon_512x512.png
cp icon_256x256.png icon.iconset/icon_256x256.png
cp icon_128x128.png icon.iconset/icon_128x128.png
cp icon_64x64.png icon.iconset/icon_64x64.png
cp icon_32x32.png icon.iconset/icon_32x32.png
cp icon_16x16.png icon.iconset/icon_16x16.png

# 转换
iconutil -c icns icon.iconset -o icon.icns
```

---

## 🔄 两种安装方式的选择

### 推荐策略：

**提供两种安装包，让用户选择：**

1. **claude-code-router-0.1.0-arm64.pkg** (Phase 39)
   - 适合：开发者、命令行用户
   - 特点：自动配置PATH，可直接使用命令

2. **Claude-Code-Router-0.1.0.dmg** (Phase 39B)
   - 适合：普通用户、GUI用户
   - 特点：拖拽安装，简单直观

---

## 📝 发布说明示例

```markdown
## 下载

### 方式1：.dmg 拖拽安装（推荐普通用户）
- [下载 Claude-Code-Router-0.1.0.dmg](...)
- 双击打开，拖动图标到 Applications 文件夹

### 方式2：.pkg 安装包（推荐开发者）
- [下载 claude-code-router-0.1.0-arm64.pkg](...)
- 双击安装，自动配置命令行工具

### 命令行工具
- .dmg 用户：启动应用后，在菜单中选择 "Install CLI Tools"
- .pkg 用户：安装后直接可用
```

---

## ✅ 完成标准

- [x] Info.plist 创建
- [x] build-app.sh 脚本
- [x] build-dmg.sh 脚本
- [x] icon.icns 图标
- [x] install-cli.sh 脚本
- [x] .app bundle 构建测试
- [x] .dmg 创建测试
- [ ] 安装测试
- [x] 文档更新

---

**总结：** Phase 39B 提供了 .dmg 拖拽安装方式作为 .pkg 的补充，让不同类型的用户都能选择最适合自己的安装方式。
