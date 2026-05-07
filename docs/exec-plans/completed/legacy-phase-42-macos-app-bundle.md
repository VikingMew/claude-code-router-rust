# Phase 42 — macOS .app Bundle (Standalone)

**状态：** 📋 计划中
**优先级：** P2（用户体验增强）
**依赖：** 无（独立于 Phase 39 .pkg）

---

## 📋 目标

创建独立的 macOS .app bundle，用户可以直接拖动到 /Applications 使用，无需安装程序。

---

## 🎯 设计理念

### 与 .pkg 的区别

| 特性 | .pkg (Phase 39) | .app bundle (本Phase) |
|------|-----------------|----------------------|
| 安装方式 | 安装向导 | 直接拖动 |
| 需要密码 | ✅ 是 | ❌ 否 |
| 安装位置 | `/usr/local/bin` | `/Applications` 或任意位置 |
| 命令行使用 | ✅ 直接使用 | ⚠️ 需用户配置 PATH |
| 用户体验 | 传统、正式 | 现代、简单 |
| 适用场景 | 系统级安装、命令行工具 | GUI应用、便携使用 |

---

## 📐 .app Bundle 结构

```
Claude Code Router.app/
├── Contents/
│   ├── Info.plist                 # Bundle 元数据
│   ├── MacOS/
│   │   └── ccr-ui                 # 主可执行文件
│   ├── Resources/
│   │   ├── bin/
│   │   │   ├── ccr-server         # 服务器二进制
│   │   │   └── ccr-cli            # CLI 工具
│   │   ├── AppIcon.icns           # 应用图标
│   │   └── install-cli.sh         # CLI 安装脚本（可选）
│   └── PkgInfo                    # 包类型标识
```

---

## 📄 Info.plist 配置

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
    <string>AppIcon.icns</string>

    <key>CFBundlePackageType</key>
    <string>APPL</string>

    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>

    <key>NSHighResolutionCapable</key>
    <true/>

    <key>LSUIElement</key>
    <false/>
</dict>
</plist>
```

---

## 🔧 构建脚本

### build-app-bundle.sh

```bash
#!/bin/bash
set -e

VERSION="${1:-0.1.0}"
APP_NAME="Claude Code Router.app"
BUILD_DIR="build/macos-app"
DIST_DIR="dist"

echo "Building macOS .app bundle (Version: ${VERSION})..."

# Clean previous build
rm -rf "${BUILD_DIR}" "${DIST_DIR}/${APP_NAME}"

# Build release binaries
echo "Building release binaries..."
cargo build --release --workspace

# Create .app structure
echo "Creating .app bundle structure..."
mkdir -p "${BUILD_DIR}/${APP_NAME}/Contents/MacOS"
mkdir -p "${BUILD_DIR}/${APP_NAME}/Contents/Resources/bin"

# Copy main executable
cp target/release/ccr-ui "${BUILD_DIR}/${APP_NAME}/Contents/MacOS/"
chmod +x "${BUILD_DIR}/${APP_NAME}/Contents/MacOS/ccr-ui"

# Copy CLI tools to Resources/bin
cp target/release/ccr-server "${BUILD_DIR}/${APP_NAME}/Contents/Resources/bin/"
cp target/release/ccr-cli "${BUILD_DIR}/${APP_NAME}/Contents/Resources/bin/"
chmod +x "${BUILD_DIR}/${APP_NAME}/Contents/Resources/bin/"*

# Copy Info.plist
sed "s/{{VERSION}}/${VERSION}/g" packaging/macos-app/Info.plist > "${BUILD_DIR}/${APP_NAME}/Contents/Info.plist"

# Create PkgInfo
echo -n "APPL????" > "${BUILD_DIR}/${APP_NAME}/Contents/PkgInfo"

# Copy icon (if exists)
if [ -f "packaging/macos-app/AppIcon.icns" ]; then
    cp packaging/macos-app/AppIcon.icns "${BUILD_DIR}/${APP_NAME}/Contents/Resources/"
fi

# Create CLI install script
cat > "${BUILD_DIR}/${APP_NAME}/Contents/Resources/install-cli.sh" <<'EOF'
#!/bin/bash
# Install CLI tools to /usr/local/bin

APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${APP_DIR}/Resources/bin"

echo "Installing CLI tools to /usr/local/bin..."
echo "This requires sudo privileges."

sudo ln -sf "${BIN_DIR}/ccr-server" /usr/local/bin/ccr-server
sudo ln -sf "${BIN_DIR}/ccr-cli" /usr/local/bin/ccr-cli

echo "✅ CLI tools installed successfully!"
echo ""
echo "You can now use:"
echo "  ccr-server"
echo "  ccr-cli"
EOF

chmod +x "${BUILD_DIR}/${APP_NAME}/Contents/Resources/install-cli.sh"

# Move to dist
mkdir -p "${DIST_DIR}"
mv "${BUILD_DIR}/${APP_NAME}" "${DIST_DIR}/"

echo "✅ .app bundle created: ${DIST_DIR}/${APP_NAME}"
echo ""
echo "To use:"
echo "  1. Drag '${APP_NAME}' to /Applications"
echo "  2. Double-click to launch"
echo ""
echo "To install CLI tools (optional):"
echo "  Open the app → Help menu → Install CLI Tools"
echo "  Or run: ${DIST_DIR}/${APP_NAME}/Contents/Resources/install-cli.sh"
```

---

## 📦 打包脚本

### package-app.sh (创建可分发的压缩包)

```bash
#!/bin/bash
set -e

VERSION="${1:-0.1.0}"
APP_NAME="Claude Code Router.app"
DIST_DIR="dist"
ARCHIVE_NAME="Claude-Code-Router-${VERSION}-macOS.tar.gz"

echo "Packaging .app bundle..."

cd "${DIST_DIR}"

# Create tar.gz archive
tar -czf "${ARCHIVE_NAME}" "${APP_NAME}"

echo "✅ Archive created: ${DIST_DIR}/${ARCHIVE_NAME}"
echo ""
echo "Distribution:"
echo "  - Size: $(du -h "${ARCHIVE_NAME}" | cut -f1)"
echo "  - Users can extract and drag to /Applications"
```

---

## 🎨 图标创建

### 准备图标文件

需要准备不同尺寸的 PNG 图标：

```bash
icon_16x16.png
icon_32x32.png
icon_64x64.png
icon_128x128.png
icon_256x256.png
icon_512x512.png
icon_1024x1024.png
```

### 转换为 .icns

```bash
#!/bin/bash
# create-icns.sh

ICONSET="AppIcon.iconset"

# Create iconset directory
mkdir -p "${ICONSET}"

# Copy and rename icons
cp icon_16x16.png "${ICONSET}/icon_16x16.png"
cp icon_32x32.png "${ICONSET}/icon_16x16@2x.png"
cp icon_32x32.png "${ICONSET}/icon_32x32.png"
cp icon_64x64.png "${ICONSET}/icon_32x32@2x.png"
cp icon_128x128.png "${ICONSET}/icon_128x128.png"
cp icon_256x256.png "${ICONSET}/icon_128x128@2x.png"
cp icon_256x256.png "${ICONSET}/icon_256x256.png"
cp icon_512x512.png "${ICONSET}/icon_256x256@2x.png"
cp icon_512x512.png "${ICONSET}/icon_512x512.png"
cp icon_1024x1024.png "${ICONSET}/icon_512x512@2x.png"

# Convert to icns
iconutil -c icns "${ICONSET}" -o AppIcon.icns

# Clean up
rm -rf "${ICONSET}"

echo "✅ AppIcon.icns created"
```

---

## 🚀 用户使用流程

### 1. 下载

```bash
# 用户下载
curl -L https://github.com/.../Claude-Code-Router-0.1.0-macOS.tar.gz -o ccr.tar.gz

# 解压
tar -xzf ccr.tar.gz
```

### 2. 安装

```
拖动 "Claude Code Router.app" 到 /Applications 文件夹
```

### 3. 首次启动

```
1. 在 Applications 中找到 "Claude Code Router"
2. 右键 → 打开（绕过 Gatekeeper 检查）
3. 点击 "打开" 确认
```

### 4. 使用 GUI

```
双击应用图标即可启动 UI
```

### 5. 安装命令行工具（可选）

**方式 1：通过应用菜单**
```
应用菜单 → Help → Install CLI Tools
```

**方式 2：手动运行脚本**
```bash
cd /Applications/Claude\ Code\ Router.app/Contents/Resources
./install-cli.sh
```

**方式 3：手动创建符号链接**
```bash
sudo ln -sf "/Applications/Claude Code Router.app/Contents/Resources/bin/ccr-server" /usr/local/bin/ccr-server
sudo ln -sf "/Applications/Claude Code Router.app/Contents/Resources/bin/ccr-cli" /usr/local/bin/ccr-cli
```

---

## 🔐 代码签名（可选）

### 签名 .app bundle

```bash
# 签名所有可执行文件
codesign --force --deep --sign "Developer ID Application: YOUR NAME (TEAM_ID)" \
  "dist/Claude Code Router.app"

# 验证签名
codesign --verify --verbose "dist/Claude Code Router.app"

# 显示签名信息
codesign -dv "dist/Claude Code Router.app"
```

### 公证（Notarization）

```bash
# 创建 zip 用于公证
ditto -c -k --keepParent "dist/Claude Code Router.app" "dist/CCR-notarize.zip"

# 提交公证
xcrun notarytool submit "dist/CCR-notarize.zip" \
  --apple-id "your@email.com" \
  --team-id "TEAM_ID" \
  --password "app-specific-password" \
  --wait

# 装订公证票据
xcrun stapler staple "dist/Claude Code Router.app"

# 验证公证
spctl -a -v "dist/Claude Code Router.app"
```

---

## 📁 目录结构

```
ccr-rust/
├── packaging/
│   └── macos-app/                # 新建目录
│       ├── Info.plist            # Bundle 配置模板
│       ├── AppIcon.icns          # 应用图标
│       ├── build-app-bundle.sh   # 构建脚本
│       ├── package-app.sh        # 打包脚本
│       └── create-icns.sh        # 图标转换脚本
├── build/
│   └── macos-app/                # 构建输出
│       └── Claude Code Router.app/
└── dist/
    ├── Claude Code Router.app/   # 最终 .app
    └── Claude-Code-Router-*-macOS.tar.gz  # 分发压缩包
```

---

## 🎯 GUI 集成：CLI 安装菜单

在 ccr-ui 中添加菜单项，方便用户安装 CLI 工具：

```rust
// ccr-ui/src/app.rs

impl CcrApp {
    fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Help", |ui| {
                if ui.button("Install CLI Tools").clicked() {
                    self.install_cli_tools();
                }
                if ui.button("About").clicked() {
                    self.show_about = true;
                }
            });
        });
    }

    #[cfg(target_os = "macos")]
    fn install_cli_tools(&mut self) {
        use std::process::Command;

        // Get app bundle path
        let app_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf()));

        if let Some(contents_path) = app_path {
            let script_path = contents_path.join("Resources/install-cli.sh");

            if script_path.exists() {
                // Run install script in Terminal
                let _ = Command::new("open")
                    .arg("-a")
                    .arg("Terminal")
                    .arg(&script_path)
                    .spawn();

                self.status_message = Some("Installing CLI tools in Terminal...".to_string());
            }
        }
    }
}
```

---

## ✅ 完成标准

- [ ] Info.plist 模板创建
- [ ] build-app-bundle.sh 脚本
- [ ] package-app.sh 脚本
- [ ] create-icns.sh 脚本
- [ ] AppIcon.icns 图标文件
- [ ] install-cli.sh 脚本
- [ ] .app bundle 本地构建测试
- [ ] 拖动到 /Applications 测试
- [ ] GUI 启动测试
- [ ] CLI 安装脚本测试
- [ ] 代码签名（可选）
- [ ] 公证（可选）
- [ ] GUI 菜单集成（可选）
- [ ] 文档更新

---

## 📊 与其他安装方式的比较

### Phase 39: .pkg Installer
- ✅ 系统级安装
- ✅ 自动配置 PATH
- ✅ 适合开发者和命令行用户
- ⚠️ 需要管理员密码
- ⚠️ 安装过程较复杂

### Phase 42: .app Bundle（本 Phase）
- ✅ 无需密码
- ✅ 拖拽即用
- ✅ 适合 GUI 用户
- ✅ 便携（可放任意位置）
- ⚠️ CLI 工具需手动安装
- ⚠️ 不会自动配置 PATH

### 建议发布策略

提供两种安装方式，让用户选择：

```markdown
## macOS 安装

### 方式 1：.app Bundle（推荐 GUI 用户）
1. 下载 `Claude-Code-Router-0.1.0-macOS.tar.gz`
2. 解压并拖动到 /Applications
3. 双击启动

### 方式 2：.pkg Installer（推荐开发者）
1. 下载 `claude-code-router-0.1.0-arm64.pkg`
2. 双击安装
3. 命令行工具自动可用
```

---

## 🔄 总结

**Phase 42 提供：**
- 简单的拖拽安装体验
- 独立的 .app bundle（无需 .dmg 包装）
- 可选的 CLI 工具安装
- 便携性（可运行在任意位置）

**适用场景：**
- GUI 为主的用户
- 不想输入管理员密码的用户
- 需要便携安装的场景
- 测试和开发场景

**与 Phase 39 互补：**
- Phase 39 (.pkg): 系统级、命令行友好
- Phase 42 (.app): 用户级、GUI 友好

两种方式共存，满足不同用户需求。
