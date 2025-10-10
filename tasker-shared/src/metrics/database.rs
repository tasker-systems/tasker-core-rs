//! # Database Metrics (TAS-29 Phase 3.3)
//!
//! OpenTelemetry metrics for database operations including:
//! - SQL query duration histograms
//! - Query execution counters
//! - Connection pool metrics
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics::database::*;
//! use opentelemetry::KeyValue;
//!
//! // Record SQL query execution
//! let start = std::time::Instant::now();
//! // ... execute query ...
//! let duration_ms = start.elapsed().as_millis() as f64;
//!
//! sql_query_duration().record(
//!     duration_ms,
//!     &[
//!         KeyValue::new("query_type", "get_next_ready_tasks"),
//!         KeyValue::new("result", "success"),
//!     ],
//! );
//! ```

use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};
use std::sync::OnceLock;

/// Lazy-initialized meter for database metrics
static DATABASE_METER: OnceLock<Meter> = OnceLock::new();

/// Get or initialize the database meter
fn meter() -> &'static Meter {
    DATABASE_METER.get_or_init(|| opentelemetry::global::meter_provider().meter("tasker-database"))
}

// Counters

/// Total number of SQL queries executed
///
/// Labels:
/// - query_type: get_next_ready_tasks, transition_task_state_atomic, etc.
/// - result: success, error
pub fn sql_queries_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.sql.queries.total")
        .with_description("Total number of SQL queries executed")
        .init()
}

/// Total number of database connection pool checkouts
///
/// Labels:
/// - pool: orchestration, worker
pub fn db_pool_checkouts_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.db.pool.checkouts.total")
        .with_description("Total number of database connection pool checkouts")
        .init()
}

/// Total number of database connection errors
///
/// Labels:
/// - error_type: timeout, connection_refused, pool_exhausted
pub fn db_connection_errors_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.db.connection.errors.total")
        .with_description("Total number of database connection errors")
        .init()
}

// Histograms

/// SQL query execution duration in milliseconds
///
/// Tracks end-to-end query execution time.
///
/// Labels:
/// - query_type: get_next_ready_tasks, transition_task_state_atomic, etc.
/// - result: success, error
pub fn sql_query_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.sql.query.duration")
        .with_description("SQL query execution duration in milliseconds")
        .with_unit("ms")
        .init()
}

/// Database transaction duration in milliseconds
///
/// Tracks full transaction execution time.
///
/// Labels:
/// - transaction_type: task_initialization, step_finalization, etc.
/// - result: commit, rollback
pub fn db_transaction_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.db.transaction.duration")
        .with_description("Database transaction duration in milliseconds")
        .with_unit("ms")
        .init()
}

/// Connection pool checkout duration in milliseconds
///
/// Tracks time waiting for connection from pool.
///
/// Labels:
/// - pool: orchestration, worker
pub fn db_pool_checkout_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.db.pool.checkout.duration")
        .with_description("Connection pool checkout duration in milliseconds")
        .with_unit("ms")
        .init()
}

// Gauges

/// Current database connection pool size
///
/// Labels:
/// - pool: orchestration, worker
/// - state: idle, active
pub fn db_pool_connections() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.db.pool.connections")
        .with_description("Current database connection pool size")
        .init()
}

// Static instances for convenience

/// Static counter: sql_queries_total
pub static SQL_QUERIES_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: db_pool_checkouts_total
pub static DB_POOL_CHECKOUTS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: db_connection_errors_total
pub static DB_CONNECTION_ERRORS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: sql_query_duration
pub static SQL_QUERY_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: db_transaction_duration
pub static DB_TRANSACTION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: db_pool_checkout_duration
pub static DB_POOL_CHECKOUT_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static gauge: db_pool_connections
pub static DB_POOL_CONNECTIONS: OnceLock<Gauge<u64>> = OnceLock::new();

/// Initialize all database metrics
///
/// This should be called during application startup after init_metrics().
pub fn init() {
    SQL_QUERIES_TOTAL.get_or_init(sql_queries_total);
    DB_POOL_CHECKOUTS_TOTAL.get_or_init(db_pool_checkouts_total);
    DB_CONNECTION_ERRORS_TOTAL.get_or_init(db_connection_errors_total);
    SQL_QUERY_DURATION.get_or_init(sql_query_duration);
    DB_TRANSACTION_DURATION.get_or_init(db_transaction_duration);
    DB_POOL_CHECKOUT_DURATION.get_or_init(db_pool_checkout_duration);
    DB_POOL_CONNECTIONS.get_or_init(db_pool_connections);
}
