# AetherShell Browser Extension Build Script
# Builds WASM module and prepares extension for loading

param(
    [switch]$Release,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent (Split-Path -Parent $ScriptDir)
$WasmDir = "$ScriptDir\wasm"
$WebPkgDir = "$RootDir\web\pkg"

Write-Host "🔨 AetherShell Browser Extension Builder" -ForegroundColor Cyan
Write-Host ""

# Clean if requested
if ($Clean) {
    Write-Host "🧹 Cleaning build artifacts..." -ForegroundColor Yellow
    if (Test-Path $WasmDir) {
        Remove-Item -Recurse -Force $WasmDir
    }
    Write-Host "✓ Clean complete" -ForegroundColor Green
    exit 0
}

# Build WASM module
Write-Host "📦 Building WASM module..." -ForegroundColor Yellow

# Strip absolute source paths out of the artifact. Without this, rustc bakes
# panic-location strings such as
#   C:\Users\<you>\.cargo\registry\src\...\serde_json-1.0.149\src\error.rs
# into the .wasm, publishing the building developer's username to anyone who
# loads the extension. The generated wasm/ output used to be committed, which
# is how one developer's home directory ended up in the repository.
#
# Cargo's [profile] trim-paths would be the tidy fix, but it is still unstable
# as of Cargo 1.97, so remap the prefixes directly. CARGO_ENCODED_RUSTFLAGS is
# used rather than RUSTFLAGS because it is separated by 0x1f instead of spaces,
# so it survives paths containing spaces.
$Unit = [char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS = @(
    "--remap-path-prefix=$env:USERPROFILE=~"
    "--remap-path-prefix=$RootDir=."
) -join $Unit

Push-Location "$RootDir\web"
try {
    if ($Release) {
        wasm-pack build --target web --release
    }
    else {
        wasm-pack build --target web --dev
    }
}
finally {
    Pop-Location
}

if (!(Test-Path $WebPkgDir)) {
    Write-Host "❌ WASM build failed - pkg directory not found" -ForegroundColor Red
    exit 1
}

Write-Host "✓ WASM build complete" -ForegroundColor Green
Write-Host ""

# Create wasm directory in extension
Write-Host "📁 Copying WASM files to extension..." -ForegroundColor Yellow
if (!(Test-Path $WasmDir)) {
    New-Item -ItemType Directory -Path $WasmDir | Out-Null
}

# Copy WASM files
$FilesToCopy = @(
    "aether_wasm.js",
    "aether_wasm_bg.wasm",
    "aether_wasm.d.ts"
)

foreach ($File in $FilesToCopy) {
    $Source = "$WebPkgDir\$File"
    $Dest = "$WasmDir\$File"
    if (Test-Path $Source) {
        Copy-Item $Source $Dest -Force
        Write-Host "  ✓ Copied $File" -ForegroundColor DarkGreen
    }
    else {
        Write-Host "  ⚠ Missing $File" -ForegroundColor Yellow
    }
}

Write-Host ""
Write-Host "✅ Build complete!" -ForegroundColor Green
Write-Host ""
Write-Host "To load the extension in Chrome:" -ForegroundColor White
Write-Host "  1. Open chrome://extensions/" -ForegroundColor Gray
Write-Host "  2. Enable 'Developer mode'" -ForegroundColor Gray
Write-Host "  3. Click 'Load unpacked'" -ForegroundColor Gray
Write-Host "  4. Select: $ScriptDir" -ForegroundColor Gray
Write-Host ""
Write-Host "To load in Firefox:" -ForegroundColor White
Write-Host "  1. Open about:debugging" -ForegroundColor Gray
Write-Host "  2. Click 'This Firefox'" -ForegroundColor Gray
Write-Host "  3. Click 'Load Temporary Add-on'" -ForegroundColor Gray
Write-Host "  4. Select: $ScriptDir\manifest.json" -ForegroundColor Gray
