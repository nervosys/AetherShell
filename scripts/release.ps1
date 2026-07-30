# AetherShell Release Script
# Usage: .\scripts\release.ps1 -Version "0.2.0" [-DryRun]

param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

Write-Host "🚀 AetherShell Release Script" -ForegroundColor Cyan
Write-Host "   Version: $Version" -ForegroundColor Gray
Write-Host ""

# Validate version format
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    Write-Host "❌ Invalid version format. Use semantic versioning (e.g., 0.2.0)" -ForegroundColor Red
    exit 1
}

# Check we're on master/main
$branch = git branch --show-current
if ($branch -notin @('master', 'main')) {
    Write-Host "❌ Must be on master or main branch (currently on: $branch)" -ForegroundColor Red
    exit 1
}

# Check for uncommitted changes
$status = git status --porcelain
if ($status) {
    Write-Host "❌ Uncommitted changes detected. Please commit or stash them." -ForegroundColor Red
    git status --short
    exit 1
}

# Verify version in Cargo.toml
$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -notmatch "version = `"$Version`"") {
    Write-Host "⚠️  Version in Cargo.toml doesn't match $Version" -ForegroundColor Yellow
    Write-Host "   Update Cargo.toml version before releasing." -ForegroundColor Yellow
    exit 1
}

Write-Host "✓ Pre-flight checks passed" -ForegroundColor Green
Write-Host ""

if ($DryRun) {
    Write-Host "🔍 DRY RUN - Would execute:" -ForegroundColor Yellow
    Write-Host "   git tag v$Version" -ForegroundColor Gray
    Write-Host "   git push origin v$Version" -ForegroundColor Gray
    Write-Host ""
    Write-Host "This would trigger:" -ForegroundColor Yellow
    Write-Host "   • GitHub Release with binaries for 6 platforms" -ForegroundColor Gray
    Write-Host "   • WASM package build" -ForegroundColor Gray
    Write-Host "   • crates.io publish" -ForegroundColor Gray
    Write-Host "   • npm publish (WASM)" -ForegroundColor Gray
    exit 0
}

Write-Host "📝 Creating tag v$Version..." -ForegroundColor Yellow
git tag "v$Version"

Write-Host "📤 Pushing tag to origin..." -ForegroundColor Yellow
git push origin "v$Version"

Write-Host ""
Write-Host "✅ Release v$Version initiated!" -ForegroundColor Green
Write-Host ""
Write-Host "GitHub Actions will now:" -ForegroundColor White
Write-Host "   1. Build binaries for Linux, macOS, Windows (x64 + ARM64)" -ForegroundColor Gray
Write-Host "   2. Build WASM package" -ForegroundColor Gray
Write-Host "   3. Create GitHub Release (draft)" -ForegroundColor Gray
Write-Host "   4. Publish to crates.io" -ForegroundColor Gray
Write-Host "   5. Publish to npm" -ForegroundColor Gray
Write-Host ""
Write-Host "Monitor progress at:" -ForegroundColor White
Write-Host "   https://github.com/nervosys/AetherShell/actions" -ForegroundColor Cyan
Write-Host ""
Write-Host "After workflows complete, review and publish the draft release at:" -ForegroundColor White
Write-Host "   https://github.com/nervosys/AetherShell/releases" -ForegroundColor Cyan
