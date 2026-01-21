#!/usr/bin/env pwsh
# AetherShell Test Coverage Runner
# Run all coverage tests and report results

param(
    [switch]$Verbose,
    [switch]$RustOnly,
    [switch]$AeOnly
)

$ErrorActionPreference = "Continue"
$script:passed = 0
$script:failed = 0
$script:failedTests = @()

Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║           AetherShell Test Coverage Runner                    ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# Ensure we're in the project root
$projectRoot = $PSScriptRoot | Split-Path -Parent
Set-Location $projectRoot

# Build first
Write-Host "Building..." -ForegroundColor Yellow
cargo build --quiet 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "Build successful" -ForegroundColor Green
Write-Host ""

# Run Rust tests
if (-not $AeOnly) {
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host " RUST TESTS" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    
    $rustTests = @(
        "builtins_coverage",
        "theme_coverage",
        "builtins",
        "eval",
        "pipeline",
        "smoke",
        "typecheck"
    )
    
    foreach ($test in $rustTests) {
        $output = cargo test --test $test 2>&1 | Out-String
        if ($output -match "(\d+) passed") {
            $passCount = $Matches[1]
            Write-Host "  ✓ $test ($passCount tests passed)" -ForegroundColor Green
            $script:passed += [int]$passCount
        }
        if ($output -match "(\d+) failed" -and $Matches[1] -ne "0") {
            $failCount = $Matches[1]
            Write-Host "  ✗ $test ($failCount tests failed)" -ForegroundColor Red
            $script:failed += [int]$failCount
            $script:failedTests += $test
        }
        if ($Verbose) {
            Write-Host $output -ForegroundColor Gray
        }
    }
    Write-Host ""
}

# Run .ae tests
if (-not $RustOnly) {
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host " .AE COVERAGE TESTS" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    
    $aeFiles = Get-ChildItem "tests/coverage/*.ae"
    foreach ($file in $aeFiles) {
        $output = & ".\target\debug\ae.exe" $file.FullName 2>&1 | Out-String
        if ($LASTEXITCODE -eq 0 -and $output -match "COMPLETE") {
            # Count passed checks by looking for the success marker (may appear garbled in PowerShell)
            $testCount = ([regex]::Matches($output, "Γ£ô")).Count
            if ($testCount -eq 0) {
                # Fallback: count lines with success indicators  
                $testCount = ([regex]::Matches($output, "returns|works|=\s*\d")).Count
            }
            Write-Host "  ✓ $($file.Name) (passed)" -ForegroundColor Green
            $script:passed += 1
        }
        else {
            Write-Host "  ✗ $($file.Name) - FAILED" -ForegroundColor Red
            $script:failed += 1
            $script:failedTests += $file.Name
            if ($Verbose) {
                Write-Host $output -ForegroundColor Gray
            }
        }
    }
    Write-Host ""
}

# Summary
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " SUMMARY" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Total Passed: $script:passed" -ForegroundColor Green
Write-Host "  Total Failed: $script:failed" -ForegroundColor $(if ($script:failed -gt 0) { "Red" } else { "Green" })

if ($script:failedTests.Count -gt 0) {
    Write-Host ""
    Write-Host "  Failed Tests:" -ForegroundColor Red
    foreach ($t in $script:failedTests) {
        Write-Host "    - $t" -ForegroundColor Red
    }
}

Write-Host ""
if ($script:failed -eq 0) {
    Write-Host "  ✓ ALL TESTS PASSED!" -ForegroundColor Green
    exit 0
}
else {
    Write-Host "  ✗ SOME TESTS FAILED" -ForegroundColor Red
    exit 1
}
