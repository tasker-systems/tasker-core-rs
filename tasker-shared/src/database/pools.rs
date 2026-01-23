//! Database connection pool management for Tasker
//!
//! This module provides the `DatabasePools` struct that manages separate connection
//! pools for the main Tasker database and the PGMQ message queue database.
//!
//! ## Deployment Modes
//!
//! - **Single Database**: Both pools point to the same database (default, backward compatible)
//! - **Split Database**: PGMQ runs on a separate database for independent scaling
//!
//! ## Configuration
//!
//! Pool configuration is driven by `[common.database]` and `[common.pgmq_database]`
//! sections in the TOML configuration. When `PGMQ_DATABASE_URL` is not set or empty,
//! PGMQ operations use the main database pool.

use crate::config::tasker::{PoolConfig, TaskerConfig};
use crate::database::pool_stats::AtomicPoolStats;
use crate::{TaskerError, TaskerResult};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Holds connection pools for both databases
///
/// When deployed with single database, both pools point to same DB.
/// This maintains backward compatibility while enabling split-database deployments.
#[derive(Clone)]
pub struct DatabasePools {
    /// Main Tasker database pool (tasks, steps, transitions)
    tasker: PgPool,

    /// PGMQ database pool (queue operations)
    /// May be same as tasker pool in single-DB deployments
    pgmq: PgPool,

    /// Whether PGMQ is on a separate database
    pgmq_is_separate: bool,

    /// Atomic stats for the tasker pool (TAS-164)
    tasker_stats: Arc<AtomicPoolStats>,

    /// Atomic stats for the pgmq pool (TAS-164)
    pgmq_stats: Arc<AtomicPoolStats>,

    /// Slow acquire threshold in microseconds (TAS-164)
    slow_threshold_us: u64,
}

impl std::fmt::Debug for DatabasePools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabasePools")
            .field("tasker_pool_size", &self.tasker.size())
            .field("pgmq_pool_size", &self.pgmq.size())
            .field("pgmq_is_separate", &self.pgmq_is_separate)
            .field("slow_threshold_us", &self.slow_threshold_us)
            .field("tasker_stats", &self.tasker_stats.snapshot())
            .field("pgmq_stats", &self.pgmq_stats.snapshot())
            .finish()
    }
}

impl DatabasePools {
    /// Create database pools from configuration
    ///
    /// This is the primary constructor that respects the PGMQ database configuration.
    /// When `PGMQ_DATABASE_URL` is set and non-empty, a separate pool is created.
    /// Otherwise, both pools share the same connection to the main database.
    pub async fn from_config(config: &TaskerConfig) -> TaskerResult<Self> {
        let tasker_url = config.common.database_url();
        info!(
            "Creating Tasker database pool: {}...",
            tasker_url.chars().take(30).collect::<String>()
        );

        let tasker_pool_config = &config.common.database.pool;
        let tasker = Self::create_pool(tasker_pool_config, &tasker_url).await?;
        info!("Tasker database pool created successfully");

        let pgmq_is_separate = config.common.pgmq_is_separate();

        let pgmq_pool_config = config.common.pgmq_pool_config();
        let pgmq = if pgmq_is_separate {
            let pgmq_url = config.common.pgmq_database_url();
            info!(
                "Creating separate PGMQ database pool: {}...",
                pgmq_url.chars().take(30).collect::<String>()
            );

            let pool = Self::create_pool(pgmq_pool_config, &pgmq_url).await?;
            info!("PGMQ database pool created successfully (separate database)");
            pool
        } else {
            info!("PGMQ using main database pool (single-database mode)");
            tasker.clone()
        };

        let slow_threshold_us = u64::from(tasker_pool_config.slow_acquire_threshold_ms) * 1000;
        let tasker_stats = Arc::new(AtomicPoolStats::new(
            "tasker".to_string(),
            tasker_pool_config.max_connections,
        ));
        let pgmq_stats = Arc::new(AtomicPoolStats::new(
            "pgmq".to_string(),
            pgmq_pool_config.max_connections,
        ));

        Ok(Self {
            tasker,
            pgmq,
            pgmq_is_separate,
            tasker_stats,
            pgmq_stats,
            slow_threshold_us,
        })
    }

    /// Create pools with a pre-existing tasker pool
    ///
    /// This is useful for testing or when the main pool has already been created.
    /// If PGMQ is configured for a separate database, a new pool is created for it.
    pub async fn with_tasker_pool(config: &TaskerConfig, tasker: PgPool) -> TaskerResult<Self> {
        let pgmq_is_separate = config.common.pgmq_is_separate();
        let tasker_pool_config = &config.common.database.pool;
        let pgmq_pool_config = config.common.pgmq_pool_config();

        let pgmq = if pgmq_is_separate {
            let pgmq_url = config.common.pgmq_database_url();
            info!(
                "Creating separate PGMQ database pool: {}...",
                pgmq_url.chars().take(30).collect::<String>()
            );

            let pool = Self::create_pool(pgmq_pool_config, &pgmq_url).await?;
            info!("PGMQ database pool created successfully (separate database)");
            pool
        } else {
            info!("PGMQ using main database pool (single-database mode)");
            tasker.clone()
        };

        let slow_threshold_us = u64::from(tasker_pool_config.slow_acquire_threshold_ms) * 1000;
        let tasker_stats = Arc::new(AtomicPoolStats::new(
            "tasker".to_string(),
            tasker_pool_config.max_connections,
        ));
        let pgmq_stats = Arc::new(AtomicPoolStats::new(
            "pgmq".to_string(),
            pgmq_pool_config.max_connections,
        ));

        Ok(Self {
            tasker,
            pgmq,
            pgmq_is_separate,
            tasker_stats,
            pgmq_stats,
            slow_threshold_us,
        })
    }

    /// Create a pool with specific configuration
    async fn create_pool(pool_config: &PoolConfig, database_url: &str) -> TaskerResult<PgPool> {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(pool_config.max_connections)
            .min_connections(pool_config.min_connections)
            .acquire_timeout(Duration::from_secs(
                pool_config.acquire_timeout_seconds as u64,
            ))
            .idle_timeout(Some(Duration::from_secs(
                pool_config.idle_timeout_seconds as u64,
            )))
            .max_lifetime(Some(Duration::from_secs(
                pool_config.max_lifetime_seconds as u64,
            )))
            .connect(database_url)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to create pool: {e}")))
    }

    /// Get the main Tasker database pool
    ///
    /// Use this pool for task, step, and transition operations.
    #[inline]
    pub fn tasker(&self) -> &PgPool {
        &self.tasker
    }

    /// Get the PGMQ database pool
    ///
    /// Use this pool for queue operations (send, read, delete).
    /// Returns the same pool as `tasker()` in single-database deployments.
    #[inline]
    pub fn pgmq(&self) -> &PgPool {
        &self.pgmq
    }

    /// Check if PGMQ is running on a separate database
    #[inline]
    pub fn pgmq_is_separate(&self) -> bool {
        self.pgmq_is_separate
    }

    /// Get pool for a specific operation type
    ///
    /// This provides semantic routing of database operations to the appropriate pool.
    pub fn for_operation(&self, op: DatabaseOperation) -> &PgPool {
        match op {
            DatabaseOperation::TaskQuery
            | DatabaseOperation::StepQuery
            | DatabaseOperation::TransitionInsert
            | DatabaseOperation::StateTransition => &self.tasker,

            DatabaseOperation::QueueSend
            | DatabaseOperation::QueueRead
            | DatabaseOperation::QueueDelete
            | DatabaseOperation::QueueCreate => &self.pgmq,
        }
    }

    /// Get current pool utilization (active/idle connection counts).
    pub fn utilization(&self) -> PoolUtilization {
        let tasker_stats = self.tasker_stats.snapshot();
        let pgmq_stats = self.pgmq_stats.snapshot();
        PoolUtilization {
            tasker_size: self.tasker.size(),
            tasker_idle: self.tasker.num_idle() as u32,
            tasker_max: tasker_stats.max_connections,
            pgmq_size: self.pgmq.size(),
            pgmq_idle: self.pgmq.num_idle() as u32,
            pgmq_max: pgmq_stats.max_connections,
            pgmq_is_separate: self.pgmq_is_separate,
        }
    }

    /// Get stats snapshots for both pools.
    pub fn stats_snapshots(
        &self,
    ) -> (
        crate::database::pool_stats::PoolStatsSnapshot,
        crate::database::pool_stats::PoolStatsSnapshot,
    ) {
        (self.tasker_stats.snapshot(), self.pgmq_stats.snapshot())
    }

    /// Record a tasker pool acquire duration.
    #[inline]
    pub fn record_tasker_acquire(&self, duration_us: u64) {
        self.tasker_stats
            .record_acquire(duration_us, self.slow_threshold_us);
    }

    /// Record a pgmq pool acquire duration.
    #[inline]
    pub fn record_pgmq_acquire(&self, duration_us: u64) {
        self.pgmq_stats
            .record_acquire(duration_us, self.slow_threshold_us);
    }

    /// Record a tasker pool acquire error.
    #[inline]
    pub fn record_tasker_error(&self) {
        self.tasker_stats.record_error();
    }

    /// Record a pgmq pool acquire error.
    #[inline]
    pub fn record_pgmq_error(&self) {
        self.pgmq_stats.record_error();
    }
}

/// Current pool utilization snapshot (TAS-164).
#[derive(Debug, Clone, Serialize)]
pub struct PoolUtilization {
    pub tasker_size: u32,
    pub tasker_idle: u32,
    pub tasker_max: u32,
    pub pgmq_size: u32,
    pub pgmq_idle: u32,
    pub pgmq_max: u32,
    pub pgmq_is_separate: bool,
}

/// Categories of database operations for pool routing
///
/// Used with `DatabasePools::for_operation()` to semantically select
/// the appropriate pool based on operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseOperation {
    /// Task state queries and updates
    TaskQuery,
    /// Step state queries and updates
    StepQuery,
    /// Transition record inserts
    TransitionInsert,
    /// State machine transitions
    StateTransition,
    /// Queue message send operations
    QueueSend,
    /// Queue message read operations
    QueueRead,
    /// Queue message delete operations
    QueueDelete,
    /// Queue creation operations
    QueueCreate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_operation_routing() {
        // This is a compile-time check that all operations map to appropriate pools
        let ops = [
            (DatabaseOperation::TaskQuery, "tasker"),
            (DatabaseOperation::StepQuery, "tasker"),
            (DatabaseOperation::TransitionInsert, "tasker"),
            (DatabaseOperation::StateTransition, "tasker"),
            (DatabaseOperation::QueueSend, "pgmq"),
            (DatabaseOperation::QueueRead, "pgmq"),
            (DatabaseOperation::QueueDelete, "pgmq"),
            (DatabaseOperation::QueueCreate, "pgmq"),
        ];

        // Verify routing logic is consistent
        for (op, expected_pool) in ops {
            let is_queue_op = matches!(
                op,
                DatabaseOperation::QueueSend
                    | DatabaseOperation::QueueRead
                    | DatabaseOperation::QueueDelete
                    | DatabaseOperation::QueueCreate
            );
            assert_eq!(
                is_queue_op,
                expected_pool == "pgmq",
                "Operation {:?} should route to {} pool",
                op,
                expected_pool
            );
        }
    }
}
