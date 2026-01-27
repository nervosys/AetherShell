# AetherShell Development Setup Script
# Run this after cloning to set up your development environment

$ErrorActionPreference = "Stop"

Write-Host "🔧 AetherShell Development Setup" -ForegroundColor Cyan
Write-Host ""

# Check Rust
Write-Host "Checking Rust installation..." -ForegroundColor Yellow
try {
    $rustVersion = rustc --version
    Write-Host "  ✓ $rustVersion" -ForegroundColor Green
} catch {
    Write-Host "  ❌ Rust not found. Install from https://rustup.rs" -ForegroundColor Red
    exit 1
}

# Check cargo
$cargoVersion = cargo --version
Write-Host "  ✓ $cargoVersion" -ForegroundColor Green

# Check for WASM target
Write-Host ""
Write-Host "Checking WASM target..." -ForegroundColor Yellow
$targets = rustup target list --installed
if ($targets -match "wasm32-unknown-unknown") {
    Write-Host "  ✓ wasm32-unknown-unknown target installed" -ForegroundColor Green
} else {
    Write-Host "  Installing wasm32-unknown-unknown target..." -ForegroundColor Yellow
    rustup target add wasm32-unknown-unknown
    Write-Host "  ✓ Target installed" -ForegroundColor Green
}

# Check for wasm-pack
Write-Host ""
Write-Host "Checking wasm-pack..." -ForegroundColor Yellow
try {
    $wasmPackVersion = wasm-pack --version
    Write-Host "  ✓ $wasmPackVersion" -ForegroundColor Green
} catch {
    Write-Host "  Installing wasm-pack..." -ForegroundColor Yellow
    cargo install wasm-pack
    Write-Host "  ✓ wasm-pack installed" -ForegroundColor Green
}

# Build native
Write-Host ""
Write-Host "Building native binary..." -ForegroundColor Yellow
cargo build --features native
Write-Host "  ✓ Native build complete" -ForegroundColor Green

# Build WASM
Write-Host ""
Write-Host "Building WASM module..." -ForegroundColor Yellow
Push-Location web
try {
    wasm-pack build --target web --dev
    Write-Host "  ✓ WASM build complete" -ForegroundColor Green
} finally {
    Pop-Location
}

# Run tests
Write-Host ""
Write-Host "Running tests..." -ForegroundColor Yellow
cargo test --features native --lib 2>&1 | Select-String -Pattern "^test result:"
Write-Host "  ✓ Tests complete" -ForegroundColor Green

# Summary
Write-Host ""
Write-Host "=" * 50 -ForegroundColor Gray
Write-Host "✅ Development environment ready!" -ForegroundColor Green
Write-Host ""
Write-Host "Quick commands:" -ForegroundColor White
Write-Host "  cargo run --features native          # Run REPL" -ForegroundColor Gray
Write-Host "  cargo run --features native -- tui   # Run TUI" -ForegroundColor Gray
Write-Host "  cargo test --features native         # Run tests" -ForegroundColor Gray
Write-Host "  cargo build --release --features native  # Release build" -ForegroundColor Gray
Write-Host ""
Write-Host "Binary location: target/debug/ae.exe" -ForegroundColor Gray
Write-Host ""
