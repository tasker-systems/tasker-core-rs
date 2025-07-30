//! Optimized Database Queries for Production Performance
//!
//! This module contains optimized versions of critical database queries
//! with performance enhancements including:
//! - Strategic indexing recommendations
//! - Query result caching
//! - Batch operations
//! - Connection pool optimization

use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::config::QueryCacheConfig;
use crate::database::sql_functions::{
    SqlFunctionExecutor, ActiveWorkerResult, WorkerHealthResult, 
    OptimalWorkerResult, WorkerPoolStatistics
};

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

/// Optimized worker selection queries with caching and performance enhancements
pub struct OptimizedWorkerQueries {
    sql_executor: SqlFunctionExecutor,
    active_workers_cache: QueryResultCache<Vec<ActiveWorkerResult>>,
    worker_health_cache: QueryResultCache<Vec<WorkerHealthResult>>,
}

// Legacy structs removed - now using optimized SQL function result types:
// - ActiveWorkerResult (from sql_functions.rs)
// - WorkerHealthResult (from sql_functions.rs) 
// - OptimalWorkerResult (from sql_functions.rs)
// - WorkerPoolStatistics (from sql_functions.rs)

impl OptimizedWorkerQueries {
    pub fn new(pool: PgPool) -> Self {
        let config = QueryCacheConfig::from_environment();
        
        // Log configuration for debugging
        info!("Initializing OptimizedWorkerQueries with environment-specific cache configuration");
        config.log_configuration();
        
        Self {
            sql_executor: SqlFunctionExecutor::new(pool),
            active_workers_cache: QueryResultCache::new(config.active_workers.ttl_seconds),
            worker_health_cache: QueryResultCache::new(config.worker_health.ttl_seconds),
        }
    }
    
    /// Create with explicit configuration (useful for testing)
    pub fn new_with_config(pool: PgPool, config: QueryCacheConfig) -> Self {
        info!("Initializing OptimizedWorkerQueries with explicit cache configuration");
        config.log_configuration();
        
        Self {
            sql_executor: SqlFunctionExecutor::new(pool),
            active_workers_cache: QueryResultCache::new(config.active_workers.ttl_seconds),
            worker_health_cache: QueryResultCache::new(config.worker_health.ttl_seconds),
        }
    }

    /// **OPTIMIZED**: Find active workers using SQL function with environment-aware caching
    /// 
    /// Performance improvements:
    /// - Environment-aware result caching (1s in test, 10s in dev, 30s in prod)
    /// - Pre-computed SQL function with optimized query plan
    /// - Strategic database indexes for optimal performance
    /// - Pre-computed health scoring at database level
    pub async fn find_active_workers_for_task_optimized(
        &self,
        named_task_id: i32,
    ) -> Result<Vec<ActiveWorkerResult>, sqlx::Error> {
        let cache_key = format!("active_workers_{}", named_task_id);

        // Check cache first
        if let Some(cached_result) = self.active_workers_cache.get(&cache_key).await {
            debug!("Returning cached active workers for task {}", named_task_id);
            return Ok(cached_result);
        }

        let start_time = Instant::now();

        // Use optimized SQL function with pre-computed query plan
        let active_workers = self.sql_executor
            .find_active_workers_for_task(named_task_id)
            .await?;

        let query_duration = start_time.elapsed();
        info!(
            "SQL function active workers query completed in {:?} for task {} (found {} workers)",
            query_duration,
            named_task_id,
            active_workers.len()
        );

        // Cache the result
        self.active_workers_cache
            .set(cache_key, active_workers.clone())
            .await;

        Ok(active_workers)
    }

    /// **OPTIMIZED**: Batch worker health check using SQL function
    /// 
    /// Returns comprehensive worker health information using optimized database function
    pub async fn get_worker_health_batch(
        &self,
        worker_ids: &[i32],
    ) -> Result<Vec<WorkerHealthResult>, sqlx::Error> {
        if worker_ids.is_empty() {
            return Ok(Vec::new());
        }

        let cache_key = format!("worker_health_{:?}", worker_ids);

        // Check cache first
        if let Some(cached_result) = self.worker_health_cache.get(&cache_key).await {
            debug!("Returning cached worker health for {} workers", worker_ids.len());
            return Ok(cached_result);
        }

        let start_time = Instant::now();

        // Use optimized SQL function
        let health_status = self.sql_executor
            .get_worker_health_batch(worker_ids)
            .await?;

        let query_duration = start_time.elapsed();
        info!(
            "SQL function worker health query completed in {:?} for {} workers",
            query_duration,
            worker_ids.len()
        );

        // Cache the result
        self.worker_health_cache
            .set(cache_key, health_status.clone())
            .await;

        Ok(health_status)
    }

    /// **OPTIMIZED**: Intelligent worker selection using SQL function
    /// 
    /// Uses pre-computed SQL function with comprehensive scoring algorithm
    pub async fn select_optimal_worker_for_task(
        &self,
        named_task_id: i32,
        required_capacity: i32,
    ) -> Result<Option<OptimalWorkerResult>, sqlx::Error> {
        let start_time = Instant::now();

        // Use optimized SQL function
        let optimal_worker = self.sql_executor
            .select_optimal_worker_for_task(named_task_id, Some(required_capacity))
            .await?;

        let query_duration = start_time.elapsed();

        if let Some(ref worker) = optimal_worker {
            info!(
                "SQL function optimal worker selection completed in {:?} for task {} (selected worker: {}, score: {:.2})",
                query_duration,
                named_task_id,
                worker.worker_name,
                worker.selection_score_f64()
            );
        } else {
            info!(
                "No optimal worker found in {:?} for task {} with capacity requirement {}",
                query_duration, named_task_id, required_capacity
            );
        }

        Ok(optimal_worker)
    }

    /// **OPTIMIZED**: Cache invalidation using SQL function coordination
    /// 
    /// Uses SQL function to coordinate cache invalidation with database-level triggers
    pub async fn invalidate_worker_caches(&self, named_task_id: Option<i32>) -> Result<bool, sqlx::Error> {
        // Use SQL function for coordinated cache invalidation
        let sql_result = self.sql_executor
            .invalidate_worker_cache(named_task_id)
            .await?;

        // Also clear local application-level caches
        if let Some(task_id) = named_task_id {
            let cache_key = format!("active_workers_{}", task_id);
            self.active_workers_cache.invalidate(&cache_key).await;
        } else {
            // Clear all caches if no specific task provided
            self.active_workers_cache.clear().await;
        }
        
        // Always clear health cache as it's more volatile
        self.worker_health_cache.clear().await;

        Ok(sql_result)
    }

    /// **NEW**: Get comprehensive worker pool statistics using SQL function
    /// 
    /// Provides detailed worker pool metrics for monitoring and health assessment
    pub async fn get_worker_pool_statistics(&self) -> Result<WorkerPoolStatistics, sqlx::Error> {
        self.sql_executor.get_worker_pool_statistics().await
    }

    /// Get database connection pool statistics for monitoring
    /// 
    /// Note: This accesses pool statistics through a helper method since
    /// SqlFunctionExecutor doesn't expose the pool directly
    pub async fn get_pool_statistics(&self) -> Result<PoolStatistics, sqlx::Error> {
        // For now, return basic statistics - could be enhanced with a pool accessor method
        Ok(PoolStatistics {
            size: 0,      // Would need pool accessor to get actual values
            num_idle: 0,  // Would need pool accessor to get actual values  
            is_closed: false,
        })
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
//   - `find_active_workers_for_task(p_named_task_id INTEGER)`
//   - `get_worker_health_batch(p_worker_ids INTEGER[])`
//   - `select_optimal_worker_for_task(p_named_task_id INTEGER, p_required_capacity INTEGER)`
//   - `get_worker_pool_statistics()`
//   - `invalidate_worker_cache(p_named_task_id INTEGER)`
// 
// Run migration: `cargo sqlx migrate run` to apply these optimizations.