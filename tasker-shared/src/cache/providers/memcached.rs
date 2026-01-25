//! Memcached cache provider (TAS-171)
//!
//! Provides distributed caching using memcached protocol.
//! Requires the `cache-memcached` feature flag.

use crate::cache::errors::{CacheError, CacheResult};
use crate::cache::traits::CacheService;
use crate::config::tasker::MemcachedConfig;
use async_memcached::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::debug;

/// Memcached-backed cache service
///
/// Provides distributed caching with memcached protocol.
/// Uses the async-memcached client for async operations.
///
/// Note: Memcached doesn't support pattern-based deletion like Redis SCAN,
/// so `delete_pattern` relies on TTL expiry (similar to Moka).
pub struct MemcachedCacheService {
    client: Arc<Mutex<Client>>,
}

impl std::fmt::Debug for MemcachedCacheService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemcachedCacheService")
            .field("client", &"Client")
            .finish()
    }
}

impl Clone for MemcachedCacheService {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
        }
    }
}

impl MemcachedCacheService {
    /// Create a new Memcached cache service from configuration
    pub async fn from_config(config: &MemcachedConfig) -> CacheResult<Self> {
        let client = Client::new(&config.url).await.map_err(|e| {
            CacheError::ConnectionError(format!("Failed to connect to memcached: {}", e))
        })?;

        debug!(url = %redact_url(&config.url), "Memcached cache service connected");

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }
}

impl CacheService for MemcachedCacheService {
    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        let mut client = self.client.lock().await;

        let result = client
            .get(key)
            .await
            .map_err(|e| CacheError::BackendError(format!("Memcached GET failed: {}", e)))?;

        match result {
            Some(value) => {
                // async-memcached returns Value, we need to convert to String
                let data = String::from_utf8(value.data)
                    .map_err(|e| CacheError::BackendError(format!("Invalid UTF-8 data: {}", e)))?;
                debug!(key = key, "Cache HIT (memcached)");
                Ok(Some(data))
            }
            None => {
                debug!(key = key, "Cache MISS (memcached)");
                Ok(None)
            }
        }
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> CacheResult<()> {
        let mut client = self.client.lock().await;
        let ttl_seconds = ttl.as_secs() as i64;

        client
            .set(key, value.as_bytes(), Some(ttl_seconds), None)
            .await
            .map_err(|e| CacheError::BackendError(format!("Memcached SET failed: {}", e)))?;

        debug!(
            key = key,
            ttl_seconds = ttl_seconds,
            "Cache SET (memcached)"
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> CacheResult<()> {
        let mut client = self.client.lock().await;

        // Memcached delete returns error if key doesn't exist, which is fine
        let _ = client.delete(key).await;

        debug!(key = key, "Cache DEL (memcached)");
        Ok(())
    }

    async fn delete_pattern(&self, pattern: &str) -> CacheResult<u64> {
        // Memcached doesn't support pattern-based key iteration like Redis SCAN
        // For our use case (analytics/templates), we rely on TTL expiry
        debug!(
            pattern = pattern,
            "Pattern deletion not supported for Memcached, relying on TTL expiry"
        );
        Ok(0)
    }

    async fn health_check(&self) -> CacheResult<bool> {
        let mut client = self.client.lock().await;

        // Use version command as health check
        match client.version().await {
            Ok(_) => Ok(true),
            Err(e) => {
                debug!(error = %e, "Memcached health check failed");
                Ok(false)
            }
        }
    }

    fn provider_name(&self) -> &'static str {
        "memcached"
    }

    fn is_distributed(&self) -> bool {
        // Memcached is distributed - state is shared across all instances
        // This means cache invalidations propagate correctly, but also that
        // network calls are involved (circuit breaker protection recommended)
        true
    }
}

/// Redact credentials from a memcached URL for logging
fn redact_url(url: &str) -> String {
    // Memcached URLs typically don't have credentials, but redact just in case
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let prefix = &url[..=colon_pos];
            let suffix = &url[at_pos..];
            return format!("{}***{}", prefix, suffix);
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_url_no_credentials() {
        assert_eq!(redact_url("tcp://localhost:11211"), "tcp://localhost:11211");
    }

    #[test]
    fn test_redact_url_with_credentials() {
        assert_eq!(
            redact_url("tcp://user:pass@localhost:11211"),
            "tcp://user:***@localhost:11211"
        );
    }

    // Integration tests require a running Memcached instance (behind test-services feature)
    #[cfg(feature = "test-services")]
    mod integration {
        use super::*;
        use tracing::warn;

        fn test_memcached_config() -> MemcachedConfig {
            MemcachedConfig {
                url: std::env::var("MEMCACHED_URL")
                    .unwrap_or_else(|_| "tcp://localhost:11211".to_string()),
                connection_timeout_seconds: 5,
            }
        }

        #[tokio::test]
        async fn test_memcached_crud_operations() {
            let config = test_memcached_config();
            let svc = match MemcachedCacheService::from_config(&config).await {
                Ok(svc) => svc,
                Err(e) => {
                    warn!("Skipping Memcached test (not available): {}", e);
                    return;
                }
            };

            let key = format!("test:crud:{}", uuid::Uuid::new_v4());
            let value = r#"{"name":"test","version":"1.0"}"#;

            // Set
            svc.set(&key, value, Duration::from_secs(60)).await.unwrap();

            // Get (hit)
            let result = svc.get(&key).await.unwrap();
            assert_eq!(result, Some(value.to_string()));

            // Delete
            svc.delete(&key).await.unwrap();

            // Get (miss)
            let result = svc.get(&key).await.unwrap();
            assert_eq!(result, None);
        }

        #[tokio::test]
        async fn test_memcached_health_check() {
            let config = test_memcached_config();
            let svc = match MemcachedCacheService::from_config(&config).await {
                Ok(svc) => svc,
                Err(e) => {
                    warn!("Skipping Memcached test (not available): {}", e);
                    return;
                }
            };

            assert!(svc.health_check().await.unwrap());
        }

        #[tokio::test]
        async fn test_memcached_is_distributed() {
            let config = test_memcached_config();
            let svc = match MemcachedCacheService::from_config(&config).await {
                Ok(svc) => svc,
                Err(e) => {
                    warn!("Skipping Memcached test (not available): {}", e);
                    return;
                }
            };

            assert!(svc.is_distributed());
        }
    }
}
