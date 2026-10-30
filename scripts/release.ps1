# IronShell Release Script
# Usage: .\scripts\release.ps1 -Version "13.0.0" [-DryRun]

param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

Write-Host " IronShell Release Script" -ForegroundColor Cyan
Write-Host "   Version: $Version" -ForegroundColor Gray
Write-Host ""

# Validate version format
if ($Version -notmatch '\A(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\z') {
    Write-Host " Invalid version format. Use semantic versioning (e.g., 0.2.0)" -ForegroundColor Red
    exit 1
}

# Check we're on master/main
$repositoryRoot = git rev-parse --show-toplevel
if ($LASTEXITCODE -ne 0 -or -not $repositoryRoot) { throw 'Cannot determine the Git repository root.' }
$expectedRoot = (Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$actualRoot = (Resolve-Path -LiteralPath $repositoryRoot).Path
if ($actualRoot -ine $expectedRoot) { throw 'The current Git repository does not contain this release script.' }
$branch = git branch --show-current
if ($LASTEXITCODE -ne 0) { throw 'Cannot read the current Git branch.' }
if ($branch -notin @('master', 'main')) {
    Write-Host " Must be on master or main branch (currently on: $branch)" -ForegroundColor Red
    exit 1
}

# Check for uncommitted changes
$status = git status --porcelain
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect the working tree.' }
if ($status) {
    Write-Host " Uncommitted changes detected. Please commit or stash them." -ForegroundColor Red
    git status --short
    exit 1
}

# Match the root package version, not a dependency's version or a regex
# wildcard (the dots in a version must not match arbitrary characters).
$cargoToml = Get-Content -LiteralPath (Join-Path $expectedRoot 'Cargo.toml') -Raw
$packageSection = [regex]::Match($cargoToml, '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)').Groups[1].Value
$packageVersion = [regex]::Match($packageSection, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if ($packageVersion -cne $Version) {
    Write-Host "  Version in Cargo.toml doesn't match $Version" -ForegroundColor Yellow
    Write-Host "   Update Cargo.toml version before releasing." -ForegroundColor Yellow
    exit 1
}

git show-ref --verify --quiet "refs/tags/v$Version"
if ($LASTEXITCODE -eq 0) { throw "Tag v$Version already exists; prepare a new version." }
if ($LASTEXITCODE -ne 1) { throw 'Cannot inspect existing Git tags.' }

Write-Host " Pre-flight checks passed" -ForegroundColor Green
Write-Host ""

if ($DryRun) {
    Write-Host " DRY RUN - Would execute:" -ForegroundColor Yellow
    Write-Host "   git tag v$Version" -ForegroundColor Gray
    Write-Host "   git push origin v$Version" -ForegroundColor Gray
    Write-Host ""
    Write-Host "This would trigger:" -ForegroundColor Yellow
    Write-Host "    GitHub Release with binaries for 6 platforms" -ForegroundColor Gray
    Write-Host "    WASM package build" -ForegroundColor Gray
    Write-Host "    Published GitHub Release (not a draft), created before builds finish" -ForegroundColor Gray
    Write-Host "    Registry publishing only where the workflow's credential preflight succeeds" -ForegroundColor Gray
    exit 0
}

Write-Host " Creating tag v$Version..." -ForegroundColor Yellow
git tag "v$Version"
if ($LASTEXITCODE -ne 0) { throw "Failed to create tag v$Version; nothing was pushed." }

Write-Host " Pushing tag to origin..." -ForegroundColor Yellow
git push origin "v$Version"
if ($LASTEXITCODE -ne 0) { throw "Failed to push tag v$Version; release initiation is not confirmed." }

Write-Host ""
Write-Host " Release v$Version initiated!" -ForegroundColor Green
Write-Host ""
Write-Host "GitHub Actions will now:" -ForegroundColor White
Write-Host "   1. Build binaries for Linux, macOS, Windows (x64 + ARM64)" -ForegroundColor Gray
Write-Host "   2. Build WASM package" -ForegroundColor Gray
Write-Host "   3. Publish a GitHub Release and attach completed build artifacts" -ForegroundColor Gray
Write-Host "   4. Attempt crates.io, npm and PyPI publishing only when their preflight allows it" -ForegroundColor Gray
Write-Host "   5. Verify registry publishing outcomes separately from build success" -ForegroundColor Gray
Write-Host ""
Write-Host "Monitor progress at:" -ForegroundColor White
Write-Host "   https://github.com/nervosys/AetherShell/actions" -ForegroundColor Cyan
Write-Host ""
Write-Host "Review the published release and artifact completeness at:" -ForegroundColor White
Write-Host "   https://github.com/nervosys/AetherShell/releases" -ForegroundColor Cyan
