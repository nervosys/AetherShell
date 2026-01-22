# Nervosys Multi-Program Package Registry Infrastructure

AWS infrastructure for hosting package registries for multiple Nervosys programs at `packages.nervosys.ai`.

## Supported Registries

| Registry      | URL Pattern                                   | Description               |
| ------------- | --------------------------------------------- | ------------------------- |
| `aethershell` | `packages.nervosys.ai/aethershell/api/v1/...` | AetherShell shell scripts |
| `autonomi`    | `packages.nervosys.ai/autonomi/api/v1/...`    | Autonomi packages         |
| `machina`     | `packages.nervosys.ai/machina/api/v1/...`     | Machina packages          |

Additional registries can be added via the `allowed_registries` Terraform variable.

## Architecture

```
                                    ┌─────────────────┐
                                    │   Route 53      │
                                    │ packages.       │
                                    │ nervosys.ai     │
                                    └────────┬────────┘
                                             │
                                    ┌────────▼────────┐
                                    │   CloudFront    │
                                    │   Distribution  │
                                    └────────┬────────┘
                                             │
                        ┌────────────────────┼────────────────────┐
                        │                    │                    │
               /api/* requests        /packages/* requests   Static assets
                        │                    │                    │
               ┌────────▼────────┐  ┌────────▼────────┐          │
               │   API Gateway   │  │       S3        │          │
               │    HTTP API     │  │  Package Files  │◄─────────┘
               └────────┬────────┘  └─────────────────┘
                        │
               ┌────────▼────────┐
               │     Lambda      │
               │   (Rust/ARM64)  │
               └────────┬────────┘
                        │
          ┌─────────────┼─────────────┐
          │             │             │
  ┌───────▼───────┐ ┌───▼────┐  ┌─────▼─────┐
  │   DynamoDB    │ │   S3   │  │ DynamoDB  │
  │   Packages    │ │ Upload │  │ Downloads │
  │   Metadata    │ │        │  │   Stats   │
  └───────────────┘ └────────┘  └───────────┘
```

## Components

| Component       | Purpose                                                 |
| --------------- | ------------------------------------------------------- |
| **CloudFront**  | CDN with TLS termination, caching for package downloads |
| **API Gateway** | HTTP API for package registry operations                |
| **Lambda**      | Rust function handling API requests                     |
| **S3**          | Package file storage                                    |
| **DynamoDB**    | Package metadata and download statistics                |
| **Route 53**    | DNS management                                          |
| **ACM**         | TLS certificate for `packages.nervosys.ai`              |

## Prerequisites

1. **AWS CLI** configured with appropriate credentials
2. **Terraform** >= 1.0
3. **cargo-lambda** for building the Lambda function:
   ```bash
   cargo install cargo-lambda
   ```
4. Route 53 hosted zone for `nervosys.ai`

## Setup

### 1. Create Terraform State Backend (one-time)

```bash
# Create S3 bucket for Terraform state
aws s3 mb s3://nervosys-terraform-state --region us-east-1
aws s3api put-bucket-versioning \
    --bucket nervosys-terraform-state \
    --versioning-configuration Status=Enabled

# Create DynamoDB table for state locking
aws dynamodb create-table \
    --table-name terraform-locks \
    --attribute-definitions AttributeName=LockID,AttributeType=S \
    --key-schema AttributeName=LockID,KeyType=HASH \
    --billing-mode PAY_PER_REQUEST \
    --region us-east-1
```

### 2. Configure Variables

```bash
cp terraform.tfvars.example terraform.tfvars
# Edit terraform.tfvars with your values
```

Required variables:
- `hosted_zone_id`: Your Route 53 hosted zone ID for `nervosys.ai`

### 3. Build and Deploy

**Linux/macOS:**
```bash
./build.sh deploy
```

**Windows (PowerShell):**
```powershell
.\build.ps1 -Deploy
```

**Manual deployment:**
```bash
# Build Lambda function
cd lambda
cargo lambda build --release --arm64 --output-format zip
cp target/lambda/aethershell-packages-api/bootstrap.zip ../lambda/api.zip
cd ..

# Deploy infrastructure
terraform init
terraform plan
terraform apply
```

## API Endpoints

All endpoints are prefixed with `/{registry}` where registry is one of: `aethershell`, `autonomi`, `machina`.

### Global Endpoints

| Method | Endpoint       | Description                  |
| ------ | -------------- | ---------------------------- |
| GET    | `/`            | Service health check         |
| GET    | `/health`      | Service health check         |
| GET    | `/registries`  | List available registries    |

### Registry-Specific Endpoints

| Method | Endpoint                                              | Description          |
| ------ | ----------------------------------------------------- | -------------------- |
| GET    | `/{registry}/`                                        | Registry info        |
| GET    | `/{registry}/api/v1/packages`                         | List all packages    |
| GET    | `/{registry}/api/v1/packages/{name}`                  | Get package details  |
| GET    | `/{registry}/api/v1/packages/{name}/{version}`        | Get specific version |
| GET    | `/{registry}/api/v1/packages/{name}/{version}/download` | Get download URL   |
| GET    | `/{registry}/api/v1/search?q={query}`                 | Search packages      |
| POST   | `/{registry}/api/v1/packages`                         | Publish new package  |
| DELETE | `/{registry}/api/v1/packages/{name}/{version}`        | Yank a version       |

### Example URLs

```bash
# List AetherShell packages
curl https://packages.nervosys.ai/aethershell/api/v1/packages

# Get Autonomi package details
curl https://packages.nervosys.ai/autonomi/api/v1/packages/my-package

# Search Machina packages
curl https://packages.nervosys.ai/machina/api/v1/search?q=utils
```

## Package Format

Packages are stored as `.tar.gz` archives with the following structure:
```
my-package-1.0.0/
├── aether.toml      # Package manifest
├── main.ae          # Entry point
└── lib/             # Additional modules
    ├── utils.ae
    └── helpers.ae
```

### Manifest (aether.toml)
```toml
[package]
name = "my-package"
version = "1.0.0"
description = "A useful package"
authors = ["Your Name <you@example.com>"]
license = "MIT"
repository = "https://github.com/you/my-package"

[dependencies]
other-package = "^1.0"
```

## Publishing Packages

```bash
# From AetherShell CLI (future)
ae pkg publish

# Or via API (replace {registry} with aethershell, autonomi, or machina)
curl -X POST https://packages.nervosys.ai/{registry}/api/v1/packages \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-package",
    "version": "1.0.0",
    "description": "A useful package",
    "authors": ["Your Name"],
    "license": "MIT",
    "data": "<base64-encoded-tarball>"
  }'
```

## Data Model

### DynamoDB Schema

**Packages Table:**
- `pk` (Hash Key): `{registry}#{package_name}` (e.g., `aethershell#http-utils`)
- `sk` (Range Key): `{version}` (e.g., `1.0.0`)
- `registry`: Registry name for GSI queries
- GSI `registry_index`: Queries by registry

**Downloads Table:**
- `package_key`: `{registry}#{name}@{version}`
- `date`: `YYYY-MM-DD`

### S3 Structure

```
nervosys-packages-{env}/
├── aethershell/
│   └── packages/
│       └── {name}/
│           └── {version}/
│               └── {name}.tar.gz
├── autonomi/
│   └── packages/
│       └── ...
└── machina/
    └── packages/
        └── ...
```

## Costs (Estimated)

| Service     | Monthly Cost                               |
| ----------- | ------------------------------------------ |
| Lambda      | ~$0 (free tier covers most usage)          |
| API Gateway | ~$1-5 for moderate traffic                 |
| CloudFront  | ~$0.50-5 depending on traffic              |
| S3          | ~$0.02 per GB stored                       |
| DynamoDB    | ~$0 (on-demand, low traffic)               |
| Route 53    | $0.50 per hosted zone                      |
| **Total**   | **~$2-15/month** for small-medium registry |

## Security

- All traffic is encrypted (TLS 1.2+)
- S3 bucket is private (CloudFront OAC)
- Package data integrity verified via SHA-256 checksums
- API authentication required for publishing/yanking
- DynamoDB encryption at rest enabled
- CloudWatch logging enabled

## Monitoring

- CloudWatch Logs for Lambda and API Gateway
- CloudWatch Metrics for all services
- Enable CloudWatch Alarms for:
  - Lambda errors
  - API Gateway 5xx errors
  - S3 bucket size

## Cleanup

```bash
terraform destroy
```

**Warning**: This will delete all packages and data!
