//! Cache provider with integrated circuit breaker (TAS-156, TAS-168, TAS-171)
//!
//! Uses enum dispatch (like MessagingProvider) for zero-cost abstraction.
//! Circuit breaker protection is an internal implementation detail - consumers
//! simply use `CacheProvider` and get automatic resilience for distributed backends.

use super::errors::{CacheError, CacheResult};
use super::providers::NoOpCacheService;
use super::traits::CacheService;
use crate::config::tasker::{CacheConfig, CircuitBreakerConfig};
use crate::resilience::{CircuitBreaker, CircuitState};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

#[cfg(feature = "cache-redis")]
use super::providers::RedisCacheService;

#[cfg(feature = "cache-moka")]
use super::providers::MokaCacheService;

#[cfg(feature = "cache-memcached")]
use super::providers::MemcachedCacheService;

/// Internal cache backend enum for zero-cost dispatch
///
/// This is an implementation detail. Consumers should use `CacheProvider`.
#[derive(Debug, Clone)]
enum CacheBackend {
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

impl CacheBackend {
    fn is_distributed(&self) -> bool {
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

    fn provider_name(&self) -> &'static str {
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

    fn is_enabled(&self) -> bool {
        !matches!(self, Self::NoOp(_))
    }

    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
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

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> CacheResult<()> {
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

    async fn delete(&self, key: &str) -> CacheResult<()> {
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

    async fn delete_pattern(&self, pattern: &str) -> CacheResult<u64> {
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

    async fn health_check(&self) -> CacheResult<bool> {
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

/// Cache provider with integrated circuit breaker protection (TAS-171)
///
/// Provides a unified caching interface with automatic resilience for distributed
/// backends. The circuit breaker is an internal implementation detail - consumers
/// use `CacheProvider` and get automatic fail-fast behavior when Redis/Memcached
/// is unavailable.
///
/// ## Backends
///
/// - **Redis/Dragonfly**: Distributed cache for multi-instance deployments
/// - **Memcached**: Distributed cache using memcached protocol
/// - **Moka**: In-process cache for single-instance or DoS protection
/// - **NoOp**: Always-miss fallback when caching is disabled
///
/// ## Circuit Breaker
///
/// For distributed backends (Redis, Memcached), a circuit breaker prevents
/// repeated timeout penalties when the backend is unavailable:
///
/// - When open: `get()` returns `Ok(None)`, `set()`/`delete()` return `Ok(())`
/// - Automatically recovers when the backend becomes available
///
/// In-memory backends (Moka) don't need circuit breaker protection.
#[derive(Clone)]
pub struct CacheProvider {
    backend: CacheBackend,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl std::fmt::Debug for CacheProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheProvider")
            .field("backend", &self.backend)
            .field(
                "circuit_breaker",
                &self.circuit_breaker.as_ref().map(|cb| cb.state()),
            )
            .finish()
    }
}

impl CacheProvider {
    /// Create a cache provider from configuration with graceful degradation
    ///
    /// If Redis is configured but fails to connect, logs a warning and
    /// returns a NoOp provider instead. The system never fails to start
    /// due to cache issues.
    ///
    /// For distributed backends, pass `cb_config` to enable circuit breaker
    /// protection. If `None`, no circuit breaker is used.
    ///
    /// ## Arguments
    ///
    /// * `config` - Cache configuration (backend, TTL, etc.)
    /// * `cb_config` - Optional circuit breaker configuration
    pub async fn from_config_graceful(
        config: &CacheConfig,
        cb_config: Option<&CircuitBreakerConfig>,
    ) -> Self {
        let backend = Self::create_backend(config).await;

        // Only create circuit breaker for distributed + enabled backends
        let circuit_breaker = if backend.is_distributed() && backend.is_enabled() {
            cb_config.map(|cb_cfg| {
                let component_config = cb_cfg.config_for_component("cache");
                let cb = CircuitBreaker::new(
                    "cache".to_string(),
                    component_config.to_resilience_config(),
                );
                info!(
                    failure_threshold = component_config.failure_threshold,
                    timeout_seconds = component_config.timeout_seconds,
                    "Cache circuit breaker initialized"
                );
                Arc::new(cb)
            })
        } else {
            None
        };

        Self {
            backend,
            circuit_breaker,
        }
    }

    /// Create the cache backend from configuration
    async fn create_backend(config: &CacheConfig) -> CacheBackend {
        if !config.enabled {
            info!("Distributed cache disabled by configuration");
            return CacheBackend::NoOp(NoOpCacheService::new());
        }

        match config.backend.as_str() {
            // TAS-171: "dragonfly" is an alias for Redis (same protocol)
            "redis" | "dragonfly" => Self::create_redis_backend(config).await,
            // TAS-171: Memcached support
            "memcached" => Self::create_memcached_backend(config).await,
            "moka" | "memory" | "in-memory" => Self::create_moka_backend(config),
            other => {
                warn!(
                    backend = other,
                    "Unknown cache backend, falling back to NoOp"
                );
                CacheBackend::NoOp(NoOpCacheService::new())
            }
        }
    }

    /// Attempt to create a Redis backend, falling back to NoOp on failure
    #[cfg(feature = "cache-redis")]
    async fn create_redis_backend(config: &CacheConfig) -> CacheBackend {
        let redis_config = match &config.redis {
            Some(rc) => rc,
            None => {
                warn!(
                    "Redis cache enabled but no [cache.redis] config found, falling back to NoOp"
                );
                return CacheBackend::NoOp(NoOpCacheService::new());
            }
        };

        match RedisCacheService::from_config(redis_config).await {
            Ok(service) => {
                info!(
                    backend = "redis",
                    "Distributed cache provider initialized successfully"
                );
                CacheBackend::Redis(Box::new(service))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to connect to Redis, falling back to NoOp cache (graceful degradation)"
                );
                CacheBackend::NoOp(NoOpCacheService::new())
            }
        }
    }

    /// Fallback when cache-redis feature is not enabled
    #[cfg(not(feature = "cache-redis"))]
    async fn create_redis_backend(_config: &CacheConfig) -> CacheBackend {
        warn!("Redis cache backend requested but 'cache-redis' feature not enabled, using NoOp");
        CacheBackend::NoOp(NoOpCacheService::new())
    }

    /// Create a Moka in-memory cache backend (TAS-168)
    #[cfg(feature = "cache-moka")]
    fn create_moka_backend(config: &CacheConfig) -> CacheBackend {
        let moka_config = config.moka.clone().unwrap_or_default();
        let default_ttl = Duration::from_secs(config.default_ttl_seconds as u64);

        let service = MokaCacheService::from_config(&moka_config, default_ttl);
        info!(
            backend = "moka",
            max_capacity = moka_config.max_capacity,
            ttl_seconds = config.default_ttl_seconds,
            "In-memory cache provider initialized successfully"
        );
        CacheBackend::Moka(Box::new(service))
    }

    /// Fallback when cache-moka feature is not enabled
    #[cfg(not(feature = "cache-moka"))]
    fn create_moka_backend(_config: &CacheConfig) -> CacheBackend {
        warn!("Moka cache backend requested but 'cache-moka' feature not enabled, using NoOp");
        CacheBackend::NoOp(NoOpCacheService::new())
    }

    /// Create a Memcached backend (TAS-171)
    #[cfg(feature = "cache-memcached")]
    async fn create_memcached_backend(config: &CacheConfig) -> CacheBackend {
        let memcached_config = match &config.memcached {
            Some(mc) => mc,
            None => {
                warn!(
                    "Memcached cache enabled but no [cache.memcached] config found, falling back to NoOp"
                );
                return CacheBackend::NoOp(NoOpCacheService::new());
            }
        };

        match MemcachedCacheService::from_config(memcached_config).await {
            Ok(service) => {
                info!(
                    backend = "memcached",
                    "Distributed cache provider (memcached) initialized successfully"
                );
                CacheBackend::Memcached(Box::new(service))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to connect to Memcached, falling back to NoOp cache (graceful degradation)"
                );
                CacheBackend::NoOp(NoOpCacheService::new())
            }
        }
    }

    /// Fallback when cache-memcached feature is not enabled
    #[cfg(not(feature = "cache-memcached"))]
    async fn create_memcached_backend(_config: &CacheConfig) -> CacheBackend {
        warn!(
            "Memcached cache backend requested but 'cache-memcached' feature not enabled, using NoOp"
        );
        CacheBackend::NoOp(NoOpCacheService::new())
    }

    /// Create a NoOp provider (for explicit opt-out or testing)
    pub fn noop() -> Self {
        Self {
            backend: CacheBackend::NoOp(NoOpCacheService::new()),
            circuit_breaker: None,
        }
    }

    /// Check if circuit breaker should be used for this operation
    fn should_use_circuit_breaker(&self) -> bool {
        self.circuit_breaker.is_some() && self.backend.is_distributed() && self.backend.is_enabled()
    }

    /// Check if caching is actually enabled (not NoOp)
    pub fn is_enabled(&self) -> bool {
        self.backend.is_enabled()
    }

    /// Check if this provider is distributed (safe for multi-instance deployments)
    ///
    /// Returns `true` for Redis/Dragonfly/Memcached (shared state) and NoOp (no state).
    /// Returns `false` for Moka (in-process only).
    ///
    /// Use this to determine if the cache is safe for template caching
    /// in multi-instance deployments where workers may invalidate templates.
    pub fn is_distributed(&self) -> bool {
        self.backend.is_distributed()
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        self.backend.provider_name()
    }

    /// Get current circuit breaker state (for monitoring)
    ///
    /// Returns `None` if no circuit breaker is configured.
    pub fn circuit_state(&self) -> Option<CircuitState> {
        self.circuit_breaker.as_ref().map(|cb| cb.state())
    }

    /// Get a value from cache
    ///
    /// If circuit is open, returns `Ok(None)` (cache miss behavior).
    pub async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        if !self.should_use_circuit_breaker() {
            return self.backend.get(key).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        if !cb.should_allow() {
            debug!(key = key, "Cache circuit open, returning miss");
            return Ok(None);
        }

        let start = Instant::now();
        let result = self.backend.get(key).await;
        let duration = start.elapsed();

        match &result {
            Ok(_) => cb.record_success_manual(duration),
            Err(_) => cb.record_failure_manual(duration),
        }

        result
    }

    /// Set a value in cache with TTL
    ///
    /// If circuit is open, returns `Ok(())` (no-op behavior).
    pub async fn set(&self, key: &str, value: &str, ttl: Duration) -> CacheResult<()> {
        if !self.should_use_circuit_breaker() {
            return self.backend.set(key, value, ttl).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        if !cb.should_allow() {
            debug!(key = key, "Cache circuit open, skipping set");
            return Ok(());
        }

        let start = Instant::now();
        let result = self.backend.set(key, value, ttl).await;
        let duration = start.elapsed();

        match &result {
            Ok(_) => cb.record_success_manual(duration),
            Err(_) => cb.record_failure_manual(duration),
        }

        result
    }

    /// Delete a specific key
    ///
    /// If circuit is open, returns `Ok(())` (no-op behavior).
    pub async fn delete(&self, key: &str) -> CacheResult<()> {
        if !self.should_use_circuit_breaker() {
            return self.backend.delete(key).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        if !cb.should_allow() {
            debug!(key = key, "Cache circuit open, skipping delete");
            return Ok(());
        }

        let start = Instant::now();
        let result = self.backend.delete(key).await;
        let duration = start.elapsed();

        match &result {
            Ok(_) => cb.record_success_manual(duration),
            Err(_) => cb.record_failure_manual(duration),
        }

        result
    }

    /// Delete keys matching a pattern (uses SCAN, non-blocking)
    ///
    /// If circuit is open, returns `Ok(0)` (no-op behavior).
    pub async fn delete_pattern(&self, pattern: &str) -> CacheResult<u64> {
        if !self.should_use_circuit_breaker() {
            return self.backend.delete_pattern(pattern).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        if !cb.should_allow() {
            debug!(
                pattern = pattern,
                "Cache circuit open, skipping delete_pattern"
            );
            return Ok(0);
        }

        let start = Instant::now();
        let result = self.backend.delete_pattern(pattern).await;
        let duration = start.elapsed();

        match &result {
            Ok(_) => cb.record_success_manual(duration),
            Err(_) => cb.record_failure_manual(duration),
        }

        result
    }

    /// Health check the cache backend
    ///
    /// If circuit is open, returns `Ok(false)` (unhealthy).
    pub async fn health_check(&self) -> CacheResult<bool> {
        if !self.should_use_circuit_breaker() {
            return self.backend.health_check().await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        if !cb.should_allow() {
            debug!("Cache circuit open, returning unhealthy");
            return Ok(false);
        }

        let start = Instant::now();
        let result = self.backend.health_check().await;
        let duration = start.elapsed();

        match &result {
            Ok(true) => cb.record_success_manual(duration),
            Ok(false) | Err(_) => cb.record_failure_manual(duration),
        }

        result
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
        assert!(provider.circuit_state().is_none());
    }

    #[tokio::test]
    async fn test_from_config_disabled() {
        let config = CacheConfig {
            enabled: false,
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config, None).await;
        assert!(!provider.is_enabled());
    }

    #[tokio::test]
    async fn test_from_config_unknown_backend() {
        let config = CacheConfig {
            enabled: true,
            backend: "unknown_backend".to_string(),
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config, None).await;
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
        let provider = CacheProvider::from_config_graceful(&config, None).await;
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
        let provider = CacheProvider::from_config_graceful(&config, None).await;
        assert!(provider.is_enabled());
        assert_eq!(provider.provider_name(), "moka");
        assert!(!provider.is_distributed()); // Moka is not distributed
        assert!(provider.circuit_state().is_none()); // No CB for non-distributed
    }

    #[cfg(feature = "cache-moka")]
    #[tokio::test]
    async fn test_from_config_memory_alias() {
        let config = CacheConfig {
            enabled: true,
            backend: "memory".to_string(),
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config, None).await;
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
        let provider = CacheProvider::from_config_graceful(&config, None).await;
        assert!(provider.is_enabled());
        assert_eq!(provider.provider_name(), "moka");
    }

    #[tokio::test]
    async fn test_noop_is_distributed() {
        let provider = CacheProvider::noop();
        assert!(provider.is_distributed()); // NoOp is "safe" (no state)
    }

    /// TAS-171: Test dragonfly alias resolves to Redis backend
    #[cfg(feature = "cache-redis")]
    #[tokio::test]
    async fn test_from_config_dragonfly_alias() {
        let config = CacheConfig {
            enabled: true,
            backend: "dragonfly".to_string(),
            redis: None, // No redis config = falls back to NoOp
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config, None).await;
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
        let provider = CacheProvider::from_config_graceful(&config, None).await;
        assert!(!provider.is_enabled());
    }
}
