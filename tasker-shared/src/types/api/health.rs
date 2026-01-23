//! # Shared Health API Types (TAS-164)
//!
//! Pool utilization types shared by both orchestration and worker health endpoints.
//! Each instance reports its own connection pool state independently.

use serde::{Deserialize, Serialize};

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

/// Connection pool utilization information (TAS-164)
///
/// Shared between orchestration and worker health responses.
/// Each service instance reports its own pool utilization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct PoolUtilizationInfo {
    pub tasker_pool: PoolDetail,
    pub pgmq_pool: PoolDetail,
}

/// Detailed metrics for a single connection pool (TAS-164)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct PoolDetail {
    pub active_connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
    pub utilization_percent: f64,
    pub total_acquires: u64,
    pub slow_acquires: u64,
    pub acquire_errors: u64,
    pub average_acquire_time_ms: f64,
    pub max_acquire_time_ms: f64,
}

/// Build a `PoolUtilizationInfo` from a `DatabasePools` reference.
///
/// Shared helper used by both orchestration and worker health endpoints.
pub fn build_pool_utilization(pools: &crate::database::DatabasePools) -> PoolUtilizationInfo {
    let utilization = pools.utilization();
    let (tasker_snap, pgmq_snap) = pools.stats_snapshots();

    let tasker_active = utilization
        .tasker_size
        .saturating_sub(utilization.tasker_idle);
    let pgmq_active = utilization.pgmq_size.saturating_sub(utilization.pgmq_idle);

    PoolUtilizationInfo {
        tasker_pool: PoolDetail {
            active_connections: tasker_active,
            idle_connections: utilization.tasker_idle,
            max_connections: utilization.tasker_max,
            utilization_percent: if utilization.tasker_max > 0 {
                f64::from(tasker_active) / f64::from(utilization.tasker_max) * 100.0
            } else {
                0.0
            },
            total_acquires: tasker_snap.total_acquires,
            slow_acquires: tasker_snap.slow_acquires,
            acquire_errors: tasker_snap.acquire_errors,
            average_acquire_time_ms: tasker_snap.average_acquire_time_us / 1000.0,
            max_acquire_time_ms: tasker_snap.max_acquire_time_us as f64 / 1000.0,
        },
        pgmq_pool: PoolDetail {
            active_connections: pgmq_active,
            idle_connections: utilization.pgmq_idle,
            max_connections: utilization.pgmq_max,
            utilization_percent: if utilization.pgmq_max > 0 {
                f64::from(pgmq_active) / f64::from(utilization.pgmq_max) * 100.0
            } else {
                0.0
            },
            total_acquires: pgmq_snap.total_acquires,
            slow_acquires: pgmq_snap.slow_acquires,
            acquire_errors: pgmq_snap.acquire_errors,
            average_acquire_time_ms: pgmq_snap.average_acquire_time_us / 1000.0,
            max_acquire_time_ms: pgmq_snap.max_acquire_time_us as f64 / 1000.0,
        },
    }
}
