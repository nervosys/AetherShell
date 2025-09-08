use crate::ai_api::{models::*, config::APIConfig, AIModelAPI};
use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
    timeout::TimeoutLayer,
};
use utoipa::ToSchema;
use utoipa_swagger_ui::SwaggerUi;

/// HTTP Server for the AI Model API
pub struct APIServer {
    config: APIConfig,
    api: Arc<AIModelAPI>,
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub api: Arc<AIModelAPI>,
    pub config: APIConfig,
}

/// Query parameters for model listing
#[derive(Debug, Deserialize, ToSchema)]
pub struct ModelsQuery {
    /// Filter by provider
    pub provider: Option<String>,
    /// Filter by capability
    pub capability: Option<String>,
    /// Include local models only
    pub local_only: Option<bool>,
}

/// Query parameters for pagination
#[derive(Debug, Deserialize, ToSchema)]
pub struct PaginationQuery {
    /// Page number (0-based)
    pub page: Option<usize>,
    /// Items per page
    pub limit: Option<usize>,
}

impl APIServer {
    pub fn new(config: APIConfig) -> Result<Self> {
        let api = Arc::new(AIModelAPI::new(config.clone())?);
        Ok(Self { config, api })
    }

    /// Start the HTTP server
    pub async fn start(&self) -> Result<()> {
        let app = self.create_router().await?;
        
        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        let listener = TcpListener::bind(&addr).await?;
        
        tracing::info!("AI Model API server starting on {}", addr);
        
        axum::serve(listener, app).await?;
        
        Ok(())
    }

    /// Create the main router with all endpoints
    async fn create_router(&self) -> Result<Router> {
        let state = AppState {
            api: self.api.clone(),
            config: self.config.clone(),
        };

        let api_routes = Router::new()
            // OpenAI-compatible endpoints
            .route("/models", get(list_models))
            .route("/chat/completions", post(chat_completions))
            .route("/embeddings", post(embeddings))
            
            // Extended endpoints for model management
            .route("/models/:model_id", get(get_model))
            .route("/models/:model_id/download", post(download_model))
            .route("/models/:model_id/convert", post(convert_model))
            .route("/models/:model_id", axum::routing::delete(delete_model))
            
            // Provider management
            .route("/providers", get(list_providers))
            .route("/providers/:provider_id/validate", post(validate_provider))
            
            // Storage and caching
            .route("/storage/stats", get(storage_stats))
            .route("/storage/cleanup", post(cleanup_storage))
            
            // Health and status
            .route("/health", get(health_check))
            .route("/status", get(server_status))
            
            .with_state(state);

        let mut app = Router::new()
            .nest("/v1", api_routes)
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(TimeoutLayer::new(std::time::Duration::from_secs(
                        self.config.server.request_timeout_seconds,
                    )))
                    .layer(if self.config.server.enable_cors {
                        CorsLayer::new()
                            .allow_origin(Any)
                            .allow_methods(Any)
                            .allow_headers(Any)
                    } else {
                        CorsLayer::new()
                    }),
            );

        // Add OpenAPI documentation if enabled
        if self.config.server.enable_openapi {
            app = app.merge(
                SwaggerUi::new("/swagger-ui")
                    .url("/api-docs/openapi.json", utoipa::openapi::OpenApiBuilder::new()
                        .info(utoipa::openapi::InfoBuilder::new()
                            .title("AI Model API")
                            .version("1.0.0")
                            .build()
                        )
                        .build()),
            );
        }

        Ok(app)
    }
}

// API Handlers

/// List available models
async fn list_models(
    State(state): State<AppState>,
    Query(query): Query<ModelsQuery>,
) -> Result<Json<Vec<ModelInfo>>, (StatusCode, Json<APIError>)> {
    match state.api.list_models().await {
        Ok(mut models) => {
            // Apply filters
            if let Some(provider) = &query.provider {
                models.retain(|m| &m.provider == provider);
            }
            
            if let Some(capability) = &query.capability {
                models.retain(|m| match capability.as_str() {
                    "chat" => m.capabilities.chat,
                    "embeddings" => m.capabilities.embeddings,
                    "image_generation" => m.capabilities.image_generation,
                    "image_understanding" => m.capabilities.image_understanding,
                    "function_calling" => m.capabilities.function_calling,
                    _ => true,
                });
            }
            
            if query.local_only.unwrap_or(false) {
                models.retain(|m| m.local_path.is_some());
            }
            
            Ok(Json(models))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(APIError {
                error: ErrorDetail {
                    message: e.to_string(),
                    error_type: "internal_error".to_string(),
                    param: None,
                    code: None,
                },
            }),
        )),
    }
}

/// Get specific model information
async fn get_model(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelInfo>, (StatusCode, Json<APIError>)> {
    match state.api.list_models().await {
        Ok(models) => {
            if let Some(model) = models.iter().find(|m| m.id == model_id) {
                Ok(Json(model.clone()))
            } else {
                Err((
                    StatusCode::NOT_FOUND,
                    Json(APIError {
                        error: ErrorDetail {
                            message: format!("Model {} not found", model_id),
                            error_type: "not_found".to_string(),
                            param: Some("model_id".to_string()),
                            code: None,
                        },
                    }),
                ))
            }
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(APIError {
                error: ErrorDetail {
                    message: e.to_string(),
                    error_type: "internal_error".to_string(),
                    param: None,
                    code: None,
                },
            }),
        )),
    }
}

/// Chat completions endpoint
async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, (StatusCode, Json<APIError>)> {
    // Validate API key if required
    if state.config.security.require_api_key {
        if let Err(e) = validate_api_key(&headers, &state.config) {
            return Err(e);
        }
    }

    match state.api.chat_completion(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(APIError {
                error: ErrorDetail {
                    message: e.to_string(),
                    error_type: "invalid_request_error".to_string(),
                    param: None,
                    code: None,
                },
            }),
        )),
    }
}

/// Embeddings endpoint
async fn embeddings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Json<EmbeddingResponse>, (StatusCode, Json<APIError>)> {
    // Validate API key if required
    if state.config.security.require_api_key {
        if let Err(e) = validate_api_key(&headers, &state.config) {
            return Err(e);
        }
    }

    match state.api.embeddings(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(APIError {
                error: ErrorDetail {
                    message: e.to_string(),
                    error_type: "invalid_request_error".to_string(),
                    param: None,
                    code: None,
                },
            }),
        )),
    }
}

/// Download model endpoint
async fn download_model(
    State(_state): State<AppState>,
    Path(_model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<APIError>)> {
    // TODO: Implement model downloading
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(APIError {
            error: ErrorDetail {
                message: "Model downloading not yet implemented".to_string(),
                error_type: "not_implemented".to_string(),
                param: None,
                code: None,
            },
        }),
    ))
}

/// Convert model format endpoint
async fn convert_model(
    State(_state): State<AppState>,
    Path(_model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<APIError>)> {
    // TODO: Implement model conversion endpoint
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(APIError {
            error: ErrorDetail {
                message: "Model conversion not yet implemented".to_string(),
                error_type: "not_implemented".to_string(),
                param: None,
                code: None,
            },
        }),
    ))
}

/// Delete model endpoint
async fn delete_model(
    State(_state): State<AppState>,
    Path(_model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<APIError>)> {
    // TODO: Implement model deletion
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(APIError {
            error: ErrorDetail {
                message: "Model deletion not yet implemented".to_string(),
                error_type: "not_implemented".to_string(),
                param: None,
                code: None,
            },
        }),
    ))
}

/// List providers endpoint
async fn list_providers(
    State(_state): State<AppState>,
) -> Result<Json<Vec<ProviderInfo>>, (StatusCode, Json<APIError>)> {
    let providers = vec![
        ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            description: "OpenAI GPT models".to_string(),
            enabled: true,
            status: "active".to_string(),
        },
        ProviderInfo {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            description: "Anthropic Claude models".to_string(),
            enabled: true,
            status: "active".to_string(),
        },
        ProviderInfo {
            id: "local".to_string(),
            name: "Local Models".to_string(),
            description: "Locally hosted models".to_string(),
            enabled: true,
            status: "active".to_string(),
        },
    ];
    
    Ok(Json(providers))
}

/// Validate provider endpoint
async fn validate_provider(
    State(_state): State<AppState>,
    Path(_provider_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<APIError>)> {
    // TODO: Implement provider validation
    Ok(Json(serde_json::json!({"valid": true})))
}

/// Storage statistics endpoint
async fn storage_stats(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<APIError>)> {
    // TODO: Implement storage stats
    Ok(Json(serde_json::json!({
        "total_models": 0,
        "total_size": "0 B",
        "cache_size": "0 B"
    })))
}

/// Cleanup storage endpoint
async fn cleanup_storage(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<APIError>)> {
    // TODO: Implement storage cleanup
    Ok(Json(serde_json::json!({
        "cleaned": true,
        "freed_bytes": 0
    })))
}

/// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// Server status endpoint
async fn server_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "running",
        "version": env!("CARGO_PKG_VERSION"),
        "config": {
            "cors_enabled": state.config.server.enable_cors,
            "openapi_enabled": state.config.server.enable_openapi,
        },
        "uptime": "unknown", // TODO: Track uptime
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

// Helper functions

/// Validate API key from request headers
fn validate_api_key(
    headers: &HeaderMap,
    config: &APIConfig,
) -> Result<(), (StatusCode, Json<APIError>)> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    if let Some(provided_key) = auth_header {
        if config.security.api_keys.contains(&provided_key.to_string()) {
            Ok(())
        } else {
            Err((
                StatusCode::UNAUTHORIZED,
                Json(APIError {
                    error: ErrorDetail {
                        message: "Invalid API key".to_string(),
                        error_type: "authentication_error".to_string(),
                        param: None,
                        code: None,
                    },
                }),
            ))
        }
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(APIError {
                error: ErrorDetail {
                    message: "Missing API key".to_string(),
                    error_type: "authentication_error".to_string(),
                    param: None,
                    code: None,
                },
            }),
        ))
    }
}

// Additional types for API responses

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub status: String,
}

// OpenAPI documentation
// Temporarily disabled OpenAPI documentation for compilation
// #[derive(OpenApi)]
pub struct ApiDoc;

/// Start the API server with the given configuration
pub async fn start_server(config: APIConfig) -> Result<()> {
    let server = APIServer::new(config)?;
    server.start().await
}
