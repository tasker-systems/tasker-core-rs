//! No-op cache provider (TAS-156)
//!
//! Always returns None/success. Used when caching is disabled or
//! when Redis is unavailable (graceful degradation).

use crate::cache::errors::CacheResult;
use crate::cache::traits::CacheService;
use std::time::Duration;

/// No-op cache service that never caches anything
///
/// All reads return None, all writes succeed silently.
/// Used as the default when caching is disabled.
#[derive(Debug, Clone, Default)]
pub struct NoOpCacheService;

impl NoOpCacheService {
    /// Create a new no-op cache service
    pub fn new() -> Self {
        Self
    }
}

impl CacheService for NoOpCacheService {
    async fn get(&self, _key: &str) -> CacheResult<Option<String>> {
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: &str, _ttl: Duration) -> CacheResult<()> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> CacheResult<()> {
        Ok(())
    }

    async fn delete_pattern(&self, _pattern: &str) -> CacheResult<u64> {
        Ok(0)
    }

    async fn health_check(&self) -> CacheResult<bool> {
        Ok(true)
    }

    fn provider_name(&self) -> &'static str {
        "noop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_get_returns_none() {
        let svc = NoOpCacheService::new();
        assert_eq!(svc.get("any_key").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_noop_set_succeeds() {
        let svc = NoOpCacheService::new();
        svc.set("key", "value", Duration::from_secs(60))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_noop_delete_succeeds() {
        let svc = NoOpCacheService::new();
        svc.delete("key").await.unwrap();
    }

    #[tokio::test]
    async fn test_noop_delete_pattern_returns_zero() {
        let svc = NoOpCacheService::new();
        assert_eq!(svc.delete_pattern("prefix:*").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_noop_health_check_returns_true() {
        let svc = NoOpCacheService::new();
        assert!(svc.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_noop_provider_name() {
        let svc = NoOpCacheService::new();
        assert_eq!(svc.provider_name(), "noop");
    }
}
