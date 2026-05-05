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
