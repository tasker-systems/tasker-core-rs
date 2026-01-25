//! Circuit breaker protected cache provider (TAS-171)
//!
//! Wraps `CacheProvider` with circuit breaker protection to prevent repeated
//! timeout penalties when Redis/Dragonfly is unavailable. When the circuit is open,
//! cache operations fail fast with graceful fallback behavior.
//!
//! ## Design
//!
//! - Circuit breaker only applies to distributed (Redis/Dragonfly) operations
//! - In-memory (Moka) and NoOp providers bypass circuit breaker logic
//! - Open circuit behavior:
//!   - `get()` returns `Ok(None)` - cache miss
//!   - `set()` / `delete()` return `Ok(())` - no-op
//!   - `health_check()` returns `Ok(false)` - unhealthy
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_shared::cache::{CacheProvider, CircuitBreakerCache};
//! use tasker_shared::resilience::CircuitBreaker;
//! use std::sync::Arc;
//!
//! let provider = CacheProvider::from_config_graceful(&cache_config).await;
//! let circuit_breaker = Arc::new(CircuitBreaker::new("cache".to_string(), cb_config));
//! let protected_cache = CircuitBreakerCache::new(provider, Some(circuit_breaker));
//!
//! // Operations are protected by circuit breaker
//! let value = protected_cache.get("key").await?;
//! ```

use super::errors::CacheResult;
use super::provider::CacheProvider;
use crate::resilience::{CircuitBreaker, CircuitState};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::debug;

/// Circuit breaker protected cache provider
///
/// Wraps `CacheProvider` and protects Redis/Dragonfly operations with a circuit breaker.
/// When the circuit is open:
/// - `get()` returns `Ok(None)` - cache miss
/// - `set()` returns `Ok(())` - no-op
/// - `delete()` returns `Ok(())` - no-op
#[derive(Debug, Clone)]
pub struct CircuitBreakerCache {
    inner: CacheProvider,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl CircuitBreakerCache {
    /// Create a new circuit breaker protected cache
    ///
    /// The circuit breaker is only used for distributed (Redis) providers.
    /// For in-memory (Moka) or NoOp providers, pass `None` for the circuit breaker.
    pub fn new(provider: CacheProvider, circuit_breaker: Option<Arc<CircuitBreaker>>) -> Self {
        Self {
            inner: provider,
            circuit_breaker,
        }
    }

    /// Check if circuit breaker protection should be used
    ///
    /// Only applies to distributed cache providers (Redis/Dragonfly) because
    /// they involve network calls that can timeout or fail.
    /// In-memory (Moka) and NoOp providers don't need protection.
    fn should_use_circuit_breaker(&self) -> bool {
        if self.circuit_breaker.is_none() {
            return false;
        }

        // Distributed providers involve network calls and need circuit breaker protection
        // This check delegates to the provider's own characteristic declaration
        self.inner.is_distributed() && self.inner.is_enabled()
    }

    /// Get current circuit state
    ///
    /// Returns `None` if no circuit breaker is configured.
    pub fn circuit_state(&self) -> Option<CircuitState> {
        self.circuit_breaker.as_ref().map(|cb| cb.state())
    }

    /// Check if the cache is enabled (not NoOp)
    pub fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    /// Check if the cache is distributed (safe for multi-instance deployments)
    pub fn is_distributed(&self) -> bool {
        self.inner.is_distributed()
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        self.inner.provider_name()
    }

    /// Get a value from cache
    ///
    /// If circuit is open, returns `Ok(None)` (cache miss behavior).
    pub async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        // Skip circuit breaker for non-distributed providers
        if !self.should_use_circuit_breaker() {
            return self.inner.get(key).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        // Check if circuit allows calls
        if !cb.should_allow() {
            debug!(key = key, "Cache circuit open, returning miss");
            return Ok(None);
        }

        let start = Instant::now();
        let result = self.inner.get(key).await;
        let duration = start.elapsed();

        // Record success/failure
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
        // Skip circuit breaker for non-distributed providers
        if !self.should_use_circuit_breaker() {
            return self.inner.set(key, value, ttl).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        // Check if circuit allows calls
        if !cb.should_allow() {
            debug!(key = key, "Cache circuit open, skipping set");
            return Ok(());
        }

        let start = Instant::now();
        let result = self.inner.set(key, value, ttl).await;
        let duration = start.elapsed();

        // Record success/failure
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
        // Skip circuit breaker for non-distributed providers
        if !self.should_use_circuit_breaker() {
            return self.inner.delete(key).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        // Check if circuit allows calls
        if !cb.should_allow() {
            debug!(key = key, "Cache circuit open, skipping delete");
            return Ok(());
        }

        let start = Instant::now();
        let result = self.inner.delete(key).await;
        let duration = start.elapsed();

        // Record success/failure
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
        // Skip circuit breaker for non-distributed providers
        if !self.should_use_circuit_breaker() {
            return self.inner.delete_pattern(pattern).await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        // Check if circuit allows calls
        if !cb.should_allow() {
            debug!(
                pattern = pattern,
                "Cache circuit open, skipping delete_pattern"
            );
            return Ok(0);
        }

        let start = Instant::now();
        let result = self.inner.delete_pattern(pattern).await;
        let duration = start.elapsed();

        // Record success/failure
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
        // Skip circuit breaker for non-distributed providers
        if !self.should_use_circuit_breaker() {
            return self.inner.health_check().await;
        }

        let cb = self
            .circuit_breaker
            .as_ref()
            .expect("checked in should_use");

        // Check if circuit allows calls
        if !cb.should_allow() {
            debug!("Cache circuit open, returning unhealthy");
            return Ok(false);
        }

        let start = Instant::now();
        let result = self.inner.health_check().await;
        let duration = start.elapsed();

        // Record success/failure
        match &result {
            Ok(true) => cb.record_success_manual(duration),
            Ok(false) | Err(_) => cb.record_failure_manual(duration),
        }

        result
    }

    /// Get the inner cache provider (for advanced use cases)
    pub fn inner(&self) -> &CacheProvider {
        &self.inner
    }

    /// Get the circuit breaker (for metrics/monitoring)
    pub fn circuit_breaker(&self) -> Option<&Arc<CircuitBreaker>> {
        self.circuit_breaker.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_cache_without_cb() {
        let provider = CacheProvider::noop();
        let cache = CircuitBreakerCache::new(provider, None);

        assert!(!cache.is_enabled());
        assert!(cache.circuit_state().is_none());
        assert!(!cache.should_use_circuit_breaker());

        // Operations should pass through
        let result = cache.get("test").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_circuit_breaker_cache_with_noop_provider() {
        use crate::resilience::config::CircuitBreakerConfig;

        let provider = CacheProvider::noop();
        let cb_config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        };
        let cb = Arc::new(CircuitBreaker::new("cache".to_string(), cb_config));
        let cache = CircuitBreakerCache::new(provider, Some(cb));

        // NoOp provider should not use circuit breaker
        // (is_distributed=true but is_enabled=false, so no network calls)
        assert!(!cache.should_use_circuit_breaker());

        // Circuit state should still be available
        assert!(cache.circuit_state().is_some());
        assert_eq!(cache.circuit_state().unwrap(), CircuitState::Closed);
    }

    #[cfg(feature = "cache-moka")]
    #[tokio::test]
    async fn test_circuit_breaker_cache_with_moka() {
        use crate::config::tasker::CacheConfig;
        use crate::resilience::config::CircuitBreakerConfig;

        let config = CacheConfig {
            enabled: true,
            backend: "moka".to_string(),
            ..CacheConfig::default()
        };
        let provider = CacheProvider::from_config_graceful(&config).await;

        let cb_config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        };
        let cb = Arc::new(CircuitBreaker::new("cache".to_string(), cb_config));
        let cache = CircuitBreakerCache::new(provider, Some(cb));

        // Moka provider should not use circuit breaker
        // (is_distributed=false, so no network calls to protect)
        assert!(!cache.should_use_circuit_breaker());
        assert!(cache.is_enabled());
    }
}
