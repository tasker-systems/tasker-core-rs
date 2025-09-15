//! # Custom Axum Extractors
//!
//! Custom extractors for common web API patterns like database connections,
//! authenticated users, and request context.

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sqlx::PgPool;
use tracing::debug;

use crate::web::middleware::request_id::RequestId;
use crate::web::state::AppState;
use tasker_shared::types::{
    auth::WorkerClaims,
    web::{ApiError, DbOperationType},
};

/// Database connection extractor with smart pool selection
pub struct DatabaseConnection {
    pub pool: PgPool,
    pub operation_type: DbOperationType,
}

impl DatabaseConnection {
    /// Create a new database connection for write operations
    pub fn for_write(state: &AppState) -> Self {
        Self {
            pool: state.select_db_pool(DbOperationType::WebWrite).clone(),
            operation_type: DbOperationType::WebWrite,
        }
    }

    /// Create a new database connection for read operations
    pub fn for_read(state: &AppState) -> Self {
        Self {
            pool: state.select_db_pool(DbOperationType::ReadOnly).clone(),
            operation_type: DbOperationType::ReadOnly,
        }
    }

    /// Create a new database connection for analytics queries
    pub fn for_analytics(state: &AppState) -> Self {
        Self {
            pool: state.select_db_pool(DbOperationType::Analytics).clone(),
            operation_type: DbOperationType::Analytics,
        }
    }
}

/// Authenticated worker extractor
///
/// Extracts worker claims from request extensions (added by auth middleware).
/// Fails if no valid authentication is present.
pub struct AuthenticatedWorker {
    pub claims: WorkerClaims,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedWorker
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let claims = parts
            .extensions
            .get::<WorkerClaims>()
            .ok_or(ApiError::Unauthorized)?
            .clone();

        debug!(worker_id = %claims.sub, "Extracted authenticated worker");

        Ok(Self { claims })
    }
}

/// Optional authenticated worker extractor
///
/// Similar to AuthenticatedWorker but returns None if no valid authentication
/// is present instead of failing.
pub struct OptionalAuthenticatedWorker {
    pub claims: Option<WorkerClaims>,
}

#[async_trait]
impl<S> FromRequestParts<S> for OptionalAuthenticatedWorker
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let claims = parts.extensions.get::<WorkerClaims>().cloned();

        if let Some(ref claims) = claims {
            debug!(worker_id = %claims.sub, "Extracted optional authenticated worker");
        }

        Ok(Self { claims })
    }
}

/// Request context extractor
///
/// Extracts common request context information like request ID and tracing span.
pub struct RequestContext {
    pub request_id: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .map(|rid| rid.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(Self { request_id })
    }
}
