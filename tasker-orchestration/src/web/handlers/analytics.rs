//! # Analytics Handlers (TAS-168)
//!
//! Read-only endpoints for performance metrics and bottleneck analysis.
//! These handlers delegate to [`AnalyticsService`] for caching and
//! [`AnalyticsQueryService`] for database queries.
//!
//! ## Caching
//!
//! All analytics endpoints use response caching with configurable TTL
//! (default 60 seconds). Cache keys include all query parameters to
//! ensure different queries get different cache entries.

use axum::extract::{Query, State};
use axum::Json;
use tracing::info;

use crate::web::circuit_breaker::execute_with_circuit_breaker;
use crate::web::middleware::permission::require_permission;
use crate::web::state::AppState;
use tasker_shared::types::api::orchestration::{
    BottleneckAnalysis, BottleneckQuery, MetricsQuery, PerformanceMetrics,
};
use tasker_shared::types::permissions::Permission;
use tasker_shared::types::security::SecurityContext;
use tasker_shared::types::web::{ApiError, ApiResult};

/// Get performance metrics: GET /v1/analytics/performance
///
/// **Required Permission:** `system:analytics_read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/analytics/performance",
    params(
        ("hours" = Option<u32>, Query, description = "Number of hours to look back (default: 24)")
    ),
    responses(
        (status = 200, description = "Performance metrics", body = PerformanceMetrics),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("system:analytics_read"))
    ),
    tag = "analytics"
))]
pub async fn get_performance_metrics(
    State(state): State<AppState>,
    security: SecurityContext,
    Query(params): Query<MetricsQuery>,
) -> ApiResult<Json<PerformanceMetrics>> {
    require_permission(&security, Permission::AnalyticsRead)?;

    let hours = params.hours.unwrap_or(24);

    info!(hours = hours, "Retrieving performance metrics");

    execute_with_circuit_breaker(&state, || async {
        // TAS-168: Delegate to AnalyticsService (handles caching internally)
        let metrics = state
            .analytics_service
            .get_performance_metrics(hours)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        Ok::<Json<PerformanceMetrics>, sqlx::Error>(Json(metrics))
    })
    .await
}

/// Get bottleneck analysis: GET /v1/analytics/bottlenecks
///
/// **Required Permission:** `system:analytics_read`
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/analytics/bottlenecks",
    params(
        ("limit" = Option<i32>, Query, description = "Maximum number of slow steps to return (default: 10)"),
        ("min_executions" = Option<i32>, Query, description = "Minimum number of executions for inclusion (default: 5)")
    ),
    responses(
        (status = 200, description = "Bottleneck analysis", body = BottleneckAnalysis),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    extensions(
        ("x-required-permission" = json!("system:analytics_read"))
    ),
    tag = "analytics"
))]
pub async fn get_bottlenecks(
    State(state): State<AppState>,
    security: SecurityContext,
    Query(params): Query<BottleneckQuery>,
) -> ApiResult<Json<BottleneckAnalysis>> {
    require_permission(&security, Permission::AnalyticsRead)?;

    let limit = params.limit.unwrap_or(10);
    let min_executions = params.min_executions.unwrap_or(5);

    info!(
        limit = limit,
        min_executions = min_executions,
        "Performing bottleneck analysis"
    );

    execute_with_circuit_breaker(&state, || async {
        // TAS-168: Delegate to AnalyticsService (handles caching internally)
        let analysis = state
            .analytics_service
            .get_bottleneck_analysis(limit, min_executions)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        Ok::<Json<BottleneckAnalysis>, sqlx::Error>(Json(analysis))
    })
    .await
}
