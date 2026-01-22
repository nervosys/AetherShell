# AetherShell Package Registry Infrastructure
# packages.nervosys.ai

terraform {
  required_version = ">= 1.0"
  
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }

  backend "s3" {
    bucket         = "nervosys-terraform-state"
    key            = "aethershell/packages/terraform.tfstate"
    region         = "us-east-1"
    encrypt        = true
    dynamodb_table = "terraform-locks"
  }
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = "AetherShell"
      Environment = var.environment
      ManagedBy   = "Terraform"
    }
  }
}

# For CloudFront certificate (must be us-east-1)
provider "aws" {
  alias  = "us_east_1"
  region = "us-east-1"
}

# -----------------------------------------------------------------------------
# Variables
# -----------------------------------------------------------------------------

variable "aws_region" {
  description = "AWS region for resources"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment name"
  type        = string
  default     = "production"
}

variable "domain_name" {
  description = "Domain name for the package registry"
  type        = string
  default     = "packages.nervosys.ai"
}

variable "hosted_zone_id" {
  description = "Route 53 hosted zone ID for nervosys.ai"
  type        = string
}

# -----------------------------------------------------------------------------
# S3 Bucket for Package Storage
# -----------------------------------------------------------------------------

resource "aws_s3_bucket" "packages" {
  bucket = "aethershell-packages-${var.environment}"
}

resource "aws_s3_bucket_versioning" "packages" {
  bucket = aws_s3_bucket.packages.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "packages" {
  bucket = aws_s3_bucket.packages.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_public_access_block" "packages" {
  bucket = aws_s3_bucket.packages.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_cors_configuration" "packages" {
  bucket = aws_s3_bucket.packages.id

  cors_rule {
    allowed_headers = ["*"]
    allowed_methods = ["GET", "HEAD"]
    allowed_origins = ["*"]
    expose_headers  = ["ETag", "Content-Length", "Content-Type"]
    max_age_seconds = 3600
  }
}

# -----------------------------------------------------------------------------
# DynamoDB for Package Metadata
# -----------------------------------------------------------------------------

resource "aws_dynamodb_table" "packages" {
  name         = "aethershell-packages-${var.environment}"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "name"
  range_key    = "version"

  attribute {
    name = "name"
    type = "S"
  }

  attribute {
    name = "version"
    type = "S"
  }

  attribute {
    name = "created_at"
    type = "S"
  }

  global_secondary_index {
    name            = "created_at_index"
    hash_key        = "name"
    range_key       = "created_at"
    projection_type = "ALL"
  }

  point_in_time_recovery {
    enabled = true
  }

  tags = {
    Name = "AetherShell Package Registry"
  }
}

# Package download counts
resource "aws_dynamodb_table" "download_stats" {
  name         = "aethershell-downloads-${var.environment}"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "package_version"
  range_key    = "date"

  attribute {
    name = "package_version"
    type = "S"
  }

  attribute {
    name = "date"
    type = "S"
  }

  ttl {
    attribute_name = "ttl"
    enabled        = true
  }
}

# -----------------------------------------------------------------------------
# Lambda Functions for API
# -----------------------------------------------------------------------------

# IAM Role for Lambda
resource "aws_iam_role" "lambda" {
  name = "aethershell-packages-lambda-${var.environment}"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "lambda.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy" "lambda" {
  name = "aethershell-packages-lambda-policy"
  role = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "logs:CreateLogGroup",
          "logs:CreateLogStream",
          "logs:PutLogEvents"
        ]
        Resource = "arn:aws:logs:*:*:*"
      },
      {
        Effect = "Allow"
        Action = [
          "s3:GetObject",
          "s3:PutObject",
          "s3:ListBucket",
          "s3:DeleteObject"
        ]
        Resource = [
          aws_s3_bucket.packages.arn,
          "${aws_s3_bucket.packages.arn}/*"
        ]
      },
      {
        Effect = "Allow"
        Action = [
          "dynamodb:GetItem",
          "dynamodb:PutItem",
          "dynamodb:UpdateItem",
          "dynamodb:DeleteItem",
          "dynamodb:Query",
          "dynamodb:Scan"
        ]
        Resource = [
          aws_dynamodb_table.packages.arn,
          "${aws_dynamodb_table.packages.arn}/index/*",
          aws_dynamodb_table.download_stats.arn
        ]
      }
    ]
  })
}

# Lambda function for API
resource "aws_lambda_function" "api" {
  filename         = "${path.module}/lambda/api.zip"
  function_name    = "aethershell-packages-api-${var.environment}"
  role             = aws_iam_role.lambda.arn
  handler          = "bootstrap"
  runtime          = "provided.al2023"
  architectures    = ["arm64"]
  memory_size      = 256
  timeout          = 30

  environment {
    variables = {
      RUST_LOG         = "info"
      PACKAGES_BUCKET  = aws_s3_bucket.packages.bucket
      PACKAGES_TABLE   = aws_dynamodb_table.packages.name
      DOWNLOADS_TABLE  = aws_dynamodb_table.download_stats.name
      ENVIRONMENT      = var.environment
    }
  }

  depends_on = [aws_iam_role_policy.lambda]
}

resource "aws_lambda_function_url" "api" {
  function_name      = aws_lambda_function.api.function_name
  authorization_type = "NONE"

  cors {
    allow_origins     = ["*"]
    allow_methods     = ["GET", "POST", "PUT", "DELETE"]
    allow_headers     = ["*"]
    expose_headers    = ["*"]
    max_age           = 3600
  }
}

# -----------------------------------------------------------------------------
# API Gateway
# -----------------------------------------------------------------------------

resource "aws_apigatewayv2_api" "packages" {
  name          = "aethershell-packages-${var.environment}"
  protocol_type = "HTTP"
  description   = "AetherShell Package Registry API"

  cors_configuration {
    allow_origins = ["*"]
    allow_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
    allow_headers = ["*"]
    max_age       = 3600
  }
}

resource "aws_apigatewayv2_stage" "packages" {
  api_id      = aws_apigatewayv2_api.packages.id
  name        = "$default"
  auto_deploy = true

  access_log_settings {
    destination_arn = aws_cloudwatch_log_group.api_gateway.arn
    format = jsonencode({
      requestId      = "$context.requestId"
      ip             = "$context.identity.sourceIp"
      requestTime    = "$context.requestTime"
      httpMethod     = "$context.httpMethod"
      routeKey       = "$context.routeKey"
      status         = "$context.status"
      responseLength = "$context.responseLength"
      latency        = "$context.integrationLatency"
    })
  }
}

resource "aws_cloudwatch_log_group" "api_gateway" {
  name              = "/aws/apigateway/aethershell-packages-${var.environment}"
  retention_in_days = 30
}

resource "aws_apigatewayv2_integration" "lambda" {
  api_id                 = aws_apigatewayv2_api.packages.id
  integration_type       = "AWS_PROXY"
  integration_uri        = aws_lambda_function.api.invoke_arn
  payload_format_version = "2.0"
}

resource "aws_apigatewayv2_route" "default" {
  api_id    = aws_apigatewayv2_api.packages.id
  route_key = "$default"
  target    = "integrations/${aws_apigatewayv2_integration.lambda.id}"
}

resource "aws_lambda_permission" "api_gateway" {
  statement_id  = "AllowAPIGateway"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.api.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.packages.execution_arn}/*/*"
}

# -----------------------------------------------------------------------------
# CloudFront Distribution
# -----------------------------------------------------------------------------

resource "aws_acm_certificate" "packages" {
  provider          = aws.us_east_1
  domain_name       = var.domain_name
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_route53_record" "cert_validation" {
  for_each = {
    for dvo in aws_acm_certificate.packages.domain_validation_options : dvo.domain_name => {
      name   = dvo.resource_record_name
      record = dvo.resource_record_value
      type   = dvo.resource_record_type
    }
  }

  allow_overwrite = true
  name            = each.value.name
  records         = [each.value.record]
  ttl             = 60
  type            = each.value.type
  zone_id         = var.hosted_zone_id
}

resource "aws_acm_certificate_validation" "packages" {
  provider                = aws.us_east_1
  certificate_arn         = aws_acm_certificate.packages.arn
  validation_record_fqdns = [for record in aws_route53_record.cert_validation : record.fqdn]
}

# CloudFront Origin Access Control for S3
resource "aws_cloudfront_origin_access_control" "packages" {
  name                              = "aethershell-packages-oac"
  description                       = "OAC for AetherShell packages S3 bucket"
  origin_access_control_origin_type = "s3"
  signing_behavior                  = "always"
  signing_protocol                  = "sigv4"
}

resource "aws_cloudfront_distribution" "packages" {
  enabled             = true
  is_ipv6_enabled     = true
  comment             = "AetherShell Package Registry"
  default_root_object = ""
  aliases             = [var.domain_name]
  price_class         = "PriceClass_100"

  # API Gateway origin (for /api/*)
  origin {
    domain_name = replace(aws_apigatewayv2_api.packages.api_endpoint, "https://", "")
    origin_id   = "api"

    custom_origin_config {
      http_port              = 80
      https_port             = 443
      origin_protocol_policy = "https-only"
      origin_ssl_protocols   = ["TLSv1.2"]
    }
  }

  # S3 origin (for package downloads)
  origin {
    domain_name              = aws_s3_bucket.packages.bucket_regional_domain_name
    origin_id                = "s3"
    origin_access_control_id = aws_cloudfront_origin_access_control.packages.id
  }

  # Default behavior - API
  default_cache_behavior {
    allowed_methods        = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    cached_methods         = ["GET", "HEAD"]
    target_origin_id       = "api"
    viewer_protocol_policy = "redirect-to-https"
    compress               = true

    forwarded_values {
      query_string = true
      headers      = ["Authorization", "Accept", "Content-Type"]

      cookies {
        forward = "none"
      }
    }

    min_ttl     = 0
    default_ttl = 0
    max_ttl     = 0
  }

  # Package downloads - cached
  ordered_cache_behavior {
    path_pattern           = "/packages/*"
    allowed_methods        = ["GET", "HEAD", "OPTIONS"]
    cached_methods         = ["GET", "HEAD"]
    target_origin_id       = "s3"
    viewer_protocol_policy = "redirect-to-https"
    compress               = true

    forwarded_values {
      query_string = false

      cookies {
        forward = "none"
      }
    }

    min_ttl     = 0
    default_ttl = 86400
    max_ttl     = 31536000
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  viewer_certificate {
    acm_certificate_arn      = aws_acm_certificate.packages.arn
    ssl_support_method       = "sni-only"
    minimum_protocol_version = "TLSv1.2_2021"
  }

  depends_on = [aws_acm_certificate_validation.packages]
}

# S3 bucket policy for CloudFront
resource "aws_s3_bucket_policy" "packages" {
  bucket = aws_s3_bucket.packages.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AllowCloudFrontServicePrincipal"
        Effect = "Allow"
        Principal = {
          Service = "cloudfront.amazonaws.com"
        }
        Action   = "s3:GetObject"
        Resource = "${aws_s3_bucket.packages.arn}/*"
        Condition = {
          StringEquals = {
            "AWS:SourceArn" = aws_cloudfront_distribution.packages.arn
          }
        }
      }
    ]
  })
}

# -----------------------------------------------------------------------------
# Route 53 DNS
# -----------------------------------------------------------------------------

resource "aws_route53_record" "packages" {
  zone_id = var.hosted_zone_id
  name    = var.domain_name
  type    = "A"

  alias {
    name                   = aws_cloudfront_distribution.packages.domain_name
    zone_id                = aws_cloudfront_distribution.packages.hosted_zone_id
    evaluate_target_health = false
  }
}

resource "aws_route53_record" "packages_aaaa" {
  zone_id = var.hosted_zone_id
  name    = var.domain_name
  type    = "AAAA"

  alias {
    name                   = aws_cloudfront_distribution.packages.domain_name
    zone_id                = aws_cloudfront_distribution.packages.hosted_zone_id
    evaluate_target_health = false
  }
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------

output "api_endpoint" {
  description = "API Gateway endpoint URL"
  value       = aws_apigatewayv2_api.packages.api_endpoint
}

output "cloudfront_domain" {
  description = "CloudFront distribution domain"
  value       = aws_cloudfront_distribution.packages.domain_name
}

output "packages_bucket" {
  description = "S3 bucket for packages"
  value       = aws_s3_bucket.packages.bucket
}

output "packages_table" {
  description = "DynamoDB table for package metadata"
  value       = aws_dynamodb_table.packages.name
}

output "registry_url" {
  description = "Package registry URL"
  value       = "https://${var.domain_name}"
}
