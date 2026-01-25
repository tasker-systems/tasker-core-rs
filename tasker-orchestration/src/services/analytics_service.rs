//! Analytics Service with Caching (TAS-168)
//!
//! Cache-aware analytics service that wraps [`AnalyticsQueryService`] with
//! the cache-aside pattern. This service:
//!
//! - Checks cache before executing database queries
//! - Populates cache on successful database fetches
//! - Handles cache errors gracefully (best-effort)
//! - Never blocks or fails due to cache issues
//!
//! ## Cache Key Format
//!
//! - Performance: `{prefix}:analytics:performance:{hours}`
//! - Bottlenecks: `{prefix}:analytics:bottlenecks:{limit}:{min_executions}`

use super::AnalyticsQueryService;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::cache::CacheProvider;
use tasker_shared::config::tasker::CacheConfig;
use tasker_shared::types::api::orchestration::{BottleneckAnalysis, PerformanceMetrics};
use tasker_shared::TaskerResult;
use tracing::{debug, warn};

/// Analytics service with response caching
///
/// Provides cached access to analytics data. The cache is optional - if
/// unavailable or disabled, requests fall through to the database.
///
/// ## Example
///
/// ```ignore
/// let service = AnalyticsService::new(
///     db_pool,
///     cache_provider,
///     Some(cache_config),
/// );
///
/// // First call: cache miss -> DB query -> cache populated
/// let metrics = service.get_performance_metrics(24).await?;
///
/// // Subsequent calls within TTL: cache hit -> fast response
/// let metrics = service.get_performance_metrics(24).await?;
/// ```
#[derive(Clone)]
pub struct AnalyticsService {
    query_service: AnalyticsQueryService,
    cache_provider: Arc<CacheProvider>,
    cache_config: Option<CacheConfig>,
}

impl std::fmt::Debug for AnalyticsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalyticsService")
            .field("query_service", &self.query_service)
            .field("cache_enabled", &self.cache_provider.is_enabled())
            .field("cache_provider", &self.cache_provider.provider_name())
            .finish()
    }
}

impl AnalyticsService {
    /// Create a new analytics service with caching
    pub fn new(
        db_pool: PgPool,
        cache_provider: Arc<CacheProvider>,
        cache_config: Option<CacheConfig>,
    ) -> Self {
        Self {
            query_service: AnalyticsQueryService::new(db_pool),
            cache_provider,
            cache_config,
        }
    }

    /// Get performance metrics with caching
    ///
    /// Uses cache-aside pattern:
    /// 1. Check cache for existing entry
    /// 2. On miss, fetch from database
    /// 3. Populate cache (best-effort)
    /// 4. Return metrics
    pub async fn get_performance_metrics(&self, hours: u32) -> TaskerResult<PerformanceMetrics> {
        let cache_key = self.performance_cache_key(hours);

        // Try cache first
        if let Some(cached) = self.try_cache_get::<PerformanceMetrics>(&cache_key).await {
            debug!(hours = hours, "Performance metrics cache HIT");
            return Ok(cached);
        }

        debug!(hours = hours, "Performance metrics cache MISS, querying DB");

        // Cache miss - fetch from DB
        let metrics = self.query_service.get_performance_metrics(hours).await?;

        // Populate cache (best-effort)
        self.try_cache_set(&cache_key, &metrics).await;

        Ok(metrics)
    }

    /// Get bottleneck analysis with caching
    ///
    /// Uses cache-aside pattern with parameter-based cache keys.
    pub async fn get_bottleneck_analysis(
        &self,
        limit: i32,
        min_executions: i32,
    ) -> TaskerResult<BottleneckAnalysis> {
        let cache_key = self.bottleneck_cache_key(limit, min_executions);

        // Try cache first
        if let Some(cached) = self.try_cache_get::<BottleneckAnalysis>(&cache_key).await {
            debug!(
                limit = limit,
                min_executions = min_executions,
                "Bottleneck analysis cache HIT"
            );
            return Ok(cached);
        }

        debug!(
            limit = limit,
            min_executions = min_executions,
            "Bottleneck analysis cache MISS, querying DB"
        );

        // Cache miss - fetch from DB
        let analysis = self
            .query_service
            .get_bottleneck_analysis(limit, min_executions)
            .await?;

        // Populate cache (best-effort)
        self.try_cache_set(&cache_key, &analysis).await;

        Ok(analysis)
    }

    /// Check if caching is enabled and working
    pub fn cache_enabled(&self) -> bool {
        self.cache_provider.is_enabled()
    }

    /// Get the cache provider name (for diagnostics)
    pub fn cache_provider_name(&self) -> &'static str {
        self.cache_provider.provider_name()
    }

    // =========================================================================
    // Cache key generation
    // =========================================================================

    fn performance_cache_key(&self, hours: u32) -> String {
        let prefix = self.key_prefix();
        format!("{prefix}:analytics:performance:{hours}")
    }

    fn bottleneck_cache_key(&self, limit: i32, min_executions: i32) -> String {
        let prefix = self.key_prefix();
        format!("{prefix}:analytics:bottlenecks:{limit}:{min_executions}")
    }

    fn key_prefix(&self) -> &str {
        self.cache_config
            .as_ref()
            .map(|c| c.key_prefix.as_str())
            .unwrap_or("tasker")
    }

    fn analytics_ttl(&self) -> Duration {
        Duration::from_secs(
            self.cache_config
                .as_ref()
                .map(|c| u64::from(c.analytics_ttl_seconds))
                .unwrap_or(60),
        )
    }

    // =========================================================================
    // Best-effort cache operations
    // =========================================================================

    /// Try to get a value from cache (returns None on miss or error)
    async fn try_cache_get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        if !self.cache_provider.is_enabled() {
            return None;
        }

        match self.cache_provider.get(key).await {
            Ok(Some(cached)) => match serde_json::from_str::<T>(&cached) {
                Ok(value) => Some(value),
                Err(e) => {
                    warn!(
                        key = key,
                        error = %e,
                        "Analytics cache deserialization failed"
                    );
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                warn!(
                    key = key,
                    error = %e,
                    "Analytics cache get failed"
                );
                None
            }
        }
    }

    /// Try to set a value in cache (best-effort, never fails)
    async fn try_cache_set<T: Serialize>(&self, key: &str, value: &T) {
        if !self.cache_provider.is_enabled() {
            return;
        }

        let ttl = self.analytics_ttl();

        match serde_json::to_string(value) {
            Ok(serialized) => {
                if let Err(e) = self.cache_provider.set(key, &serialized, ttl).await {
                    warn!(
                        key = key,
                        error = %e,
                        "Analytics cache set failed (best-effort)"
                    );
                } else {
                    debug!(
                        key = key,
                        ttl_secs = ttl.as_secs(),
                        "Analytics cache populated"
                    );
                }
            }
            Err(e) => {
                warn!(
                    key = key,
                    error = %e,
                    "Analytics cache serialization failed"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_provider() -> Arc<CacheProvider> {
        Arc::new(CacheProvider::noop())
    }

    fn test_config() -> CacheConfig {
        CacheConfig {
            enabled: true,
            key_prefix: "test".to_string(),
            analytics_ttl_seconds: 30,
            ..CacheConfig::default()
        }
    }

    #[tokio::test]
    async fn test_performance_cache_key() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), Some(test_config()));

        assert_eq!(
            service.performance_cache_key(24),
            "test:analytics:performance:24"
        );
        assert_eq!(
            service.performance_cache_key(1),
            "test:analytics:performance:1"
        );
    }

    #[tokio::test]
    async fn test_bottleneck_cache_key() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), Some(test_config()));

        assert_eq!(
            service.bottleneck_cache_key(10, 5),
            "test:analytics:bottlenecks:10:5"
        );
        assert_eq!(
            service.bottleneck_cache_key(20, 10),
            "test:analytics:bottlenecks:20:10"
        );
    }

    #[tokio::test]
    async fn test_key_prefix_default() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), None);

        assert_eq!(service.key_prefix(), "tasker");
    }

    #[tokio::test]
    async fn test_key_prefix_from_config() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let config = CacheConfig {
            key_prefix: "myapp".to_string(),
            ..CacheConfig::default()
        };
        let service = AnalyticsService::new(pool, noop_provider(), Some(config));

        assert_eq!(service.key_prefix(), "myapp");
    }

    #[tokio::test]
    async fn test_analytics_ttl_default() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), None);

        assert_eq!(service.analytics_ttl(), Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_analytics_ttl_from_config() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let config = CacheConfig {
            analytics_ttl_seconds: 120,
            ..CacheConfig::default()
        };
        let service = AnalyticsService::new(pool, noop_provider(), Some(config));

        assert_eq!(service.analytics_ttl(), Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_cache_enabled_noop() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), None);

        // NoOp is not "enabled" (no actual caching)
        assert!(!service.cache_enabled());
    }

    #[tokio::test]
    async fn test_cache_provider_name() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), None);

        assert_eq!(service.cache_provider_name(), "noop");
    }

    #[tokio::test]
    async fn test_try_cache_get_noop_returns_none() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), None);

        let result: Option<PerformanceMetrics> =
            service.try_cache_get("test:analytics:performance:24").await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_cache_set_noop_succeeds() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), None);

        let metrics = PerformanceMetrics {
            total_tasks: 100,
            active_tasks: 10,
            completed_tasks: 85,
            failed_tasks: 5,
            completion_rate: 0.85,
            error_rate: 0.05,
            average_task_duration_seconds: 2.5,
            average_step_duration_seconds: 0.5,
            tasks_per_hour: 50,
            steps_per_hour: 200,
            system_health_score: 0.95,
            analysis_period_start: "2024-01-01T00:00:00Z".to_string(),
            calculated_at: "2024-01-01T01:00:00Z".to_string(),
        };

        // Should not panic
        service
            .try_cache_set("test:analytics:performance:24", &metrics)
            .await;
    }

    #[tokio::test]
    async fn test_debug_format() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsService::new(pool, noop_provider(), None);

        let debug_str = format!("{:?}", service);
        assert!(debug_str.contains("AnalyticsService"));
        assert!(debug_str.contains("cache_enabled"));
        assert!(debug_str.contains("noop"));
    }
}
