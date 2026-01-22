//! Nervosys Multi-Program Package Registry API
//!
//! Lambda function handling the package registry API endpoints for multiple programs.
//!
//! URL Pattern: packages.nervosys.ai/{registry}/api/v1/...
//! Examples:
//!   - packages.nervosys.ai/aethershell/api/v1/packages
//!   - packages.nervosys.ai/autonomi/api/v1/packages
//!   - packages.nervosys.ai/machina/api/v1/packages

use aws_lambda_events::apigw::{ApiGatewayV2httpRequest, ApiGatewayV2httpResponse};
use aws_lambda_events::encodings::Body;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client as DynamoClient;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client as S3Client;
use chrono::{DateTime, Utc};
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::time::Duration;
use tracing::{error, info};

// -----------------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageInfo {
    pub registry: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub checksum: String,
    pub size: u64,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageVersion {
    pub version: String,
    pub checksum: String,
    pub size: u64,
    pub created_at: DateTime<Utc>,
    pub download_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageListItem {
    pub name: String,
    pub description: String,
    pub latest_version: String,
    pub downloads: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageDetail {
    pub registry: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub versions: Vec<PackageVersion>,
    pub downloads: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishRequest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Base64-encoded package tarball
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub packages: Vec<PackageListItem>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistryInfo {
    pub name: String,
    pub package_count: u64,
    pub total_downloads: u64,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub code: String,
}

// -----------------------------------------------------------------------------
// Handler
// -----------------------------------------------------------------------------

struct AppState {
    s3: S3Client,
    dynamo: DynamoClient,
    bucket: String,
    packages_table: String,
    downloads_table: String,
    allowed_registries: HashSet<String>,
}

async fn function_handler(
    event: LambdaEvent<ApiGatewayV2httpRequest>,
) -> Result<ApiGatewayV2httpResponse, Error> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    
    let allowed_registries: HashSet<String> = env::var("ALLOWED_REGISTRIES")
        .unwrap_or_else(|_| "aethershell,autonomi,machina".to_string())
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect();
    
    let state = AppState {
        s3: S3Client::new(&config),
        dynamo: DynamoClient::new(&config),
        bucket: env::var("PACKAGES_BUCKET").unwrap_or_else(|_| "nervosys-packages".to_string()),
        packages_table: env::var("PACKAGES_TABLE")
            .unwrap_or_else(|_| "nervosys-packages".to_string()),
        downloads_table: env::var("DOWNLOADS_TABLE")
            .unwrap_or_else(|_| "nervosys-downloads".to_string()),
        allowed_registries,
    };

    let request = event.payload;
    let path = request.raw_path.as_deref().unwrap_or("/");
    let method = request
        .request_context
        .http
        .method
        .as_deref()
        .unwrap_or("GET");

    info!(path = %path, method = %method, "Handling request");

    let response = route_request(&state, method, path, &request).await;
    Ok(response)
}

async fn route_request(
    state: &AppState,
    method: &str,
    path: &str,
    request: &ApiGatewayV2httpRequest,
) -> ApiGatewayV2httpResponse {
    // Health check and root endpoints
    match (method, path) {
        ("GET", "/") | ("GET", "/health") => {
            return json_response(
                200,
                &serde_json::json!({
                    "status": "ok",
                    "service": "nervosys-packages",
                    "version": env!("CARGO_PKG_VERSION"),
                    "registries": state.allowed_registries.iter().collect::<Vec<_>>()
                }),
            );
        }
        ("GET", "/registries") => {
            return list_registries(state).await;
        }
        _ => {}
    }

    // Parse registry from path: /{registry}/api/v1/...
    let parts: Vec<&str> = path.trim_start_matches('/').splitn(2, '/').collect();
    if parts.is_empty() {
        return not_found();
    }

    let registry = parts[0].to_lowercase();
    let sub_path = if parts.len() > 1 {
        format!("/{}", parts[1])
    } else {
        "/".to_string()
    };

    // Validate registry
    if !state.allowed_registries.contains(&registry) {
        return error_response(
            404,
            "unknown_registry",
            &format!(
                "Unknown registry '{}'. Available: {:?}",
                registry,
                state.allowed_registries
            ),
        );
    }

    // Route to registry-specific handlers
    match (method, sub_path.as_str()) {
        // Registry info
        ("GET", "/") | ("GET", "/api/v1") => {
            registry_info(state, &registry).await
        }

        // List packages
        ("GET", "/api/v1/packages") => {
            list_packages(state, &registry, request).await
        }

        // Search packages
        ("GET", "/api/v1/search") => {
            search_packages(state, &registry, request).await
        }

        // Get package info
        ("GET", p) if p.starts_with("/api/v1/packages/") => {
            let pkg_parts: Vec<&str> = p
                .trim_start_matches("/api/v1/packages/")
                .split('/')
                .collect();
            match pkg_parts.as_slice() {
                [name] if !name.is_empty() => get_package(state, &registry, name).await,
                [name, version] => get_package_version(state, &registry, name, version).await,
                [name, version, "download"] => {
                    download_package(state, &registry, name, version).await
                }
                _ => not_found(),
            }
        }

        // Publish package
        ("POST", "/api/v1/packages") => {
            let body = request.body.as_deref().unwrap_or("");
            publish_package(state, &registry, body, request).await
        }

        // Yank version
        ("DELETE", p) if p.starts_with("/api/v1/packages/") => {
            let pkg_parts: Vec<&str> = p
                .trim_start_matches("/api/v1/packages/")
                .split('/')
                .collect();
            match pkg_parts.as_slice() {
                [name, version] => yank_package(state, &registry, name, version, request).await,
                _ => not_found(),
            }
        }

        _ => not_found(),
    }
}

// -----------------------------------------------------------------------------
// API Handlers
// -----------------------------------------------------------------------------

async fn list_registries(state: &AppState) -> ApiGatewayV2httpResponse {
    let registries: Vec<RegistryInfo> = state
        .allowed_registries
        .iter()
        .map(|name| RegistryInfo {
            name: name.clone(),
            package_count: 0, // Would query DynamoDB for actual count
            total_downloads: 0,
        })
        .collect();

    json_response(200, &registries)
}

async fn registry_info(state: &AppState, registry: &str) -> ApiGatewayV2httpResponse {
    // Count packages in this registry
    let result = state
        .dynamo
        .query()
        .table_name(&state.packages_table)
        .index_name("registry_index")
        .key_condition_expression("registry = :reg")
        .expression_attribute_values(":reg", AttributeValue::S(registry.to_string()))
        .select(aws_sdk_dynamodb::types::Select::Count)
        .send()
        .await;

    let count = result.map(|r| r.count()).unwrap_or(0);

    json_response(
        200,
        &serde_json::json!({
            "registry": registry,
            "package_count": count,
            "api_version": "v1",
            "endpoints": {
                "packages": format!("/{}/api/v1/packages", registry),
                "search": format!("/{}/api/v1/search", registry)
            }
        }),
    )
}

async fn list_packages(
    state: &AppState,
    registry: &str,
    request: &ApiGatewayV2httpRequest,
) -> ApiGatewayV2httpResponse {
    let limit = request
        .query_string_parameters
        .first("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    let result = state
        .dynamo
        .query()
        .table_name(&state.packages_table)
        .index_name("registry_index")
        .key_condition_expression("registry = :reg")
        .expression_attribute_values(":reg", AttributeValue::S(registry.to_string()))
        .limit(limit)
        .scan_index_forward(false) // Most recent first
        .send()
        .await;

    match result {
        Ok(output) => {
            let mut packages: HashMap<String, PackageListItem> = HashMap::new();

            for item in output.items.unwrap_or_default() {
                let name = attr_string(&item, "name");
                let version = attr_string(&item, "version");
                let description = attr_string(&item, "description");
                let downloads = attr_number(&item, "downloads");
                let created_at = attr_datetime(&item, "created_at");

                packages
                    .entry(name.clone())
                    .and_modify(|p| {
                        if semver::Version::parse(&version)
                            .ok()
                            .zip(semver::Version::parse(&p.latest_version).ok())
                            .map(|(v, l)| v > l)
                            .unwrap_or(false)
                        {
                            p.latest_version = version.clone();
                            p.updated_at = created_at;
                        }
                        p.downloads += downloads;
                    })
                    .or_insert(PackageListItem {
                        name,
                        description,
                        latest_version: version,
                        downloads,
                        updated_at: created_at,
                    });
            }

            let mut packages: Vec<_> = packages.into_values().collect();
            packages.sort_by(|a, b| b.downloads.cmp(&a.downloads));

            json_response(200, &packages)
        }
        Err(e) => {
            error!("Failed to list packages: {}", e);
            error_response(500, "internal_error", "Failed to list packages")
        }
    }
}

async fn search_packages(
    state: &AppState,
    registry: &str,
    request: &ApiGatewayV2httpRequest,
) -> ApiGatewayV2httpResponse {
    let query = request
        .query_string_parameters
        .first("q")
        .unwrap_or_default();
    let limit = request
        .query_string_parameters
        .first("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    // Query with filter (would use OpenSearch for production)
    let result = state
        .dynamo
        .query()
        .table_name(&state.packages_table)
        .index_name("registry_index")
        .key_condition_expression("registry = :reg")
        .filter_expression("contains(#name, :query) OR contains(description, :query)")
        .expression_attribute_names("#name", "name")
        .expression_attribute_values(":reg", AttributeValue::S(registry.to_string()))
        .expression_attribute_values(":query", AttributeValue::S(query.to_lowercase()))
        .limit(limit as i32)
        .send()
        .await;

    match result {
        Ok(output) => {
            let packages: Vec<PackageListItem> = output
                .items
                .unwrap_or_default()
                .into_iter()
                .map(|item| PackageListItem {
                    name: attr_string(&item, "name"),
                    description: attr_string(&item, "description"),
                    latest_version: attr_string(&item, "version"),
                    downloads: attr_number(&item, "downloads"),
                    updated_at: attr_datetime(&item, "created_at"),
                })
                .collect();

            json_response(
                200,
                &SearchResult {
                    total: packages.len(),
                    packages,
                },
            )
        }
        Err(e) => {
            error!("Search failed: {}", e);
            error_response(500, "internal_error", "Search failed")
        }
    }
}

async fn get_package(state: &AppState, registry: &str, name: &str) -> ApiGatewayV2httpResponse {
    let pk = format!("{}#{}", registry, name);

    let result = state
        .dynamo
        .query()
        .table_name(&state.packages_table)
        .key_condition_expression("pk = :pk")
        .expression_attribute_values(":pk", AttributeValue::S(pk))
        .scan_index_forward(false)
        .send()
        .await;

    match result {
        Ok(output) => {
            let items = output.items.unwrap_or_default();
            if items.is_empty() {
                return not_found();
            }

            let first = &items[0];
            let versions: Vec<PackageVersion> = items
                .iter()
                .map(|item| PackageVersion {
                    version: attr_string(item, "version"),
                    checksum: attr_string(item, "checksum"),
                    size: attr_number(item, "size"),
                    created_at: attr_datetime(item, "created_at"),
                    download_url: None,
                })
                .collect();

            let detail = PackageDetail {
                registry: registry.to_string(),
                name: attr_string(first, "name"),
                description: attr_string(first, "description"),
                authors: attr_string_list(first, "authors"),
                license: attr_option_string(first, "license"),
                repository: attr_option_string(first, "repository"),
                keywords: attr_string_list(first, "keywords"),
                downloads: items.iter().map(|i| attr_number(i, "downloads")).sum(),
                created_at: items
                    .iter()
                    .map(|i| attr_datetime(i, "created_at"))
                    .min()
                    .unwrap_or_else(Utc::now),
                updated_at: items
                    .iter()
                    .map(|i| attr_datetime(i, "created_at"))
                    .max()
                    .unwrap_or_else(Utc::now),
                versions,
            };

            json_response(200, &detail)
        }
        Err(e) => {
            error!("Failed to get package {}/{}: {}", registry, name, e);
            error_response(500, "internal_error", "Failed to get package")
        }
    }
}

async fn get_package_version(
    state: &AppState,
    registry: &str,
    name: &str,
    version: &str,
) -> ApiGatewayV2httpResponse {
    let pk = format!("{}#{}", registry, name);

    let result = state
        .dynamo
        .get_item()
        .table_name(&state.packages_table)
        .key("pk", AttributeValue::S(pk))
        .key("sk", AttributeValue::S(version.to_string()))
        .send()
        .await;

    match result {
        Ok(output) => match output.item {
            Some(item) => {
                let info = PackageInfo {
                    registry: registry.to_string(),
                    name: attr_string(&item, "name"),
                    version: attr_string(&item, "version"),
                    description: attr_string(&item, "description"),
                    authors: attr_string_list(&item, "authors"),
                    license: attr_option_string(&item, "license"),
                    repository: attr_option_string(&item, "repository"),
                    keywords: attr_string_list(&item, "keywords"),
                    checksum: attr_string(&item, "checksum"),
                    size: attr_number(&item, "size"),
                    created_at: attr_datetime(&item, "created_at"),
                    downloads: attr_number(&item, "downloads"),
                };
                json_response(200, &info)
            }
            None => not_found(),
        },
        Err(e) => {
            error!(
                "Failed to get package {}/{}@{}: {}",
                registry, name, version, e
            );
            error_response(500, "internal_error", "Failed to get package version")
        }
    }
}

async fn download_package(
    state: &AppState,
    registry: &str,
    name: &str,
    version: &str,
) -> ApiGatewayV2httpResponse {
    // S3 key: {registry}/packages/{name}/{version}/{name}.tar.gz
    let key = format!(
        "{}/packages/{}/{}/{}.tar.gz",
        registry, name, version, name
    );

    // Generate presigned URL
    let presign_config = PresigningConfig::builder()
        .expires_in(Duration::from_secs(300))
        .build()
        .unwrap();

    let presigned = state
        .s3
        .get_object()
        .bucket(&state.bucket)
        .key(&key)
        .presigned(presign_config)
        .await;

    match presigned {
        Ok(presigned_request) => {
            let pk = format!("{}#{}", registry, name);

            // Increment download count
            let _ = state
                .dynamo
                .update_item()
                .table_name(&state.packages_table)
                .key("pk", AttributeValue::S(pk))
                .key("sk", AttributeValue::S(version.to_string()))
                .update_expression("SET downloads = downloads + :inc")
                .expression_attribute_values(":inc", AttributeValue::N("1".to_string()))
                .send()
                .await;

            // Record download stat
            let today = Utc::now().format("%Y-%m-%d").to_string();
            let package_key = format!("{}#{}@{}", registry, name, version);

            let _ = state
                .dynamo
                .update_item()
                .table_name(&state.downloads_table)
                .key("package_key", AttributeValue::S(package_key))
                .key("date", AttributeValue::S(today))
                .update_expression(
                    "SET download_count = if_not_exists(download_count, :zero) + :inc, #ttl = :ttl",
                )
                .expression_attribute_names("#ttl", "ttl")
                .expression_attribute_values(":inc", AttributeValue::N("1".to_string()))
                .expression_attribute_values(":zero", AttributeValue::N("0".to_string()))
                .expression_attribute_values(
                    ":ttl",
                    AttributeValue::N((Utc::now().timestamp() + 90 * 24 * 60 * 60).to_string()),
                )
                .send()
                .await;

            json_response(
                200,
                &serde_json::json!({
                    "download_url": presigned_request.uri().to_string()
                }),
            )
        }
        Err(e) => {
            error!("Failed to generate download URL: {}", e);
            error_response(500, "internal_error", "Failed to generate download URL")
        }
    }
}

async fn publish_package(
    state: &AppState,
    registry: &str,
    body: &str,
    request: &ApiGatewayV2httpRequest,
) -> ApiGatewayV2httpResponse {
    // Check authorization
    let auth_header = request
        .headers
        .get("authorization")
        .map(|v| v.to_str().unwrap_or(""));

    if auth_header.is_none() {
        return error_response(401, "unauthorized", "Authorization required");
    }

    // Parse request
    let publish_req: PublishRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            return error_response(400, "invalid_request", &format!("Invalid JSON: {}", e))
        }
    };

    // Validate package name
    if !is_valid_package_name(&publish_req.name) {
        return error_response(400, "invalid_name", "Invalid package name");
    }

    // Validate version
    if semver::Version::parse(&publish_req.version).is_err() {
        return error_response(400, "invalid_version", "Invalid semver version");
    }

    let pk = format!("{}#{}", registry, publish_req.name);

    // Check if version already exists
    let existing = state
        .dynamo
        .get_item()
        .table_name(&state.packages_table)
        .key("pk", AttributeValue::S(pk.clone()))
        .key("sk", AttributeValue::S(publish_req.version.clone()))
        .send()
        .await;

    if matches!(existing, Ok(ref r) if r.item.is_some()) {
        return error_response(
            409,
            "version_exists",
            "This version has already been published",
        );
    }

    // Decode and validate package data
    let data = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &publish_req.data,
    ) {
        Ok(d) => d,
        Err(_) => return error_response(400, "invalid_data", "Invalid base64 data"),
    };

    // Calculate checksum
    let checksum = format!("{:x}", Sha256::digest(&data));

    // Upload to S3: {registry}/packages/{name}/{version}/{name}.tar.gz
    let key = format!(
        "{}/packages/{}/{}/{}.tar.gz",
        registry, publish_req.name, publish_req.version, publish_req.name
    );

    let upload_result = state
        .s3
        .put_object()
        .bucket(&state.bucket)
        .key(&key)
        .body(data.clone().into())
        .content_type("application/gzip")
        .send()
        .await;

    if let Err(e) = upload_result {
        error!("Failed to upload package: {}", e);
        return error_response(500, "upload_failed", "Failed to upload package");
    }

    // Store metadata in DynamoDB
    let now = Utc::now();
    let put_result = state
        .dynamo
        .put_item()
        .table_name(&state.packages_table)
        .item("pk", AttributeValue::S(pk))
        .item("sk", AttributeValue::S(publish_req.version.clone()))
        .item("registry", AttributeValue::S(registry.to_string()))
        .item("name", AttributeValue::S(publish_req.name.clone()))
        .item("version", AttributeValue::S(publish_req.version.clone()))
        .item("description", AttributeValue::S(publish_req.description))
        .item(
            "authors",
            AttributeValue::L(
                publish_req
                    .authors
                    .into_iter()
                    .map(AttributeValue::S)
                    .collect(),
            ),
        )
        .item(
            "license",
            publish_req
                .license
                .map(AttributeValue::S)
                .unwrap_or(AttributeValue::Null(true)),
        )
        .item(
            "repository",
            publish_req
                .repository
                .map(AttributeValue::S)
                .unwrap_or(AttributeValue::Null(true)),
        )
        .item(
            "keywords",
            AttributeValue::L(
                publish_req
                    .keywords
                    .into_iter()
                    .map(AttributeValue::S)
                    .collect(),
            ),
        )
        .item("checksum", AttributeValue::S(checksum.clone()))
        .item("size", AttributeValue::N(data.len().to_string()))
        .item("downloads", AttributeValue::N("0".to_string()))
        .item("created_at", AttributeValue::S(now.to_rfc3339()))
        .item("yanked", AttributeValue::Bool(false))
        .send()
        .await;

    match put_result {
        Ok(_) => {
            info!(
                "Published package {}/{}@{}",
                registry, publish_req.name, publish_req.version
            );
            json_response(
                201,
                &serde_json::json!({
                    "registry": registry,
                    "name": publish_req.name,
                    "version": publish_req.version,
                    "checksum": checksum,
                    "size": data.len()
                }),
            )
        }
        Err(e) => {
            error!("Failed to store package metadata: {}", e);
            error_response(500, "internal_error", "Failed to publish package")
        }
    }
}

async fn yank_package(
    state: &AppState,
    registry: &str,
    name: &str,
    version: &str,
    request: &ApiGatewayV2httpRequest,
) -> ApiGatewayV2httpResponse {
    // Check authorization
    let auth_header = request
        .headers
        .get("authorization")
        .map(|v| v.to_str().unwrap_or(""));

    if auth_header.is_none() {
        return error_response(401, "unauthorized", "Authorization required");
    }

    let pk = format!("{}#{}", registry, name);

    let result = state
        .dynamo
        .update_item()
        .table_name(&state.packages_table)
        .key("pk", AttributeValue::S(pk.clone()))
        .key("sk", AttributeValue::S(version.to_string()))
        .update_expression("SET yanked = :yanked")
        .expression_attribute_values(":yanked", AttributeValue::Bool(true))
        .condition_expression("attribute_exists(pk)")
        .send()
        .await;

    match result {
        Ok(_) => {
            info!("Yanked package {}/{}@{}", registry, name, version);
            json_response(200, &serde_json::json!({"status": "yanked"}))
        }
        Err(e) => {
            error!("Failed to yank {}/{}@{}: {}", registry, name, version, e);
            error_response(500, "internal_error", "Failed to yank package")
        }
    }
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn json_response<T: Serialize>(status: i64, body: &T) -> ApiGatewayV2httpResponse {
    ApiGatewayV2httpResponse {
        status_code: status,
        headers: HashMap::from([
            ("content-type".to_string(), "application/json".to_string()),
            (
                "cache-control".to_string(),
                "no-cache, no-store".to_string(),
            ),
        ])
        .into(),
        body: Some(Body::Text(serde_json::to_string(body).unwrap())),
        ..Default::default()
    }
}

fn error_response(status: i64, code: &str, message: &str) -> ApiGatewayV2httpResponse {
    json_response(
        status,
        &ApiError {
            error: message.to_string(),
            code: code.to_string(),
        },
    )
}

fn not_found() -> ApiGatewayV2httpResponse {
    error_response(404, "not_found", "Resource not found")
}

fn is_valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        && name
            .chars()
            .next()
            .map(|c| c.is_alphabetic())
            .unwrap_or(false)
}

fn attr_string(item: &HashMap<String, AttributeValue>, key: &str) -> String {
    item.get(key)
        .and_then(|v| v.as_s().ok())
        .cloned()
        .unwrap_or_default()
}

fn attr_option_string(item: &HashMap<String, AttributeValue>, key: &str) -> Option<String> {
    item.get(key).and_then(|v| v.as_s().ok()).cloned()
}

fn attr_string_list(item: &HashMap<String, AttributeValue>, key: &str) -> Vec<String> {
    item.get(key)
        .and_then(|v| v.as_l().ok())
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_s().ok().cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn attr_number(item: &HashMap<String, AttributeValue>, key: &str) -> u64 {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn attr_datetime(item: &HashMap<String, AttributeValue>, key: &str) -> DateTime<Utc> {
    item.get(key)
        .and_then(|v| v.as_s().ok())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

// -----------------------------------------------------------------------------
// Main
// -----------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    lambda_runtime::run(service_fn(function_handler)).await
}
