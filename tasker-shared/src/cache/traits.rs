//! Cache service trait definition (TAS-156, TAS-171)

use super::errors::CacheResult;
use std::time::Duration;

/// Trait defining cache operations and provider characteristics
///
/// Implemented by concrete cache providers (Redis, Moka, NoOp).
/// All operations are async and return `CacheResult` for error handling.
///
/// ## Provider Characteristics
///
/// Each provider declares its own characteristics via trait methods:
/// - `is_distributed()`: Whether the cache is shared across instances
/// - `provider_name()`: Human-readable name for logging/metrics
///
/// This allows the framework to make decisions (e.g., circuit breaker protection,
/// usage constraints) based on provider capabilities rather than type matching.
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

    /// Whether this provider is distributed (safe for multi-instance deployments)
    ///
    /// Distributed providers share state across instances, which means:
    /// - Cache invalidations propagate to all instances
    /// - Network calls are involved (may need circuit breaker protection)
    /// - Safe for template caching where workers invalidate on bootstrap
    ///
    /// Returns `true` for Redis/Dragonfly (shared state) and NoOp (no state).
    /// Returns `false` for Moka (in-process only).
    fn is_distributed(&self) -> bool;
}
