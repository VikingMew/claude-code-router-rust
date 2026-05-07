# Phase 40 — Windows MSI安装包

**状态：** 📋 待实施
**优先级：** P1（发布必需）
**预计时间：** 3-4 小时

---

## 📋 目标

为Windows平台创建专业的.msi安装包，支持一键安装、自动配置环境变量、开始菜单快捷方式、卸载等功能。

---

## 🎯 背景

**当前状态：**
- ✅ Rust二进制程序可编译（Windows）
- ❌ 无安装包
- ❌ 需要手动配置PATH
- ❌ 无卸载机制

**需求：**
- 创建.msi安装包
- 自动安装到 `C:\Program Files\Claude Code Router`
- 自动添加到PATH
- 创建开始菜单快捷方式
- 支持图形化卸载
- 支持代码签名

---

## 📐 实现计划

### Phase 40.1: WiX Toolset配置

**时间：** 1 小时

**Step 1: 安装WiX Toolset**

```powershell
# 安装WiX Toolset
winget install WiX.Toolset

# 或通过Chocolatey
choco install wixtoolset
```

**Step 2: 创建Product.wxs**

```xml
<!-- ccr-rust/packaging/windows/Product.wxs -->
<?xml version='1.0' encoding='windows-1252'?>
<Wix xmlns='http://schemas.microsoft.com/wix/2006/wi'>
  <Product Name='Claude Code Router'
           Id='*'
           UpgradeCode='12345678-1234-1234-1234-123456789012'
           Language='1033'
           Codepage='1252'
           Version='0.1.0'
           Manufacturer='MusiStudio'>

    <Package Id='*'
             Keywords='Installer'
             Description='Claude Code Router Installer'
             Comments='Claude Code Router is a tool that routes Claude Code requests to different LLM providers'
             Manufacturer='MusiStudio'
             InstallerVersion='500'
             Languages='1033'
             Compressed='yes'
             SummaryCodepage='1252'
             InstallScope='perMachine'/>

    <MajorUpgrade DowngradeErrorMessage='A newer version is already installed.' />

    <Media Id='1' Cabinet='ccr.cab' EmbedCab='yes' />

    <!-- Installation directory -->
    <Directory Id='TARGETDIR' Name='SourceDir'>
      <Directory Id='ProgramFiles64Folder'>
        <Directory Id='INSTALLDIR' Name='Claude Code Router'>
          <Component Id='MainExecutables' Guid='*'>
            <!-- Binaries -->
            <File Id='CcrServerExe' Name='ccr-server.exe' Source='..\..\target\release\ccr-server.exe' KeyPath='yes'/>
            <File Id='CcrCliExe' Name='ccr-cli.exe' Source='..\..\target\release\ccr-cli.exe'/>
            <File Id='CcrUiExe' Name='ccr-ui.exe' Source='..\..\target\release\ccr-ui.exe'/>

            <!-- Add to PATH -->
            <Environment Id='PATH'
                        Name='PATH'
                        Value='[INSTALLDIR]'
                        Permanent='no'
                        Part='last'
                        Action='set'
                        System='yes'/>
          </Component>

          <!-- Uninstaller script -->
          <Component Id='UninstallScript' Guid='*'>
            <File Id='UninstallBat' Name='uninstall.bat' Source='scripts\uninstall.bat'/>
          </Component>
        </Directory>
      </Directory>

      <!-- Start Menu -->
      <Directory Id='ProgramMenuFolder'>
        <Directory Id='ApplicationProgramsFolder' Name='Claude Code Router'>
          <Component Id='StartMenuShortcuts' Guid='*'>
            <Shortcut Id='CcrUiShortcut'
                     Name='Claude Code Router'
                     Description='Launch Claude Code Router UI'
                     Target='[INSTALLDIR]ccr-ui.exe'
                     WorkingDirectory='INSTALLDIR'/>

            <Shortcut Id='UninstallShortcut'
                     Name='Uninstall Claude Code Router'
                     Description='Uninstalls Claude Code Router'
                     Target='[SystemFolder]msiexec.exe'
                     Arguments='/x [ProductCode]'/>

            <RemoveFolder Id='ApplicationProgramsFolder' On='uninstall'/>
            <RegistryValue Root='HKCU'
                          Key='Software\MusiStudio\ClaudeCodeRouter'
                          Name='installed'
                          Type='integer'
                          Value='1'
                          KeyPath='yes'/>
          </Component>
        </Directory>
      </Directory>

      <!-- User config directory -->
      <Directory Id='AppDataFolder'>
        <Directory Id='UserConfigDir' Name='claude-code-router'>
          <Component Id='ConfigDirectory' Guid='*'>
            <CreateFolder>
              <Permission User='Everyone' GenericAll='yes'/>
            </CreateFolder>
            <RegistryValue Root='HKCU'
                          Key='Software\MusiStudio\ClaudeCodeRouter'
                          Name='ConfigDir'
                          Type='string'
                          Value='[UserConfigDir]'
                          KeyPath='yes'/>
          </Component>
        </Directory>
      </Directory>
    </Directory>

    <!-- Features -->
    <Feature Id='Complete' Level='1'>
      <ComponentRef Id='MainExecutables'/>
      <ComponentRef Id='UninstallScript'/>
      <ComponentRef Id='StartMenuShortcuts'/>
      <ComponentRef Id='ConfigDirectory'/>
    </Feature>

    <!-- UI -->
    <UIRef Id='WixUI_InstallDir'/>
    <Property Id='WIXUI_INSTALLDIR' Value='INSTALLDIR'/>

    <!-- License -->
    <WixVariable Id='WixUILicenseRtf' Value='resources\License.rtf'/>

    <!-- Banner/Dialog images -->
    <WixVariable Id='WixUIBannerBmp' Value='resources\banner.bmp'/>
    <WixVariable Id='WixUIDialogBmp' Value='resources\dialog.bmp'/>

    <!-- Custom actions -->
    <CustomAction Id='CreateDefaultConfig'
                  Directory='INSTALLDIR'
                  ExeCommand='cmd /c if not exist "%USERPROFILE%\.claude-code-router\config.json" (mkdir "%USERPROFILE%\.claude-code-router" &amp; echo {"Providers":[],"Router":{"default":"openai,gpt-4o"}} > "%USERPROFILE%\.claude-code-router\config.json")'
                  Execute='deferred'
                  Return='ignore'
                  Impersonate='yes'/>

    <InstallExecuteSequence>
      <Custom Action='CreateDefaultConfig' After='InstallFiles'>NOT Installed</Custom>
    </InstallExecuteSequence>
  </Product>
</Wix>
```

---

### Phase 40.2: 创建构建脚本

**时间：** 0.5 小时

**PowerShell构建脚本：**

```powershell
# ccr-rust/packaging/windows/build-msi.ps1
param(
    [string]$Version = "0.1.0",
    [string]$SigningCert = ""
)

$ErrorActionPreference = "Stop"

Write-Host "Building Windows MSI installer (Version: $Version)" -ForegroundColor Green

# Build release binaries
Write-Host "Building release binaries..." -ForegroundColor Cyan
cargo build --release --workspace

# Check WiX Toolset
$wixPath = "${env:WIX}bin"
if (-not (Test-Path $wixPath)) {
    Write-Error "WiX Toolset not found. Please install from https://wixtoolset.org/"
    exit 1
}

# Set environment
$env:Path = "$wixPath;$env:Path"

# Create output directory
$outputDir = "dist"
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

# Compile WiX source
Write-Host "Compiling WiX source..." -ForegroundColor Cyan
candle.exe packaging/windows/Product.wxs -o build/windows/Product.wixobj

if ($LASTEXITCODE -ne 0) {
    Write-Error "candle.exe failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

# Link MSI
Write-Host "Linking MSI..." -ForegroundColor Cyan
$msiFile = "claude-code-router-$Version-x64.msi"
light.exe build/windows/Product.wixobj `
    -ext WixUIExtension `
    -cultures:en-US `
    -o "$outputDir/$msiFile"

if ($LASTEXITCODE -ne 0) {
    Write-Error "light.exe failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

Write-Host "✅ MSI created: $outputDir/$msiFile" -ForegroundColor Green

# Optional: Sign MSI
if ($SigningCert) {
    Write-Host "Signing MSI..." -ForegroundColor Cyan
    signtool.exe sign /f $SigningCert /t http://timestamp.digicert.com "$outputDir/$msiFile"

    if ($LASTEXITCODE -eq 0) {
        Write-Host "✅ MSI signed successfully" -ForegroundColor Green
    } else {
        Write-Warning "Failed to sign MSI"
    }
}

Write-Host "Done!" -ForegroundColor Green
```

**Batch构建脚本（可选）：**

```batch
@echo off
REM ccr-rust/packaging/windows/build-msi.bat
setlocal

echo Building Windows MSI installer...

REM Build release
cargo build --release --workspace
if errorlevel 1 goto :error

REM Set WiX path
set PATH=%WIX%bin;%PATH%

REM Compile
candle.exe packaging\windows\Product.wxs -o build\windows\Product.wixobj
if errorlevel 1 goto :error

REM Link
light.exe build\windows\Product.wixobj -ext WixUIExtension -cultures:en-US -o dist\claude-code-router.msi
if errorlevel 1 goto :error

echo ✅ MSI created successfully
goto :end

:error
echo ❌ Build failed
exit /b 1

:end
```

---

### Phase 40.3: 卸载脚本

**时间：** 0.5 小时

```batch
@echo off
REM ccr-rust/packaging/windows/scripts/uninstall.bat

echo Uninstalling Claude Code Router...

REM Stop running services
ccr stop 2>NUL

REM Remove from PATH (optional, MSI handles this)
REM ...

echo.
echo ✅ Claude Code Router has been removed
echo.
echo Configuration files are preserved at:
echo %USERPROFILE%\.claude-code-router
echo.
echo To completely remove configuration, delete the above directory.
pause
```

---

### Phase 40.4: 资源文件

**时间：** 1 小时

**License.rtf:**
```rtf
{\rtf1\ansi\deff0
{\fonttbl{\f0 Arial;}}
\f0\fs20
MIT License

Copyright (c) 2026 MusiStudio

Permission is hereby granted, free of charge...
}
```

**创建图标/图片：**
- `banner.bmp` (493x58 pixels) - 安装向导顶部横幅
- `dialog.bmp` (493x312 pixels) - 安装向导对话框背景
- `icon.ico` - 应用程序图标

---

## 🧪 测试计划

### 本地测试

```powershell
# 1. 构建MSI
cd ccr-rust
.\packaging\windows\build-msi.ps1

# 2. 安装测试（需要管理员权限）
msiexec /i dist\claude-code-router-0.1.0-x64.msi /l*v install.log

# 3. 验证安装
where ccr-server
ccr --version

# 4. 检查开始菜单
# 应该能看到 "Claude Code Router" 程序组

# 5. 功能测试
ccr start
ccr status
ccr stop

# 6. 卸载测试（通过控制面板或）
msiexec /x dist\claude-code-router-0.1.0-x64.msi
```

### 签名测试（需要Code Signing证书）

```powershell
# 签名MSI
.\packaging\windows\build-msi.ps1 -SigningCert "path\to\cert.pfx"

# 验证签名
signtool.exe verify /pa dist\claude-code-router-*.msi
```

---

## 📊 完成标准

- [ ] Product.wxs WiX配置
- [ ] build-msi.ps1脚本
- [ ] uninstall.bat脚本
- [ ] License.rtf创建
- [ ] 图标/图片资源
- [ ] 本地安装测试通过
- [ ] PATH自动配置测试
- [ ] 开始菜单快捷方式验证
- [ ] 卸载测试通过
- [ ] 文档更新

---

## 📝 使用示例

### 开发者构建

```powershell
cd ccr-rust
.\packaging\windows\build-msi.ps1
```

### 用户安装

```powershell
# 下载MSI
# 双击安装或使用命令行

# 静默安装
msiexec /i claude-code-router-0.1.0-x64.msi /quiet

# 交互式安装
msiexec /i claude-code-router-0.1.0-x64.msi
```

### 卸载

```powershell
# 通过控制面板
# 或使用命令行
msiexec /x {ProductCode}
```

---

## 📁 目录结构

```
ccr-rust/
├── packaging/
│   └── windows/
│       ├── Product.wxs
│       ├── build-msi.ps1
│       ├── build-msi.bat
│       ├── scripts/
│       │   └── uninstall.bat
│       └── resources/
│           ├── License.rtf
│           ├── banner.bmp
│           ├── dialog.bmp
│           └── icon.ico
├── build/
│   └── windows/
│       └── Product.wixobj
└── dist/
    └── claude-code-router-*.msi
```

---

## 🔧 高级功能（可选）

### 1. 自定义对话框

```xml
<UI>
  <Dialog Id='CustomDlg' Width='370' Height='270' Title='Custom Settings'>
    <Control Id='PortLabel' Type='Text' X='20' Y='60' Width='100' Height='17' Text='Server Port:'/>
    <Control Id='PortEdit' Type='Edit' X='130' Y='58' Width='100' Height='18' Property='SERVER_PORT'/>
  </Dialog>
</UI>
```

### 2. 服务注册

```xml
<ServiceInstall Id='CcrService'
               Name='ClaudeCodeRouter'
               DisplayName='Claude Code Router'
               Description='Routes Claude Code requests'
               Type='ownProcess'
               Start='auto'
               ErrorControl='normal'/>
```

### 3. 防火墙规则

```xml
<CustomAction Id='AddFirewallRule'
              Directory='INSTALLDIR'
              ExeCommand='netsh advfirewall firewall add rule name="CCR Server" dir=in action=allow program="[INSTALLDIR]ccr-server.exe"'
              Execute='deferred'
              Impersonate='no'/>
```

---

## 🔄 后续工作

- Phase 41: 系统托盘支持

---

**创建时间:** 2026-04-28
**预计开始:** TBD
**预计完成:** TBD
