//! Redis cache provider (TAS-156)
//!
//! Uses `redis::aio::ConnectionManager` for async multiplexed connections.
//! Requires the `cache-redis` feature flag.

use crate::cache::errors::{CacheError, CacheResult};
use crate::cache::traits::CacheService;
use crate::config::tasker::RedisConfig;
use std::time::Duration;
use tracing::debug;

/// Redis-backed cache service using ConnectionManager
///
/// Provides async multiplexed connections with automatic reconnection.
/// Uses SCAN for pattern deletion to avoid blocking the server.
#[derive(Clone)]
pub struct RedisCacheService {
    connection_manager: redis::aio::ConnectionManager,
}

impl std::fmt::Debug for RedisCacheService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisCacheService")
            .field("connection_manager", &"ConnectionManager")
            .finish()
    }
}

impl RedisCacheService {
    /// Create a new Redis cache service from configuration
    pub async fn from_config(config: &RedisConfig) -> CacheResult<Self> {
        let client = redis::Client::open(config.url.as_str()).map_err(|e| {
            CacheError::ConnectionError(format!("Failed to create Redis client: {}", e))
        })?;

        let connection_manager = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| {
                CacheError::ConnectionError(format!("Failed to connect to Redis: {}", e))
            })?;

        debug!(url = %redact_url(&config.url), "Redis cache service connected");

        Ok(Self { connection_manager })
    }
}

impl CacheService for RedisCacheService {
    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        let mut conn = self.connection_manager.clone();
        let result: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::BackendError(format!("Redis GET failed: {}", e)))?;

        if result.is_some() {
            debug!(key = key, "Cache HIT");
        } else {
            debug!(key = key, "Cache MISS");
        }

        Ok(result)
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> CacheResult<()> {
        let mut conn = self.connection_manager.clone();
        let ttl_seconds = ttl.as_secs().max(1);

        redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_seconds)
            .arg(value)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| CacheError::BackendError(format!("Redis SETEX failed: {}", e)))?;

        debug!(key = key, ttl_seconds = ttl_seconds, "Cache SET");
        Ok(())
    }

    async fn delete(&self, key: &str) -> CacheResult<()> {
        let mut conn = self.connection_manager.clone();

        redis::cmd("DEL")
            .arg(key)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| CacheError::BackendError(format!("Redis DEL failed: {}", e)))?;

        debug!(key = key, "Cache DEL");
        Ok(())
    }

    async fn delete_pattern(&self, pattern: &str) -> CacheResult<u64> {
        let mut conn = self.connection_manager.clone();
        let mut deleted: u64 = 0;
        let mut cursor: u64 = 0;

        // Use SCAN to iterate without blocking the server
        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .map_err(|e| CacheError::BackendError(format!("Redis SCAN failed: {}", e)))?;

            if !keys.is_empty() {
                let count: u64 = redis::cmd("DEL")
                    .arg(&keys)
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| {
                        CacheError::BackendError(format!("Redis DEL (batch) failed: {}", e))
                    })?;
                deleted += count;
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        debug!(pattern = pattern, deleted = deleted, "Cache pattern DEL");
        Ok(deleted)
    }

    async fn health_check(&self) -> CacheResult<bool> {
        let mut conn = self.connection_manager.clone();
        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::BackendError(format!("Redis PING failed: {}", e)))?;

        Ok(pong == "PONG")
    }

    fn provider_name(&self) -> &'static str {
        "redis"
    }

    fn is_distributed(&self) -> bool {
        // Redis/Dragonfly is distributed - state is shared across all instances
        // This means cache invalidations propagate correctly, but also that
        // network calls are involved (circuit breaker protection recommended)
        true
    }
}

/// Redact credentials from a Redis URL for logging
fn redact_url(url: &str) -> String {
    // Redact password if present: redis://user:pass@host -> redis://user:***@host
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
    fn test_redact_url_with_password() {
        assert_eq!(
            redact_url("redis://user:secret@localhost:6379"),
            "redis://user:***@localhost:6379"
        );
    }

    #[test]
    fn test_redact_url_without_password() {
        assert_eq!(
            redact_url("redis://localhost:6379"),
            "redis://localhost:6379"
        );
    }

    #[test]
    fn test_redact_url_with_db() {
        assert_eq!(
            redact_url("redis://user:pass@localhost:6379/0"),
            "redis://user:***@localhost:6379/0"
        );
    }

    // Note: is_distributed() test requires a running Redis instance
    // See integration tests below for full coverage

    // Integration tests require a running Redis instance (behind test-services feature)
    #[cfg(feature = "test-services")]
    mod integration {
        use super::*;
        use tracing::warn;

        fn test_redis_config() -> RedisConfig {
            RedisConfig {
                url: std::env::var("REDIS_URL")
                    .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
                max_connections: 5,
                connection_timeout_seconds: 5,
                database: 0,
            }
        }

        #[tokio::test]
        async fn test_redis_crud_operations() {
            let config = test_redis_config();
            let svc = match RedisCacheService::from_config(&config).await {
                Ok(svc) => svc,
                Err(e) => {
                    warn!("Skipping Redis test (not available): {}", e);
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
        async fn test_redis_ttl_expiry() {
            let config = test_redis_config();
            let svc = match RedisCacheService::from_config(&config).await {
                Ok(svc) => svc,
                Err(e) => {
                    warn!("Skipping Redis test (not available): {}", e);
                    return;
                }
            };

            let key = format!("test:ttl:{}", uuid::Uuid::new_v4());

            // Set with 1 second TTL
            svc.set(&key, "temporary", Duration::from_secs(1))
                .await
                .unwrap();

            // Should exist immediately
            assert!(svc.get(&key).await.unwrap().is_some());

            // Wait for expiry
            tokio::time::sleep(Duration::from_millis(1500)).await;

            // Should be gone
            assert!(svc.get(&key).await.unwrap().is_none());
        }

        #[tokio::test]
        async fn test_redis_pattern_delete() {
            let config = test_redis_config();
            let svc = match RedisCacheService::from_config(&config).await {
                Ok(svc) => svc,
                Err(e) => {
                    warn!("Skipping Redis test (not available): {}", e);
                    return;
                }
            };

            let prefix = format!("test:pattern:{}", uuid::Uuid::new_v4());

            // Set multiple keys with same prefix
            for i in 0..5 {
                let key = format!("{}:key{}", prefix, i);
                svc.set(&key, "value", Duration::from_secs(60))
                    .await
                    .unwrap();
            }

            // Delete by pattern
            let deleted = svc.delete_pattern(&format!("{}:*", prefix)).await.unwrap();
            assert_eq!(deleted, 5);

            // Verify all gone
            for i in 0..5 {
                let key = format!("{}:key{}", prefix, i);
                assert!(svc.get(&key).await.unwrap().is_none());
            }
        }

        #[tokio::test]
        async fn test_redis_health_check() {
            let config = test_redis_config();
            let svc = match RedisCacheService::from_config(&config).await {
                Ok(svc) => svc,
                Err(e) => {
                    warn!("Skipping Redis test (not available): {}", e);
                    return;
                }
            };

            assert!(svc.health_check().await.unwrap());
        }
    }
}
