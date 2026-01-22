# PowerShell build script for Windows
# Build and deploy the AetherShell Package Registry Lambda function

param(
    [switch]$Deploy
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$LambdaDir = Join-Path $ScriptDir "lambda"
$BuildDir = Join-Path $ScriptDir "build"

Write-Host "=== Building Lambda function ===" -ForegroundColor Cyan

# Create build directory
New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null

# Check for cargo-lambda
$cargoLambda = Get-Command cargo-lambda -ErrorAction SilentlyContinue

if ($cargoLambda) {
    Write-Host "Using cargo-lambda for build..."
    Push-Location $LambdaDir
    try {
        cargo lambda build --release --arm64 --output-format zip
        Copy-Item "target/lambda/aethershell-packages-api/bootstrap.zip" "$BuildDir/api.zip"
    } finally {
        Pop-Location
    }
} else {
    Write-Host "ERROR: cargo-lambda is not installed." -ForegroundColor Red
    Write-Host ""
    Write-Host "Install cargo-lambda:"
    Write-Host "  cargo install cargo-lambda"
    Write-Host ""
    Write-Host "Or use WSL/Docker for cross-compilation."
    exit 1
}

Write-Host "=== Build complete: $BuildDir/api.zip ===" -ForegroundColor Green

# Deploy with Terraform if requested
if ($Deploy) {
    Write-Host "=== Deploying with Terraform ===" -ForegroundColor Cyan
    Push-Location $ScriptDir
    try {
        # Copy zip to terraform directory
        Copy-Item "$BuildDir/api.zip" "$ScriptDir/lambda/api.zip"
        
        terraform init
        terraform plan -out=tfplan
        
        $confirm = Read-Host "Apply changes? [y/N]"
        if ($confirm -eq "y" -or $confirm -eq "Y") {
            terraform apply tfplan
        }
        
        Remove-Item tfplan -ErrorAction SilentlyContinue
    } finally {
        Pop-Location
    }
}

Write-Host "=== Done ===" -ForegroundColor Green
