# Phase 39 — macOS安装包（.pkg）

**状态：** 📋 待实施
**优先级：** P1（发布必需）
**预计时间：** 2-3 小时

---

## 📋 目标

为macOS平台创建专业的.pkg安装包，支持一键安装、自动配置环境变量、卸载等功能。

---

## 🎯 背景

**当前状态：**
- ✅ Rust二进制程序可编译
- ❌ 无安装包
- ❌ 需要手动配置PATH
- ❌ 无卸载机制

**需求：**
- 创建.pkg安装包
- 自动安装到 `/usr/local/bin`
- 自动创建配置目录
- 提供卸载脚本
- 支持代码签名

---

## 📐 实现计划

### Phase 39.1: 创建安装脚本

**时间：** 0.5 小时

**Step 1: preinstall脚本**

```bash
# ccr-rust/packaging/macos/scripts/preinstall
#!/bin/bash
# Backup existing installation if exists
if [ -f "/usr/local/bin/ccr-server" ]; then
    echo "Backing up existing installation..."
    mkdir -p /tmp/ccr-backup
    cp /usr/local/bin/ccr-* /tmp/ccr-backup/ 2>/dev/null || true
fi

# Create necessary directories
mkdir -p ~/.claude-code-router
mkdir -p ~/.claude-code-router/logs
mkdir -p ~/.claude-code-router/backups

exit 0
```

**Step 2: postinstall脚本**

```bash
# ccr-rust/packaging/macos/scripts/postinstall
#!/bin/bash

# Set correct permissions
chmod +x /usr/local/bin/ccr-server
chmod +x /usr/local/bin/ccr-cli
chmod +x /usr/local/bin/ccr-ui

# Create default config if not exists
if [ ! -f ~/.claude-code-router/config.json ]; then
    cat > ~/.claude-code-router/config.json <<'EOF'
{
  "Providers": [],
  "Router": {
    "default": "openai,gpt-4o"
  }
}
EOF
fi

# Add to PATH if not already added
SHELL_RC=""
if [ -n "$ZSH_VERSION" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -n "$BASH_VERSION" ]; then
    SHELL_RC="$HOME/.bash_profile"
fi

if [ -n "$SHELL_RC" ] && [ -f "$SHELL_RC" ]; then
    if ! grep -q "/usr/local/bin" "$SHELL_RC"; then
        echo 'export PATH="/usr/local/bin:$PATH"' >> "$SHELL_RC"
    fi
fi

echo "✅ Claude Code Router installed successfully!"
echo "Run 'ccr --help' to get started"

exit 0
```

---

### Phase 39.2: 创建卸载脚本

**时间：** 0.5 小时

```bash
# ccr-rust/packaging/macos/scripts/uninstall.sh
#!/bin/bash

echo "Uninstalling Claude Code Router..."

# Stop running services
ccr stop 2>/dev/null || true

# Remove binaries
rm -f /usr/local/bin/ccr-server
rm -f /usr/local/bin/ccr-cli
rm -f /usr/local/bin/ccr-ui
rm -f /usr/local/bin/ccr

# Ask user if they want to remove config
read -p "Remove configuration files? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf ~/.claude-code-router
    echo "Configuration removed"
fi

echo "✅ Claude Code Router uninstalled"
```

---

### Phase 39.3: pkgbuild配置

**时间：** 1 小时

**Step 1: 创建Distribution.xml**

```xml
<!-- ccr-rust/packaging/macos/Distribution.xml -->
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="1">
    <title>Claude Code Router</title>
    <organization>com.musistudio.ccr</organization>
    <domains enable_localSystem="true"/>
    <options customize="never" require-scripts="false" hostArchitectures="arm64,x86_64"/>

    <welcome file="welcome.html" mime-type="text/html"/>
    <license file="LICENSE"/>
    <conclusion file="conclusion.html" mime-type="text/html"/>

    <pkg-ref id="com.musistudio.ccr">
        <bundle-version>
            <bundle id="com.musistudio.ccr" CFBundleVersion="0.1.0"/>
        </bundle-version>
    </pkg-ref>

    <choices-outline>
        <line choice="default">
            <line choice="com.musistudio.ccr"/>
        </line>
    </choices-outline>

    <choice id="default"/>
    <choice id="com.musistudio.ccr" visible="false">
        <pkg-ref id="com.musistudio.ccr"/>
    </choice>
</installer-gui-script>
```

**Step 2: 创建构建脚本**

```bash
# ccr-rust/packaging/macos/build-pkg.sh
#!/bin/bash
set -e

VERSION="0.1.0"
ARCH=$(uname -m)
PKG_NAME="claude-code-router-${VERSION}-${ARCH}.pkg"

echo "Building macOS package: $PKG_NAME"

# Build release binaries
echo "Building release binaries..."
cargo build --release --workspace

# Create package structure
BUILD_DIR="build/macos"
PAYLOAD_DIR="${BUILD_DIR}/payload"
SCRIPTS_DIR="${BUILD_DIR}/scripts"

mkdir -p "${PAYLOAD_DIR}/usr/local/bin"
mkdir -p "${SCRIPTS_DIR}"

# Copy binaries
cp target/release/ccr-server "${PAYLOAD_DIR}/usr/local/bin/"
cp target/release/ccr-cli "${PAYLOAD_DIR}/usr/local/bin/"
cp target/release/ccr-ui "${PAYLOAD_DIR}/usr/local/bin/"

# Copy scripts
cp packaging/macos/scripts/preinstall "${SCRIPTS_DIR}/"
cp packaging/macos/scripts/postinstall "${SCRIPTS_DIR}/"
chmod +x "${SCRIPTS_DIR}"/*

# Build component package
pkgbuild --root "${PAYLOAD_DIR}" \
         --scripts "${SCRIPTS_DIR}" \
         --identifier com.musistudio.ccr \
         --version "${VERSION}" \
         --install-location / \
         "${BUILD_DIR}/ccr-component.pkg"

# Build product archive
productbuild --distribution packaging/macos/Distribution.xml \
             --package-path "${BUILD_DIR}" \
             --resources packaging/macos/resources \
             "dist/${PKG_NAME}"

echo "✅ Package created: dist/${PKG_NAME}"

# Optional: Sign package
if [ -n "$SIGNING_IDENTITY" ]; then
    echo "Signing package..."
    productsign --sign "$SIGNING_IDENTITY" \
                "dist/${PKG_NAME}" \
                "dist/${PKG_NAME%.pkg}-signed.pkg"
    mv "dist/${PKG_NAME%.pkg}-signed.pkg" "dist/${PKG_NAME}"
    echo "✅ Package signed"
fi
```

---

### Phase 39.4: 资源文件

**时间：** 0.5 小时

**welcome.html:**
```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; }
        h1 { color: #007AFF; }
    </style>
</head>
<body>
    <h1>Welcome to Claude Code Router</h1>
    <p>This will install Claude Code Router on your Mac.</p>
    <p>Claude Code Router is a tool that routes Claude Code requests to different LLM providers.</p>
</body>
</html>
```

**conclusion.html:**
```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; }
        h1 { color: #34C759; }
        code { background: #f5f5f5; padding: 2px 6px; border-radius: 3px; }
    </style>
</head>
<body>
    <h1>Installation Complete!</h1>
    <p>Claude Code Router has been installed successfully.</p>
    <h2>Next Steps:</h2>
    <ol>
        <li>Open a new terminal window</li>
        <li>Run <code>ccr --help</code> to see available commands</li>
        <li>Run <code>ccr start</code> to start the server</li>
    </ol>
</body>
</html>
```

---

## 🧪 测试计划

### 本地测试

```bash
# 1. 构建安装包
cd ccr-rust
./packaging/macos/build-pkg.sh

# 2. 安装测试
sudo installer -pkg dist/claude-code-router-*.pkg -target /

# 3. 验证安装
which ccr-server
ccr --version

# 4. 功能测试
ccr start
ccr status
ccr stop

# 5. 卸载测试
sudo bash packaging/macos/scripts/uninstall.sh
```

### 签名测试（需要Apple Developer账号）

```bash
# 导入证书
security import developer-cert.p12 -k ~/Library/Keychains/login.keychain

# 构建并签名
SIGNING_IDENTITY="Developer ID Installer: Your Name" \
./packaging/macos/build-pkg.sh

# 验证签名
pkgutil --check-signature dist/claude-code-router-*.pkg
```

---

## 📊 完成标准

- [ ] preinstall脚本创建
- [ ] postinstall脚本创建
- [ ] uninstall脚本创建
- [ ] Distribution.xml配置
- [ ] build-pkg.sh脚本
- [ ] welcome/conclusion资源
- [ ] 本地安装测试通过
- [ ] 卸载测试通过
- [ ] 文档更新

---

## 📝 使用示例

### 开发者构建

```bash
cd ccr-rust
./packaging/macos/build-pkg.sh
```

### 用户安装

```bash
# 下载.pkg文件
curl -L -o ccr.pkg https://github.com/.../claude-code-router-v0.1.0-arm64.pkg

# 安装
sudo installer -pkg ccr.pkg -target /

# 验证
ccr --version
```

### 卸载

```bash
sudo bash /usr/local/share/ccr/uninstall.sh
```

---

## 📁 目录结构

```
ccr-rust/
├── packaging/
│   └── macos/
│       ├── Distribution.xml
│       ├── build-pkg.sh
│       ├── scripts/
│       │   ├── preinstall
│       │   ├── postinstall
│       │   └── uninstall.sh
│       └── resources/
│           ├── welcome.html
│           ├── conclusion.html
│           └── LICENSE
├── build/
│   └── macos/
│       ├── payload/
│       ├── scripts/
│       └── ccr-component.pkg
└── dist/
    └── claude-code-router-*.pkg
```

---

## 🔄 后续工作

- Phase 40: Windows MSI安装包
- Phase 41: 系统托盘支持

---

**创建时间:** 2026-04-28
**预计开始:** TBD
**预计完成:** TBD
