//! In-memory cache provider using Moka (TAS-168)
//!
//! Provides in-process caching with TTL support for single-instance deployments.
//! Useful for analytics caching where brief staleness is acceptable and
//! DoS protection is desired even without external cache infrastructure.
//!
//! **Important**: This cache is NOT distributed. Each process maintains its own
//! cache state. Do NOT use for template caching in multi-instance deployments
//! where workers may invalidate templates independently.

use crate::cache::errors::CacheResult;
use crate::cache::traits::CacheService;
use crate::config::tasker::MokaConfig;
use std::time::Duration;
use tracing::debug;

/// In-memory cache service using Moka
///
/// Provides async in-process caching with TTL support.
/// All entries share the same TTL configured at construction time.
#[derive(Clone)]
pub struct MokaCacheService {
    cache: moka::future::Cache<String, String>,
    default_ttl: Duration,
}

impl std::fmt::Debug for MokaCacheService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MokaCacheService")
            .field("max_capacity", &self.cache.policy().max_capacity())
            .field("entry_count", &self.cache.entry_count())
            .field("default_ttl", &self.default_ttl)
            .finish()
    }
}

impl MokaCacheService {
    /// Create a new Moka cache service from configuration
    pub fn from_config(config: &MokaConfig, default_ttl: Duration) -> Self {
        let cache = moka::future::Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(default_ttl)
            .build();

        debug!(
            max_capacity = config.max_capacity,
            ttl_seconds = default_ttl.as_secs(),
            "Moka in-memory cache service created"
        );

        Self { cache, default_ttl }
    }

    /// Create with default configuration (for testing)
    pub fn new(max_capacity: u64, default_ttl: Duration) -> Self {
        Self::from_config(&MokaConfig { max_capacity }, default_ttl)
    }
}

impl CacheService for MokaCacheService {
    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        let result = self.cache.get(key).await;

        if result.is_some() {
            debug!(key = key, "Cache HIT (moka)");
        } else {
            debug!(key = key, "Cache MISS (moka)");
        }

        Ok(result)
    }

    async fn set(&self, key: &str, value: &str, _ttl: Duration) -> CacheResult<()> {
        // Moka uses cache-level TTL, not per-entry TTL
        // The TTL is set at cache construction time
        self.cache.insert(key.to_string(), value.to_string()).await;

        debug!(
            key = key,
            ttl_seconds = self.default_ttl.as_secs(),
            "Cache SET (moka)"
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> CacheResult<()> {
        self.cache.invalidate(key).await;
        debug!(key = key, "Cache DEL (moka)");
        Ok(())
    }

    async fn delete_pattern(&self, pattern: &str) -> CacheResult<u64> {
        // Moka doesn't support efficient pattern iteration
        // For analytics use case, we rely on TTL expiry
        // This is acceptable because analytics data is informational
        debug!(
            pattern = pattern,
            "Pattern deletion not supported for Moka, relying on TTL expiry"
        );
        Ok(0)
    }

    async fn health_check(&self) -> CacheResult<bool> {
        // In-memory cache is always healthy
        Ok(true)
    }

    fn provider_name(&self) -> &'static str {
        "moka"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_moka_get_returns_none_on_miss() {
        let svc = MokaCacheService::new(100, Duration::from_secs(60));
        assert_eq!(svc.get("nonexistent").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_moka_set_and_get() {
        let svc = MokaCacheService::new(100, Duration::from_secs(60));
        let key = "test_key";
        let value = r#"{"name":"test"}"#;

        svc.set(key, value, Duration::from_secs(60)).await.unwrap();

        let result = svc.get(key).await.unwrap();
        assert_eq!(result, Some(value.to_string()));
    }

    #[tokio::test]
    async fn test_moka_delete() {
        let svc = MokaCacheService::new(100, Duration::from_secs(60));
        let key = "to_delete";

        svc.set(key, "value", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(svc.get(key).await.unwrap().is_some());

        svc.delete(key).await.unwrap();
        assert!(svc.get(key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_moka_delete_pattern_returns_zero() {
        let svc = MokaCacheService::new(100, Duration::from_secs(60));
        // Pattern deletion is not supported, should return 0
        assert_eq!(svc.delete_pattern("prefix:*").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_moka_health_check_returns_true() {
        let svc = MokaCacheService::new(100, Duration::from_secs(60));
        assert!(svc.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_moka_provider_name() {
        let svc = MokaCacheService::new(100, Duration::from_secs(60));
        assert_eq!(svc.provider_name(), "moka");
    }

    #[tokio::test]
    async fn test_moka_capacity_eviction() {
        // Create cache with tiny capacity
        let svc = MokaCacheService::new(2, Duration::from_secs(60));

        // Insert more entries than capacity
        svc.set("key1", "value1", Duration::from_secs(60))
            .await
            .unwrap();
        svc.set("key2", "value2", Duration::from_secs(60))
            .await
            .unwrap();
        svc.set("key3", "value3", Duration::from_secs(60))
            .await
            .unwrap();

        // Run pending maintenance to trigger eviction
        svc.cache.run_pending_tasks().await;

        // Should have at most 2 entries
        assert!(svc.cache.entry_count() <= 2);
    }

    #[tokio::test]
    async fn test_moka_ttl_expiry() {
        let svc = MokaCacheService::new(100, Duration::from_millis(50));

        svc.set("expiring", "value", Duration::from_millis(50))
            .await
            .unwrap();

        // Should exist immediately
        assert!(svc.get("expiring").await.unwrap().is_some());

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Run maintenance to process expirations
        svc.cache.run_pending_tasks().await;

        // Should be gone
        assert!(svc.get("expiring").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_moka_debug_format() {
        let svc = MokaCacheService::new(100, Duration::from_secs(60));
        let debug_str = format!("{:?}", svc);
        assert!(debug_str.contains("MokaCacheService"));
        assert!(debug_str.contains("max_capacity"));
    }
}
