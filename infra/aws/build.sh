#!/usr/bin/env bash
# Build and deploy the AetherShell Package Registry Lambda function
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAMBDA_DIR="${SCRIPT_DIR}/lambda"
BUILD_DIR="${SCRIPT_DIR}/build"

echo "=== Building Lambda function ==="

# Create build directory
mkdir -p "${BUILD_DIR}"

# Build for ARM64 Lambda (requires cross or cargo-lambda)
if command -v cargo-lambda &> /dev/null; then
    echo "Using cargo-lambda for build..."
    cd "${LAMBDA_DIR}"
    cargo lambda build --release --arm64 --output-format zip
    cp target/lambda/aethershell-packages-api/bootstrap.zip "${BUILD_DIR}/api.zip"
elif command -v cross &> /dev/null; then
    echo "Using cross for build..."
    cd "${LAMBDA_DIR}"
    cross build --release --target aarch64-unknown-linux-gnu
    
    # Package the binary
    cd target/aarch64-unknown-linux-gnu/release
    cp aethershell-packages-api bootstrap
    zip "${BUILD_DIR}/api.zip" bootstrap
    rm bootstrap
else
    echo "ERROR: Neither cargo-lambda nor cross is installed."
    echo ""
    echo "Install cargo-lambda (recommended):"
    echo "  cargo install cargo-lambda"
    echo ""
    echo "Or install cross:"
    echo "  cargo install cross"
    echo "  # Requires Docker"
    exit 1
fi

echo "=== Build complete: ${BUILD_DIR}/api.zip ==="

# Deploy with Terraform if requested
if [[ "${1:-}" == "deploy" ]]; then
    echo "=== Deploying with Terraform ==="
    cd "${SCRIPT_DIR}"
    
    # Copy zip to terraform directory
    cp "${BUILD_DIR}/api.zip" "${SCRIPT_DIR}/lambda/api.zip"
    
    terraform init
    terraform plan -out=tfplan
    
    read -p "Apply changes? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        terraform apply tfplan
    fi
    
    rm -f tfplan
fi

echo "=== Done ==="
