//! Cache provider enum dispatch (TAS-156, TAS-168, TAS-171)
//!
//! Uses enum dispatch (like MessagingProvider) for zero-cost abstraction.
//! No vtable overhead - the compiler can inline provider methods.

use super::circuit_breaker::CircuitBreakerCache;
use super::errors::{CacheError, CacheResult};
use super::providers::NoOpCacheService;
use super::traits::CacheService;
use crate::config::tasker::{CacheConfig, CircuitBreakerConfig};
use crate::resilience::CircuitBreaker;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

#[cfg(feature = "cache-redis")]
use super::providers::RedisCacheService;

#[cfg(feature = "cache-moka")]
use super::providers::MokaCacheService;

#[cfg(feature = "cache-memcached")]
use super::providers::MemcachedCacheService;

/// Cache provider enum for zero-cost dispatch
///
/// Matches the pattern used by `MessagingProvider` in the messaging module.
/// The NoOp variant is used when caching is disabled or when backend
/// initialization fails (graceful degradation).
///
/// ## Variants
///
/// - **Redis**: Distributed cache for multi-instance deployments (Redis/Dragonfly)
/// - **Memcached**: Distributed cache using memcached protocol (TAS-171)
/// - **Moka**: In-process cache for single-instance or DoS protection
/// - **NoOp**: Always-miss fallback when caching is disabled
#[derive(Debug, Clone)]
pub enum CacheProvider {
    /// Redis cache provider (boxed to reduce enum size)
    #[cfg(feature = "cache-redis")]
    Redis(Box<RedisCacheService>),

    /// Memcached cache provider (TAS-171)
    #[cfg(feature = "cache-memcached")]
    Memcached(Box<MemcachedCacheService>),

    /// Moka in-memory cache provider (TAS-168)
    #[cfg(feature = "cache-moka")]
    Moka(Box<MokaCacheService>),

    /// No-op cache provider (always miss, always succeed)
    NoOp(NoOpCacheService),
}

impl CacheProvider {
    /// Create a cache provider from configuration with graceful degradation
    ///
    /// If Redis is configured but fails to connect, logs a warning and
    /// returns a NoOp provider instead. The system never fails to start
    /// due to cache issues.
    pub async fn from_config_graceful(config: &CacheConfig) -> Self {
        if !config.enabled {
            info!("Distributed cache disabled by configuration");
            return Self::NoOp(NoOpCacheService::new());
        }

        match config.backend.as_str() {
            // TAS-171: "dragonfly" is an alias for Redis (same protocol)
            "redis" | "dragonfly" => Self::create_redis_provider(config).await,
            // TAS-171: Memcached support
            "memcached" => Self::create_memcached_provider(config).await,
            "moka" | "memory" | "in-memory" => Self::create_moka_provider(config),
            other => {
                warn!(
                    backend = other,
                    "Unknown cache backend, falling back to NoOp"
                );
                Self::NoOp(NoOpCacheService::new())
            }
        }
    }

    /// Attempt to create a Redis provider, falling back to NoOp on failure
    #[cfg(feature = "cache-redis")]
    async fn create_redis_provider(config: &CacheConfig) -> Self {
        let redis_config = match &config.redis {
            Some(rc) => rc,
            None => {
                warn!(
                    "Redis cache enabled but no [cache.redis] config found, falling back to NoOp"
                );
                return Self::NoOp(NoOpCacheService::new());
            }
        };

        match RedisCacheService::from_config(redis_config).await {
            Ok(service) => {
                info!(
                    backend = "redis",
                    "Distributed cache provider initialized successfully"
                );
                Self::Redis(Box::new(service))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to connect to Redis, falling back to NoOp cache (graceful degradation)"
                );
                Self::NoOp(NoOpCacheService::new())
            }
        }
    }

    /// Fallback when cache-redis feature is not enabled
    #[cfg(not(feature = "cache-redis"))]
    async fn create_redis_provider(_config: &CacheConfig) -> Self {
        warn!("Redis cache backend requested but 'cache-redis' feature not enabled, using NoOp");
        Self::NoOp(NoOpCacheService::new())
    }

    /// Create a Moka in-memory cache provider (TAS-168)
    #[cfg(feature = "cache-moka")]
    fn create_moka_provider(config: &CacheConfig) -> Self {
        let moka_config = config.moka.clone().unwrap_or_default();
        let default_ttl = Duration::from_secs(config.default_ttl_seconds as u64);

        let service = MokaCacheService::from_config(&moka_config, default_ttl);
        info!(
            backend = "moka",
            max_capacity = moka_config.max_capacity,
            ttl_seconds = config.default_ttl_seconds,
            "In-memory cache provider initialized successfully"
        );
        Self::Moka(Box::new(service))
    }

    /// Fallback when cache-moka feature is not enabled
    #[cfg(not(feature = "cache-moka"))]
    fn create_moka_provider(_config: &CacheConfig) -> Self {
        warn!("Moka cache backend requested but 'cache-moka' feature not enabled, using NoOp");
        Self::NoOp(NoOpCacheService::new())
    }

    /// Create a Memcached provider (TAS-171)
    #[cfg(feature = "cache-memcached")]
    async fn create_memcached_provider(config: &CacheConfig) -> Self {
        let memcached_config = match &config.memcached {
            Some(mc) => mc,
            None => {
                warn!(
                    "Memcached cache enabled but no [cache.memcached] config found, falling back to NoOp"
                );
                return Self::NoOp(NoOpCacheService::new());
            }
        };

        match MemcachedCacheService::from_config(memcached_config).await {
            Ok(service) => {
                info!(
                    backend = "memcached",
                    "Distributed cache provider (memcached) initialized successfully"
                );
                Self::Memcached(Box::new(service))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to connect to Memcached, falling back to NoOp cache (graceful degradation)"
                );
                Self::NoOp(NoOpCacheService::new())
            }
        }
    }

    /// Fallback when cache-memcached feature is not enabled
    #[cfg(not(feature = "cache-memcached"))]
    async fn create_memcached_provider(_config: &CacheConfig) -> Self {
        warn!("Memcached cache backend requested but 'cache-memcached' feature not enabled, using NoOp");
        Self::NoOp(NoOpCacheService::new())
    }

    /// Create a NoOp provider (for explicit opt-out or testing)
    pub fn noop() -> Self {
        Self::NoOp(NoOpCacheService::new())
    }

    /// Create a cache provider with circuit breaker protection (TAS-171)
    ///
    /// The circuit breaker only applies to distributed (Redis/Dragonfly) providers.
    /// When the circuit is open, cache operations fail fast with graceful fallback:
    /// - `get()` returns `Ok(None)` (cache miss)
    /// - `set()`/`delete()` return `Ok(())` (no-op)
    ///
    /// ## Arguments
    ///
    /// * `cache_config` - Cache configuration (backend, TTL, etc.)
    /// * `cb_config` - Circuit breaker configuration (thresholds, timeouts)
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let cache = CacheProvider::from_config_with_circuit_breaker(
    ///     &cache_config,
    ///     &circuit_breaker_config,
    /// ).await;
    /// ```
    pub async fn from_config_with_circuit_breaker(
        cache_config: &CacheConfig,
        cb_config: &CircuitBreakerConfig,
    ) -> CircuitBreakerCache {
        let provider = Self::from_config_graceful(cache_config).await;

        // Only create circuit breaker for distributed + enabled providers
        // (i.e., those that make network calls and can fail)
        let circuit_breaker = if provider.is_distributed() && provider.is_enabled() {
            let component_config = cb_config.config_for_component("cache");
            let cb =
                CircuitBreaker::new("cache".to_string(), component_config.to_resilience_config());
            info!(
                failure_threshold = component_config.failure_threshold,
                timeout_seconds = component_config.timeout_seconds,
                "Cache circuit breaker initialized"
            );
            Some(Arc::new(cb))
        } else {
            None
        };

        CircuitBreakerCache::new(provider, circuit_breaker)
    }

    /// Check if caching is actually enabled (not NoOp)
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::NoOp(_))
    }

    /// Check if this provider is distributed (safe for multi-instance deployments)
    ///
    /// Delegates to the underlying provider's `is_distributed()` method.
    /// This allows each provider to declare its own characteristics.
    ///
    /// Returns `true` for Redis/Dragonfly (shared state) and NoOp (no state).
    /// Returns `false` for Moka (in-process only).
    ///
    /// Use this to determine if the cache is safe for template caching
    /// in multi-instance deployments where workers may invalidate templates.
    pub fn is_distributed(&self) -> bool {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.is_distributed(),
            #[cfg(feature = "cache-memcached")]
            Self::Memcached(s) => s.is_distributed(),
            #[cfg(feature = "cache-moka")]
            Self::Moka(s) => s.is_distributed(),
            Self::NoOp(s) => s.is_distributed(),
        }
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.provider_name(),
            #[cfg(feature = "cache-memcached")]
            Self::Memcached(s) => s.provider_name(),
            #[cfg(feature = "cache-moka")]
            Self::Moka(s) => s.provider_name(),
            Self::NoOp(s) => s.provider_name(),
        }
    }

    /// Get a value from cache
    pub async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.get(key).await,
            #[cfg(feature = "cache-memcached")]
            Self::Memcached(s) => s.get(key).await,
            #[cfg(feature = "cache-moka")]
            Self::Moka(s) => s.get(key).await,
            Self::NoOp(s) => s.get(key).await,
        }
    }

    /// Set a value in cache with TTL
    pub async fn set(&self, key: &str, value: &str, ttl: Duration) -> CacheResult<()> {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.set(key, value, ttl).await,
            #[cfg(feature = "cache-memcached")]
            Self::Memcached(s) => s.set(key, value, ttl).await,
            #[cfg(feature = "cache-moka")]
            Self::Moka(s) => s.set(key, value, ttl).await,
            Self::NoOp(s) => s.set(key, value, ttl).await,
        }
    }

    /// Delete a specific key
    pub async fn delete(&self, key: &str) -> CacheResult<()> {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.delete(key).await,
            #[cfg(feature = "cache-memcached")]
            Self::Memcached(s) => s.delete(key).await,
            #[cfg(feature = "cache-moka")]
            Self::Moka(s) => s.delete(key).await,
            Self::NoOp(s) => s.delete(key).await,
        }
    }

    /// Delete keys matching a pattern (uses SCAN, non-blocking)
    pub async fn delete_pattern(&self, pattern: &str) -> CacheResult<u64> {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.delete_pattern(pattern).await,
            #[cfg(feature = "cache-memcached")]
            Self::Memcached(s) => s.delete_pattern(pattern).await,
            #[cfg(feature = "cache-moka")]
            Self::Moka(s) => s.delete_pattern(pattern).await,
            Self::NoOp(s) => s.delete_pattern(pattern).await,
        }
    }

    /// Health check the cache backend
    pub async fn health_check(&self) -> CacheResult<bool> {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.health_check().await,
            #[cfg(feature = "cache-memcached")]
            Self::Memcached(s) => s.health_check().await,
            #[cfg(feature = "cache-moka")]
            Self::Moka(s) => s.health_check().await,
            Self::NoOp(s) => s.health_check().await,
        }
    }
}

impl From<CacheError> for crate::errors::TaskerError {
    fn from(e: CacheError) -> Self {
        crate::errors::TaskerError::CacheError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_provider_is_not_enabled() {
        let provider = CacheProvider::noop();
        assert!(!provider.is_enabled());
        assert_eq!(provider.provider_name(), "noop");
    }

    #[tokio::test]
    async fn test_from_config_disabled() {
        let config = CacheConfig {
            enabled: false,
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        assert!(!provider.is_enabled());
    }

    #[tokio::test]
    async fn test_from_config_unknown_backend() {
        let config = CacheConfig {
            enabled: true,
            backend: "unknown_backend".to_string(),
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        assert!(!provider.is_enabled());
    }

    #[cfg(feature = "cache-redis")]
    #[tokio::test]
    async fn test_from_config_redis_no_redis_config() {
        let config = CacheConfig {
            enabled: true,
            backend: "redis".to_string(),
            redis: None,
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        // Falls back to NoOp when redis config is missing
        assert!(!provider.is_enabled());
    }

    #[cfg(feature = "cache-moka")]
    #[tokio::test]
    async fn test_from_config_moka() {
        let config = CacheConfig {
            enabled: true,
            backend: "moka".to_string(),
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        assert!(provider.is_enabled());
        assert_eq!(provider.provider_name(), "moka");
        assert!(!provider.is_distributed()); // Moka is not distributed
    }

    #[cfg(feature = "cache-moka")]
    #[tokio::test]
    async fn test_from_config_memory_alias() {
        let config = CacheConfig {
            enabled: true,
            backend: "memory".to_string(),
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        assert!(provider.is_enabled());
        assert_eq!(provider.provider_name(), "moka");
    }

    #[cfg(feature = "cache-moka")]
    #[tokio::test]
    async fn test_from_config_in_memory_alias() {
        let config = CacheConfig {
            enabled: true,
            backend: "in-memory".to_string(),
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        assert!(provider.is_enabled());
        assert_eq!(provider.provider_name(), "moka");
    }

    #[tokio::test]
    async fn test_noop_is_distributed() {
        let provider = CacheProvider::noop();
        assert!(provider.is_distributed()); // NoOp is "safe" (no state)
    }

    /// TAS-171: Test dragonfly alias resolves to Redis provider
    #[cfg(feature = "cache-redis")]
    #[tokio::test]
    async fn test_from_config_dragonfly_alias() {
        let config = CacheConfig {
            enabled: true,
            backend: "dragonfly".to_string(),
            redis: None, // No redis config = falls back to NoOp
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        // Falls back to NoOp when redis config is missing (same behavior as "redis")
        assert!(!provider.is_enabled());
    }

    /// TAS-171: Test memcached backend without config falls back to NoOp
    #[cfg(feature = "cache-memcached")]
    #[tokio::test]
    async fn test_from_config_memcached_no_config() {
        let config = CacheConfig {
            enabled: true,
            backend: "memcached".to_string(),
            memcached: None, // No memcached config = falls back to NoOp
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;
        assert!(!provider.is_enabled());
    }
}
