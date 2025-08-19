//! Optimized Database Queries for Production Performance
//!
//! This module contains optimized versions of critical database queries
//! with performance enhancements including:
//! - Strategic indexing recommendations
//! - Query result caching
//! - Batch operations
//! - Connection pool optimization

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Query result cache for frequently accessed data
#[derive(Debug, Clone)]
pub struct QueryResultCache<T> {
    data: Arc<RwLock<HashMap<String, (T, Instant)>>>,
    ttl: Duration,
}

impl<T: Clone> QueryResultCache<T> {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub async fn get(&self, key: &str) -> Option<T> {
        let cache = self.data.read().await;
        if let Some((value, cached_at)) = cache.get(key) {
            if cached_at.elapsed() < self.ttl {
                debug!("Cache hit for key: {}", key);
                return Some(value.clone());
            }
        }
        debug!("Cache miss for key: {}", key);
        None
    }

    pub async fn set(&self, key: String, value: T) {
        let mut cache = self.data.write().await;
        debug!("Cached result for key: {}", key);
        cache.insert(key, (value, Instant::now()));
    }

    pub async fn invalidate(&self, key: &str) {
        let mut cache = self.data.write().await;
        cache.remove(key);
        debug!("Invalidated cache for key: {}", key);
    }

    pub async fn clear(&self) {
        let mut cache = self.data.write().await;
        cache.clear();
        info!("Cleared entire query cache");
    }
}

#[derive(Debug, Clone)]
pub struct PoolStatistics {
    pub size: u32,
    pub num_idle: u32,
    pub is_closed: bool,
}

// Database indexes and SQL functions for optimal performance
//
// **MIGRATION IMPLEMENTED**: All recommended indexes and optimized SQL functions
// have been implemented in migration: `20250729000001_create_optimized_worker_query_functions.sql`
//
// The migration includes:
// - Strategic database indexes for optimal query performance
// - Pre-computed SQL functions with optimized query plans:
//   - `find_active_workers_for_task(p_named_task_uuid INTEGER)`
//   - `get_worker_health_batch(p_worker_ids INTEGER[])`
//   - `select_optimal_worker_for_task(p_named_task_uuid INTEGER, p_required_capacity INTEGER)`
//   - `get_worker_pool_statistics()`
//   - `invalidate_worker_cache(p_named_task_uuid INTEGER)`
//
// Run migration: `cargo sqlx migrate run` to apply these optimizations.
