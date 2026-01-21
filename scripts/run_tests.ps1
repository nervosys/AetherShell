# AetherShell Test Runner Script
# Run all .ae test files and report results

param (
    [string]$TestDir = "tests",
    [string]$Pattern = "*.ae",
    [switch]$Verbose,
    [switch]$CoverageOnly
)

$ErrorActionPreference = "Continue"

# Colors for output
function Write-Success { param($msg) Write-Host $msg -ForegroundColor Green }
function Write-Failure { param($msg) Write-Host $msg -ForegroundColor Red }
function Write-Info { param($msg) Write-Host $msg -ForegroundColor Cyan }

Write-Info "=== AetherShell Test Runner ==="
Write-Info ""

# Build the project first
Write-Info "Building AetherShell..."
$buildResult = cargo build --bin ae 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Failure "Build failed!"
    Write-Host $buildResult
    exit 1
}
Write-Success "Build successful"
Write-Info ""

# Get the ae binary path
$aeBinary = ".\target\debug\ae.exe"
if (-not (Test-Path $aeBinary)) {
    $aeBinary = ".\target\release\ae.exe"
}

if (-not (Test-Path $aeBinary)) {
    Write-Failure "Could not find ae binary!"
    exit 1
}

# Collect test files
$testPaths = @()
if ($CoverageOnly) {
    $testPaths += Get-ChildItem -Path "tests\coverage" -Filter $Pattern -File
}
else {
    # Feature tests
    $testPaths += Get-ChildItem -Path "tests" -Filter "feature_*.ae" -File
    # Coverage tests
    if (Test-Path "tests\coverage") {
        $testPaths += Get-ChildItem -Path "tests\coverage" -Filter $Pattern -File
    }
    # Script tests
    if (Test-Path "tests\scripts\builtins") {
        $testPaths += Get-ChildItem -Path "tests\scripts\builtins" -Filter $Pattern -File
    }
    if (Test-Path "tests\scripts\integration") {
        $testPaths += Get-ChildItem -Path "tests\scripts\integration" -Filter $Pattern -File
    }
}

$totalTests = $testPaths.Count
$passed = 0
$failed = 0
$failedTests = @()

Write-Info "Found $totalTests test files"
Write-Info ""

foreach ($testFile in $testPaths) {
    $testName = $testFile.Name
    $testPath = $testFile.FullName
    
    if ($Verbose) {
        Write-Info "Running: $testName"
    }
    
    # Run the test file
    $output = & $aeBinary $testPath 2>&1
    $exitCode = $LASTEXITCODE
    
    # Check for failures in output
    $hasFailure = $output | Select-String -Pattern "✗|FAILED|Error|error:" -Quiet
    
    if ($exitCode -eq 0 -and -not $hasFailure) {
        $passed++
        if ($Verbose) {
            Write-Success "  ✓ PASSED"
            Write-Host $output
            Write-Host ""
        }
        else {
            Write-Host "  ✓ $testName" -ForegroundColor Green
        }
    }
    else {
        $failed++
        $failedTests += @{
            Name     = $testName
            Path     = $testPath
            Output   = $output
            ExitCode = $exitCode
        }
        Write-Host "  ✗ $testName" -ForegroundColor Red
        if ($Verbose) {
            Write-Host $output
            Write-Host ""
        }
    }
}

Write-Info ""
Write-Info "=== Test Results ==="
Write-Success "Passed: $passed"
if ($failed -gt 0) {
    Write-Failure "Failed: $failed"
    Write-Info ""
    Write-Info "Failed tests:"
    foreach ($test in $failedTests) {
        Write-Failure "  - $($test.Name)"
        if ($Verbose) {
            Write-Host "    Exit code: $($test.ExitCode)"
            Write-Host "    Output: $($test.Output)"
        }
    }
}
else {
    Write-Host "Failed: 0" -ForegroundColor Gray
}

$percentage = [math]::Round(($passed / $totalTests) * 100, 1)
Write-Info ""
Write-Info "Pass rate: $percentage% ($passed/$totalTests)"

# Exit with appropriate code
if ($failed -gt 0) {
    exit 1
}
else {
    exit 0
}
