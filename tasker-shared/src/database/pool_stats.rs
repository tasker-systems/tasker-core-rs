//! # Connection Pool Statistics (TAS-164)
//!
//! SWMR (Single Writer, Multiple Reader) atomic statistics for connection pool
//! observability. Uses `AtomicU64` with relaxed ordering for hot-path recording
//! and provides snapshot DTOs for readers.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic connection pool statistics using SWMR pattern.
///
/// Writers call `record_acquire` and `record_error` on hot paths using
/// `Relaxed` ordering for minimal overhead. Readers call `snapshot()` to
/// get a consistent-enough view of the stats.
#[derive(Debug)]
pub struct AtomicPoolStats {
    /// Pool identifier
    pool_name: String,

    /// Maximum connections configured for this pool
    max_connections: u32,

    /// Total successful acquires
    total_acquires: AtomicU64,

    /// Total acquire time in microseconds (for average computation)
    total_acquire_time_us: AtomicU64,

    /// Number of acquires exceeding slow threshold
    slow_acquires: AtomicU64,

    /// Maximum acquire time observed (in microseconds)
    max_acquire_time_us: AtomicU64,

    /// Total acquire errors (timeouts, pool exhaustion)
    acquire_errors: AtomicU64,
}

impl AtomicPoolStats {
    /// Create a new stats tracker for a named pool.
    pub fn new(pool_name: String, max_connections: u32) -> Self {
        Self {
            pool_name,
            max_connections,
            total_acquires: AtomicU64::new(0),
            total_acquire_time_us: AtomicU64::new(0),
            slow_acquires: AtomicU64::new(0),
            max_acquire_time_us: AtomicU64::new(0),
            acquire_errors: AtomicU64::new(0),
        }
    }

    /// Record a successful connection acquire.
    ///
    /// Hot-path writer using `Relaxed` ordering for minimal overhead.
    #[inline]
    pub fn record_acquire(&self, duration_us: u64, slow_threshold_us: u64) {
        self.total_acquires.fetch_add(1, Ordering::Relaxed);
        self.total_acquire_time_us
            .fetch_add(duration_us, Ordering::Relaxed);
        self.max_acquire_time_us
            .fetch_max(duration_us, Ordering::Relaxed);

        if duration_us >= slow_threshold_us {
            self.slow_acquires.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record an acquire error (timeout, pool exhaustion).
    #[inline]
    pub fn record_error(&self) {
        self.acquire_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Take a snapshot of current statistics.
    ///
    /// Reader method that loads all counters. Individual loads are relaxed,
    /// so values may be slightly stale but are always monotonically increasing.
    pub fn snapshot(&self) -> PoolStatsSnapshot {
        let total_acquires = self.total_acquires.load(Ordering::Relaxed);
        let total_acquire_time_us = self.total_acquire_time_us.load(Ordering::Relaxed);
        let slow_acquires = self.slow_acquires.load(Ordering::Relaxed);
        let max_acquire_time_us = self.max_acquire_time_us.load(Ordering::Relaxed);
        let acquire_errors = self.acquire_errors.load(Ordering::Relaxed);

        let average_acquire_time_us = if total_acquires > 0 {
            total_acquire_time_us as f64 / total_acquires as f64
        } else {
            0.0
        };

        PoolStatsSnapshot {
            pool_name: self.pool_name.clone(),
            max_connections: self.max_connections,
            total_acquires,
            slow_acquires,
            acquire_errors,
            average_acquire_time_us,
            max_acquire_time_us,
        }
    }
}

/// Point-in-time snapshot of pool statistics.
///
/// Plain DTO for serialization and display. Produced by `AtomicPoolStats::snapshot()`.
#[derive(Debug, Clone, Serialize)]
pub struct PoolStatsSnapshot {
    pub pool_name: String,
    pub max_connections: u32,
    pub total_acquires: u64,
    pub slow_acquires: u64,
    pub acquire_errors: u64,
    pub average_acquire_time_us: f64,
    pub max_acquire_time_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_acquire_increments_counters() {
        let stats = AtomicPoolStats::new("test".to_string(), 10);

        stats.record_acquire(500, 1000);
        stats.record_acquire(800, 1000);

        let snap = stats.snapshot();
        assert_eq!(snap.total_acquires, 2);
        assert_eq!(snap.slow_acquires, 0);
        assert_eq!(snap.max_acquire_time_us, 800);
    }

    #[test]
    fn test_slow_threshold_tracking() {
        let stats = AtomicPoolStats::new("test".to_string(), 10);

        stats.record_acquire(500, 1000); // not slow
        stats.record_acquire(1500, 1000); // slow
        stats.record_acquire(1000, 1000); // exactly at threshold = slow

        let snap = stats.snapshot();
        assert_eq!(snap.total_acquires, 3);
        assert_eq!(snap.slow_acquires, 2);
    }

    #[test]
    fn test_max_tracking() {
        let stats = AtomicPoolStats::new("test".to_string(), 10);

        stats.record_acquire(100, 1000);
        stats.record_acquire(5000, 1000);
        stats.record_acquire(200, 1000);

        let snap = stats.snapshot();
        assert_eq!(snap.max_acquire_time_us, 5000);
    }

    #[test]
    fn test_average_computation() {
        let stats = AtomicPoolStats::new("test".to_string(), 10);

        stats.record_acquire(100, 1000);
        stats.record_acquire(200, 1000);
        stats.record_acquire(300, 1000);

        let snap = stats.snapshot();
        assert!((snap.average_acquire_time_us - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_average_with_no_acquires() {
        let stats = AtomicPoolStats::new("test".to_string(), 10);
        let snap = stats.snapshot();
        assert_eq!(snap.average_acquire_time_us, 0.0);
    }

    #[test]
    fn test_error_counter() {
        let stats = AtomicPoolStats::new("test".to_string(), 10);

        stats.record_error();
        stats.record_error();

        let snap = stats.snapshot();
        assert_eq!(snap.acquire_errors, 2);
    }

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AtomicPoolStats>();
    }

    #[test]
    fn test_snapshot_includes_pool_metadata() {
        let stats = AtomicPoolStats::new("tasker".to_string(), 25);
        let snap = stats.snapshot();
        assert_eq!(snap.pool_name, "tasker");
        assert_eq!(snap.max_connections, 25);
    }
}
