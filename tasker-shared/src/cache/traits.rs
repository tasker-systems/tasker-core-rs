//! Cache service trait definition (TAS-156)

use super::errors::CacheResult;
use std::time::Duration;

/// Trait defining cache operations
///
/// Implemented by concrete cache providers (Redis, NoOp).
/// All operations are async and return `CacheResult` for error handling.
pub trait CacheService: Send + Sync {
    /// Get a value from the cache by key
    ///
    /// Returns `Ok(Some(value))` on cache hit, `Ok(None)` on cache miss.
    fn get(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = CacheResult<Option<String>>> + Send;

    /// Set a value in the cache with a TTL
    fn set(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = CacheResult<()>> + Send;

    /// Delete a specific key from the cache
    fn delete(&self, key: &str) -> impl std::future::Future<Output = CacheResult<()>> + Send;

    /// Delete all keys matching a pattern (uses SCAN for non-blocking iteration)
    fn delete_pattern(
        &self,
        pattern: &str,
    ) -> impl std::future::Future<Output = CacheResult<u64>> + Send;

    /// Check if the cache backend is healthy
    fn health_check(&self) -> impl std::future::Future<Output = CacheResult<bool>> + Send;

    /// Get the name of the cache provider
    fn provider_name(&self) -> &'static str;
}
