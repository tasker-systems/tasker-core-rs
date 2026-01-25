//! Cache provider enum dispatch (TAS-156, TAS-168)
//!
//! Uses enum dispatch (like MessagingProvider) for zero-cost abstraction.
//! No vtable overhead - the compiler can inline provider methods.

use super::errors::{CacheError, CacheResult};
use super::providers::NoOpCacheService;
use super::traits::CacheService;
use crate::config::tasker::CacheConfig;
use std::time::Duration;
use tracing::{info, warn};

#[cfg(feature = "cache-redis")]
use super::providers::RedisCacheService;

#[cfg(feature = "cache-moka")]
use super::providers::MokaCacheService;

/// Cache provider enum for zero-cost dispatch
///
/// Matches the pattern used by `MessagingProvider` in the messaging module.
/// The NoOp variant is used when caching is disabled or when backend
/// initialization fails (graceful degradation).
///
/// ## Variants
///
/// - **Redis**: Distributed cache for multi-instance deployments
/// - **Moka**: In-process cache for single-instance or DoS protection
/// - **NoOp**: Always-miss fallback when caching is disabled
#[derive(Debug, Clone)]
pub enum CacheProvider {
    /// Redis cache provider (boxed to reduce enum size)
    #[cfg(feature = "cache-redis")]
    Redis(Box<RedisCacheService>),

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
            "redis" => Self::create_redis_provider(config).await,
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

    /// Create a NoOp provider (for explicit opt-out or testing)
    pub fn noop() -> Self {
        Self::NoOp(NoOpCacheService::new())
    }

    /// Check if caching is actually enabled (not NoOp)
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::NoOp(_))
    }

    /// Check if this provider is distributed (safe for multi-instance deployments)
    ///
    /// Returns `true` for Redis (shared state) and NoOp (no state).
    /// Returns `false` for Moka (in-process only).
    ///
    /// Use this to determine if the cache is safe for template caching
    /// in multi-instance deployments where workers may invalidate templates.
    pub fn is_distributed(&self) -> bool {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(_) => true,
            #[cfg(feature = "cache-moka")]
            Self::Moka(_) => false, // In-process only
            Self::NoOp(_) => true, // No state = safe
        }
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(s) => s.provider_name(),
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
            backend: "memcached".to_string(),
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
}
